use super::*;

pub(in crate::storage) struct SnapshotReservation<'v, 'a, 'c, 's> {
    visibility: &'v BoundVisibility<'a, 'c, 's>,
    admission: usize,
    admission_generation: u64,
    row: usize,
    generation: u64,
    active: bool,
}
pub(in crate::storage) struct SnapshotGuard<'v, 'a, 'c, 's> {
    visibility: &'v BoundVisibility<'a, 'c, 's>,
    admission: usize,
    admission_generation: u64,
    row: usize,
    generation: u64,
    q: u64,
    active: bool,
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    pub(in crate::storage) fn reserve_snapshot(
        &self,
    ) -> VisibilityResult<SnapshotReservation<'_, 'a, 'c, 's>> {
        let mut bytes = self.entry()?;
        let admission = (0..USERS)
            .find(|&i| {
                admission_row(&bytes[..], i)[0] == 0
                    && (i * 2..i * 2 + 2).any(|index| {
                        let row = snapshot_row(&bytes[..], index);
                        row[0] == 0 && u64_at(row, 8) < u64::MAX
                    })
            })
            .ok_or_else(|| {
                unchanged(
                    if (0..USERS).any(|i| admission_row(&bytes[..], i)[0] == 1) {
                        VisibilityCause::Capacity
                    } else {
                        VisibilityCause::Exhausted
                    },
                )
            })?;
        let admission_generation = u64_at(admission_row(&bytes[..], admission), 8)
            .checked_add(1)
            .ok_or_else(|| unchanged(VisibilityCause::Exhausted))?;
        let row = (admission * 2..admission * 2 + 2)
            .find(|&i| {
                snapshot_row(&bytes[..], i)[0] == 0
                    && u64_at(snapshot_row(&bytes[..], i), 8) < u64::MAX
            })
            .ok_or_else(|| unchanged(VisibilityCause::Exhausted))?;
        let generation = u64_at(snapshot_row(&bytes[..], row), 8) + 1;
        let owner = admission_mut(&mut bytes[..], admission);
        owner[0] = 1;
        put_u64(owner, 8, admission_generation);
        reserve_row(&mut bytes[..], row, generation, admission);
        Ok(SnapshotReservation {
            visibility: self,
            admission,
            admission_generation,
            row,
            generation,
            active: true,
        })
    }

    pub(in crate::storage) fn register_current<'v>(
        &'v self,
        owner: &mut Owner<'c, 's>,
        mut reservation: SnapshotReservation<'v, 'a, 'c, 's>,
    ) -> VisibilityResult<SnapshotGuard<'v, 'a, 'c, 's>> {
        if !self.same_registry(reservation.visibility) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        if owner.visibility_identity(self.origin.core)?.index() >= USERS {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        let version = owner.try_apply()?.try_version()?;
        let registered = (|| {
            let q = self.publication()?.q;
            let mut bytes = self.entry()?;
            reservation.check(&bytes[..], 1)?;
            let row = snapshot_mut(&mut bytes[..], reservation.row);
            put_u64(row, 16, q);
            row[0] = 2;
            reservation.active = false;
            Ok(SnapshotGuard {
                visibility: self,
                admission: reservation.admission,
                admission_generation: reservation.admission_generation,
                row: reservation.row,
                generation: reservation.generation,
                q,
                active: true,
            })
        })();
        let release = version.release_checked().map_err(VisibilityError::from);
        match (registered, release) {
            (Ok(guard), Ok(())) => Ok(guard),
            (Err(error), Ok(())) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }
}

fn reserve_row(bytes: &mut [u8], index: usize, generation: u64, admission: usize) {
    let row = snapshot_mut(bytes, index);
    row.fill(0);
    row[0] = 1;
    put_u64(row, 8, generation);
    put_u64(row, 24, admission as u64);
}
fn check(
    visibility: &BoundVisibility<'_, '_, '_>,
    bytes: &[u8],
    admission: usize,
    admission_generation: u64,
    index: usize,
    generation: u64,
    state: u8,
) -> VisibilityResult<()> {
    let row = snapshot_row(bytes, index);
    let owner = admission_row(bytes, admission);
    if owner[0] != 1
        || u64_at(owner, 8) != admission_generation
        || row[0] != state
        || u64_at(row, 8) != generation
        || u64_at(row, 24) != admission as u64
    {
        return Err(visibility.origin.fatal(VisibilityCause::Invariant));
    }
    Ok(())
}
fn release(
    visibility: &BoundVisibility<'_, '_, '_>,
    admission: usize,
    admission_generation: u64,
    index: usize,
    generation: u64,
    state: u8,
) -> VisibilityResult<()> {
    let mut bytes = visibility.cleanup()?;
    check(
        visibility,
        &bytes[..],
        admission,
        admission_generation,
        index,
        generation,
        state,
    )?;
    let row = snapshot_mut(&mut bytes[..], index);
    row.fill(0);
    put_u64(row, 8, generation);
    row[0] = u8::from(generation == u64::MAX) * 3;
    if (admission * 2..admission * 2 + 2).all(|i| matches!(snapshot_row(&bytes[..], i)[0], 0 | 3)) {
        let owner = admission_mut(&mut bytes[..], admission);
        owner[0] = u8::from(admission_generation == u64::MAX) * 2;
    }
    Ok(())
}
impl SnapshotReservation<'_, '_, '_, '_> {
    fn check(&self, bytes: &[u8], state: u8) -> VisibilityResult<()> {
        check(
            self.visibility,
            bytes,
            self.admission,
            self.admission_generation,
            self.row,
            self.generation,
            state,
        )
    }
    pub(in crate::storage) fn cancel(mut self) -> VisibilityResult<()> {
        self.active = false;
        release(
            self.visibility,
            self.admission,
            self.admission_generation,
            self.row,
            self.generation,
            1,
        )
    }
}
impl Drop for SnapshotReservation<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let _ = release(
                self.visibility,
                self.admission,
                self.admission_generation,
                self.row,
                self.generation,
                1,
            );
        }
    }
}
impl<'v, 'a, 'c, 's> SnapshotGuard<'v, 'a, 'c, 's> {
    pub(in crate::storage) fn q(&self) -> u64 {
        self.q
    }
    pub(in crate::storage) fn confirms(
        &self,
        visibility: &BoundVisibility<'_, '_, '_>,
    ) -> VisibilityResult<bool> {
        if !self.visibility.same_registry(visibility) {
            return Ok(false);
        }
        let bytes = self.visibility.entry()?;
        check(
            self.visibility,
            &bytes[..],
            self.admission,
            self.admission_generation,
            self.row,
            self.generation,
            2,
        )?;
        if u64_at(snapshot_row(&bytes[..], self.row), 16) != self.q {
            return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
        }
        Ok(true)
    }
    /// Register the new owner while this exact old registration remains live.
    /// The caller explicitly releases the old guard only after this succeeds.
    pub(in crate::storage) fn handoff(
        &self,
        owner: &mut Owner<'c, 's>,
    ) -> VisibilityResult<SnapshotGuard<'v, 'a, 'c, 's>> {
        let visibility = self.visibility;
        let reservation = {
            let mut bytes = visibility.entry()?;
            check(
                visibility,
                &bytes[..],
                self.admission,
                self.admission_generation,
                self.row,
                self.generation,
                2,
            )?;
            let index = self.admission * 2 + usize::from(self.row == self.admission * 2);
            let other = snapshot_row(&bytes[..], index);
            if other[0] != 0 {
                return Err(unchanged(VisibilityCause::Capacity));
            }
            let generation = u64_at(other, 8)
                .checked_add(1)
                .ok_or_else(|| unchanged(VisibilityCause::Exhausted))?;
            reserve_row(&mut bytes[..], index, generation, self.admission);
            SnapshotReservation {
                visibility,
                admission: self.admission,
                admission_generation: self.admission_generation,
                row: index,
                generation,
                active: true,
            }
        };
        // Handoff preserves q. It is not ReadCommitted's next-statement renewal.
        if owner.visibility_identity(visibility.origin.core)?.index() >= USERS {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        let version = owner.try_apply()?.try_version()?;
        let mut reservation = reservation;
        let registered = (|| {
            let mut bytes = visibility.entry()?;
            check(
                visibility,
                &bytes[..],
                self.admission,
                self.admission_generation,
                self.row,
                self.generation,
                2,
            )?;
            reservation.check(&bytes[..], 1)?;
            let row = snapshot_mut(&mut bytes[..], reservation.row);
            row[0] = 2;
            put_u64(row, 16, self.q);
            reservation.active = false;
            Ok(Self {
                visibility,
                admission: self.admission,
                admission_generation: self.admission_generation,
                row: reservation.row,
                generation: reservation.generation,
                q: self.q,
                active: true,
            })
        })();
        let cleanup = version.release_checked().map_err(VisibilityError::from);
        match (registered, cleanup) {
            (Ok(guard), Ok(())) => Ok(guard),
            (Err(error), Ok(())) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }
    pub(in crate::storage) fn release_checked(mut self) -> VisibilityResult<()> {
        self.active = false;
        release(
            self.visibility,
            self.admission,
            self.admission_generation,
            self.row,
            self.generation,
            2,
        )
    }
}
impl Drop for SnapshotGuard<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            let _ = release(
                self.visibility,
                self.admission,
                self.admission_generation,
                self.row,
                self.generation,
                2,
            );
        }
    }
}

pub(super) fn validate(bytes: &[u8]) -> Result<(), ()> {
    for admission in 0..USERS {
        let row = admission_row(bytes, admission);
        let generation = u64_at(row, 8);
        if row[1..8].iter().any(|b| *b != 0) {
            return Err(());
        }
        match row[0] {
            0 if generation < u64::MAX => {}
            1 if generation != 0 => {}
            2 if generation == u64::MAX => {}
            _ => return Err(()),
        }
        let live =
            (admission * 2..admission * 2 + 2).any(|i| matches!(snapshot_row(bytes, i)[0], 1 | 2));
        if live != (row[0] == 1) {
            return Err(());
        }
    }
    for index in 0..SNAPSHOTS {
        let row = snapshot_row(bytes, index);
        let generation = u64_at(row, 8);
        if row[1..8].iter().any(|v| *v != 0) {
            return Err(());
        }
        match row[0] {
            0 if generation < u64::MAX && row[16..].iter().all(|v| *v == 0) => {}
            3 if generation == u64::MAX && row[16..].iter().all(|v| *v == 0) => {}
            1 | 2
                if generation != 0
                    && index < USERS * 2
                    && u64_at(row, 24) == (index / 2) as u64
                    && (row[0] != 1 || u64_at(row, 16) == 0) => {}
            _ => return Err(()),
        }
        // Checkpoint/backup rows remain unavailable until their actual producer.
        if index >= USERS * 2 && row.iter().any(|v| *v != 0) {
            return Err(());
        }
    }
    Ok(())
}
