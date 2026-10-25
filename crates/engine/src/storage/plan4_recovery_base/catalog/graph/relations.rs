use super::*;

fn actor_bound(actor: TxnRef, h: &Header) -> Result<()> {
    if actor.id().writer_epoch > h.source_epoch
        || actor.id().writer_epoch == h.source_epoch && actor.id().local > h.source_local
    {
        return Err(Error::OrderMismatch);
    }
    Ok(())
}

fn terminal(graph: &Graph<'_>, job: queue::JobView<'_>) -> Result<()> {
    let home = graph.home(job)?;
    let TransactionStatusCell::Occupied(status) = home.core() else {
        return Err(Error::IdentityMismatch);
    };
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    actor_bound(actor, &graph.layout.header())?;
    let root = graph.jobs.for_actor(actor, false)?;
    semantic_catalog::ledger::transaction_remaining(status, root)?;
    if home.incoming()
        != u64::from(root.is_some_and(|j| j.state() == queue::JobState::RootRetained))
    {
        return Err(Error::IdentityMismatch);
    }
    let valid = match job.state() {
        queue::JobState::TerminalLeased => matches!(
            status.state(),
            TransactionState::Active
                | TransactionState::Aborting(RollbackState::Transaction { .. })
        ),
        queue::JobState::TerminalReady => match status.state() {
            TransactionState::Committed(q) => {
                job.outcome() == 1 && q.get() == job.commit_q() && q.get() <= graph.root.commit_c()
            }
            TransactionState::Aborted => job.outcome() == 2,
            _ => false,
        },
        _ => false,
    };
    if !valid {
        return Err(Error::OrderMismatch);
    }
    if job.state() == queue::JobState::TerminalReady {
        if job.outcome() == 1 {
            for index in 0..JOB_COUNT {
                let other = graph.jobs.job(index)?;
                if other.state() == queue::JobState::TerminalReady
                    && other.outcome() == 1
                    && other.source() != Some(actor)
                    && other.commit_q() == job.commit_q()
                {
                    return Err(Error::OrderMismatch);
                }
            }
        }
        let (_, header) = graph.page(
            job.status().expect("terminal").page(),
            PageKind::TransactionStatus,
            2,
        )?;
        if header.lsn.0 < job.adoption_lsn() {
            return Err(Error::OrderMismatch);
        }
    }
    Ok(())
}

fn root_job(graph: &Graph<'_>, job: queue::JobView<'_>) -> Result<()> {
    let actor = job.source().ok_or(Error::IdentityMismatch)?;
    let undo = job.undo().ok_or(Error::IdentityMismatch)?;
    let object = job.object().ok_or(Error::IdentityMismatch)?;
    let (bytes, header) = graph.page(undo.page(), PageKind::Undo, 3)?;
    let page = page::decode_page(bytes, header.identity, PageKind::Undo)?;
    if page.len() != 1 || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL) {
        return Err(Error::IdentityMismatch);
    }
    let raw = page.cell(0).ok_or(Error::Corrupt)?;
    if job.state() == queue::JobState::RootReserved {
        undo_record::decode_free_undo_record(raw, undo.generation())?;
        if graph.record(object.object)?.is_some() || header.lsn.0 != job.lease_lsn() {
            return Err(Error::OrderMismatch);
        }
        let allocation = allocation_page::decode_allocation_page(
            image(graph.layout.bytes, 0),
            identity(graph.layout.expected, 0, one(), 1),
            graph.layout.expected.profile.spec().namespace,
        )?;
        for reference in job.pair().ok_or(Error::IdentityMismatch)? {
            if !matches!(allocation.entry(reference.page_no as usize)?, AllocationEntry::Reserved { generation, claimant }
                if generation == reference.incarnation && claimant == ReservationOwner::Transaction(actor))
            {
                return Err(Error::IdentityMismatch);
            }
        }
        if allocation.header().lsn.0 != job.lease_lsn()
            || job.lease_lsn() != graph.layout.header().capture_lsn
        {
            return Err(Error::OrderMismatch);
        }
        return Ok(());
    }
    let target = UndoTargetView {
        kind: UndoTargetKind::Catalog,
        owner: ObjectId::new(5).expect("Catalog"),
        generation: one(),
        entity: object.object.get(),
        arity: 0,
        key: &[],
    };
    let decoded = undo_record::decode_undo_record(
        raw,
        UndoRecordExpectation {
            namespace: graph.layout.expected.profile.spec().namespace,
            reference: undo,
            target,
        },
        &[],
    )?;
    let metadata = decoded.metadata()?;
    if metadata.creator.id != actor.id()
        || metadata.creator.status_generation != actor.generation()
        || metadata.sequence != job.undo_sequence()
        || metadata.mutation_sequence != 1
        || raw.len() != undo_record::UNDO_RECORD_BYTES
        || raw[324] != 0
        || raw[325] != 0
        || !zero(&raw[216..320])
        || header.lsn.0 < job.last_step()
    {
        return Err(Error::IdentityMismatch);
    }
    if job.state() == queue::JobState::RootRetained {
        let (record, value) = graph
            .record(object.object)?
            .ok_or(Error::IdentityMismatch)?;
        let current = record.header();
        if decoded.state() != UndoRecordState::Live
            || metadata.incoming != 2
            || raw[326..328] != [0, 0]
            || current.identity != object
            || current.stamp != VersionStamp::Transaction(actor)
            || current.undo != Some(undo)
            || current.logical.transaction() != actor.id()
            || current.logical.mutation_sequence() != job.mutation()
            || current.anchor != job.pair().ok_or(Error::IdentityMismatch)?[0]
            || value.len() + 12 != job.catalog_bytes() as usize
            || semantic_catalog::catalog_projection_digest(
                graph.layout.expected.profile.spec().namespace,
                object.object,
                Some(value),
            )? != *job.projection_digest()
        {
            return Err(Error::IdentityMismatch);
        }
    } else {
        let progress = decoded.release_progress()?;
        let cursor = job.release_cursor();
        let edges = match cursor {
            0 => [0, 0],
            1 => [0, 3],
            2 | 3 => [3, 3],
            _ => return Err(Error::OrderMismatch),
        };
        if decoded.state() != UndoRecordState::Releasing
            || metadata.incoming != 0
            || progress.progress.cursor != cursor
            || progress.chunk_count != 0
            || progress.progress.chunks_released != 0
            || progress.progress.next.is_some()
            || raw[326..328] != edges
        {
            return Err(Error::OrderMismatch);
        }
        match (job.state(), graph.record(object.object)?) {
            (queue::JobState::RootReleasingCommitted, Some((record, _))) => {
                let h = record.header();
                if !matches!(h.stamp, VersionStamp::Committed(q) if q.get() == job.commit_q())
                    || h.undo.is_some()
                    || h.identity != object
                    || h.logical.transaction() != actor.id()
                {
                    return Err(Error::IdentityMismatch);
                }
            }
            (queue::JobState::RootReleasingCompensated, None) => (),
            _ => return Err(Error::IdentityMismatch),
        }
    }
    Ok(())
}

fn catalog(graph: &Graph<'_>) -> Result<()> {
    let namespace = graph.layout.expected.profile.spec().namespace;
    let anchor = anchor::decode_anchor(
        image(graph.layout.bytes, 7),
        identity(graph.layout.expected, 7, one(), 5),
    )?;
    let root = anchor.root();
    if root.tree != TreeKind::Catalog
        || root.page != identity(graph.layout.expected, 8, one(), 5).page
        || root.level != 0
        || root.revision != 1
    {
        return Err(Error::IdentityMismatch);
    }
    anchor.decode_root(image(graph.layout.bytes, 8))?;
    let tree = graph.catalog()?;
    for index in 0..tree.len() {
        let entry = tree.entry(index)?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
        let object = ObjectId::new(u64::from_be_bytes(key)).map_err(|_| Error::IdentityMismatch)?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(Error::Corrupt);
        };
        let record =
            catalog_record::decode_catalog_record(value, CatalogExpectation { namespace, object })?;
        let h = record.header();
        if object.get() < 6
            || object.get() > graph.root.object_high()
            || h.identity.generation != one()
            || h.identity.schema_epoch.get() != 1
            || h.retired
            || h.logical.mutation_sequence() != 1
            || !matches!(record.definition(), CatalogDefinitionView::Table(_))
        {
            return Err(Error::Unsupported);
        }
        let provenance = graph
            .layout
            .provenance(h.anchor.page_no as usize)?
            .ok_or(Error::ProvenanceMismatch)?;
        let pair = provenance.pair().ok_or(Error::ProvenanceMismatch)?;
        let ReservationOwner::Transaction(actor) = provenance.source() else {
            return Err(Error::ProvenanceMismatch);
        };
        if provenance.role() != 3
            || pair[0] != h.anchor
            || provenance.object() != Some(h.identity)
            || h.logical.transaction() != actor.id()
        {
            return Err(Error::ProvenanceMismatch);
        }
        let (anchor_bytes, anchor_header) =
            graph.page(pair[0], PageKind::TreeAnchor, object.get())?;
        let actual_anchor = anchor::decode_anchor(anchor_bytes, anchor_header.identity)?;
        let root = actual_anchor.root();
        if root.tree != TreeKind::Row
            || root.page != pair[1]
            || root.level != 0
            || root.revision != 1
        {
            return Err(Error::IdentityMismatch);
        }
        let (row_bytes, row_header) = graph.page(pair[1], PageKind::RowLeaf, object.get())?;
        actual_anchor.decode_root(row_bytes)?;
        let row = tree_page::decode_tree_page(
            row_bytes,
            TreeExpectation {
                identity: row_header.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )?;
        if row.len() != 0 {
            return Err(Error::Unsupported);
        }
        // One independent CREATE can publish at most one Catalog object.
        // Distinct committed actors must also have distinct coordinator q.
        for prior in 0..index {
            let previous = tree.entry(prior)?;
            let previous_key: [u8; 8] = previous.key.try_into().map_err(|_| Error::Corrupt)?;
            let EntryPayload::Value(previous_value) = previous.payload else {
                return Err(Error::Corrupt);
            };
            let previous = catalog_record::decode_catalog_record(
                previous_value,
                CatalogExpectation {
                    namespace,
                    object: ObjectId::new(u64::from_be_bytes(previous_key))
                        .map_err(|_| Error::IdentityMismatch)?,
                },
            )?
            .header();
            if previous.logical.transaction() == actor.id()
                || matches!((previous.stamp, h.stamp),
                    (VersionStamp::Committed(a), VersionStamp::Committed(b)) if a == b)
            {
                return Err(Error::OrderMismatch);
            }
        }
        match h.stamp {
            VersionStamp::Transaction(stamped) => {
                let job = graph
                    .jobs
                    .for_actor(stamped, false)?
                    .ok_or(Error::IdentityMismatch)?;
                if actor != stamped
                    || job.state() != queue::JobState::RootRetained
                    || job.object() != Some(h.identity)
                    || job.undo() != h.undo
                    || job.pair() != Some(pair)
                {
                    return Err(Error::IdentityMismatch);
                }
            }
            VersionStamp::Committed(q) => {
                if q.get() > graph.root.commit_c() || h.undo.is_some() {
                    return Err(Error::OrderMismatch);
                }
                for index in 0..JOB_COUNT {
                    let terminal = graph.jobs.job(index)?;
                    if !terminal.state().is_terminal() {
                        continue;
                    }
                    let other = terminal.source().expect("terminal");
                    if other.id() == actor.id() {
                        if other != actor
                            || terminal.state() != queue::JobState::TerminalReady
                            || terminal.outcome() != 1
                            || terminal.commit_q() != q.get()
                        {
                            return Err(Error::OrderMismatch);
                        }
                    } else if terminal.state() == queue::JobState::TerminalReady
                        && terminal.outcome() == 1
                        && terminal.commit_q() == q.get()
                    {
                        return Err(Error::OrderMismatch);
                    }
                }
                if let Some(job) = graph.jobs.for_actor(actor, false)? {
                    if job.state() != queue::JobState::RootReleasingCommitted
                        || job.object() != Some(h.identity)
                        || job.commit_q() != q.get()
                    {
                        return Err(Error::IdentityMismatch);
                    }
                }
            }
        }
    }
    Ok(())
}

fn payloads(graph: &Graph<'_>, begins: u64) -> Result<()> {
    for ordinal in 9..64 {
        let Some(cert) = graph.layout.certificate(ordinal)? else {
            continue;
        };
        let bytes = graph
            .layout
            .image(ordinal)?
            .ok_or(Error::IdentityMismatch)?;
        let provenance = graph
            .layout
            .provenance(ordinal)?
            .ok_or(Error::ProvenanceMismatch)?;
        let page = page::decode_page(bytes, cert.header.identity, cert.header.kind)?;
        match cert.header.kind {
            PageKind::TransactionStatus => {
                let homes = status_home::decode_status_home_page(
                    bytes,
                    cert.header.identity,
                    graph.layout.expected.profile.spec().namespace,
                )?;
                if homes.len() != 1 {
                    return Err(Error::Unsupported);
                }
                let home = homes.cell(0)?;
                match home.core() {
                    TransactionStatusCell::Free(g) => {
                        if g.get() - 1 > begins
                            || g == one() && cert.header.lsn.0 != provenance.init_lsn()
                        {
                            return Err(Error::OrderMismatch);
                        }
                        for index in 0..JOB_COUNT {
                            if graph
                                .jobs
                                .job(index)?
                                .status()
                                .is_some_and(|s| s.page().page_no == ordinal as u64)
                            {
                                return Err(Error::IdentityMismatch);
                            }
                        }
                    }
                    TransactionStatusCell::Occupied(status) => {
                        let job = graph
                            .jobs
                            .for_actor(status.reference(), true)?
                            .ok_or(Error::IdentityMismatch)?;
                        if job.status().expect("terminal").page() != cert.header.identity.page
                            || status.reference().generation().get() - 1 > begins
                        {
                            return Err(Error::IdentityMismatch);
                        }
                    }
                }
            }
            PageKind::Undo => {
                if page.len() != 1 || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL) {
                    return Err(Error::Unsupported);
                }
                let raw = page.cell(0).ok_or(Error::Corrupt)?;
                if raw.len() != undo_record::UNDO_RECORD_BYTES {
                    return Err(Error::Unsupported);
                }
                let generation = Generation::new(get(raw, 8)).map_err(|_| Error::Corrupt)?;
                if generation.get() - 1 > graph.root.undo_high() {
                    return Err(Error::OrderMismatch);
                }
                let mut root = None;
                for index in 0..JOB_COUNT {
                    let job = graph.jobs.job(index)?;
                    if job
                        .undo()
                        .is_some_and(|u| u.page().page_no == ordinal as u64)
                    {
                        if root.is_some()
                            || job.undo().expect("root").page() != cert.header.identity.page
                            || job.undo().expect("root").generation() != generation
                        {
                            return Err(Error::IdentityMismatch);
                        }
                        root = Some(job);
                    }
                }
                if raw[6] == 0 {
                    undo_record::decode_free_undo_record(raw, generation)?;
                    if root.is_some_and(|j| j.state() != queue::JobState::RootReserved)
                        || generation == one() && cert.header.lsn.0 != provenance.init_lsn()
                    {
                        return Err(Error::OrderMismatch);
                    }
                } else if root.is_none() {
                    return Err(Error::IdentityMismatch);
                }
            }
            PageKind::TreeAnchor | PageKind::RowLeaf => {
                let object = cert.header.identity.owner;
                let (record, _) = graph.record(object)?.ok_or(Error::IdentityMismatch)?;
                let pair = provenance.pair().ok_or(Error::ProvenanceMismatch)?;
                if provenance.object() != Some(record.header().identity)
                    || record.header().anchor != pair[0]
                    || pair[usize::from(cert.header.kind == PageKind::RowLeaf)]
                        != cert.header.identity.page
                {
                    return Err(Error::IdentityMismatch);
                }
            }
            _ => return Err(Error::Unsupported),
        }
    }
    Ok(())
}

pub(super) fn validate(graph: &Graph<'_>) -> Result<ReservedInventory> {
    // Current CREATE-only grammar issues one Object and one UndoSequence per
    // Reserve, including canceled attempts. This is a necessary arithmetic
    // constraint; only original capture provenance proves the actual history.
    let creates = graph.root.object_high() - 5; // Root codec checked >= 5.
    let begins = graph.root.issued().checked_sub(graph.root.undo_high());
    if graph.root.undo_high() != creates
        || graph.root.canceled() > creates
        || begins.is_none_or(|count| graph.root.commit_c() > count)
    {
        return Err(Error::OrderMismatch);
    }
    for index in 0..JOB_COUNT {
        let job = graph.jobs.job(index)?;
        if job.generation().get() - 1 > graph.root.issued() {
            return Err(Error::OrderMismatch);
        }
        if job.state().is_terminal() {
            terminal(graph, job)?;
        } else if job.state().is_root() {
            root_job(graph, job)?;
        }
    }
    catalog(graph)?;
    payloads(graph, begins.ok_or(Error::OrderMismatch)?)?;
    let allocation = allocation_page::decode_allocation_page(
        image(graph.layout.bytes, 0),
        identity(graph.layout.expected, 0, one(), 1),
        graph.layout.expected.profile.spec().namespace,
    )?;
    let mut system = 0;
    let mut tx = 0;
    let generations = u128::from(graph.layout.header().system_high_water[0]) + u128::from(creates);
    for ordinal in 9..64 {
        let entry = allocation.entry(ordinal)?;
        if u128::from(entry.incarnation().get() - 1) > generations {
            return Err(Error::OrderMismatch);
        }
        if let AllocationEntry::Reserved {
            generation,
            claimant,
        } = entry
        {
            if generation.get() < 2 {
                return Err(Error::IdentityMismatch);
            }
            match claimant {
                ReservationOwner::System(_) => system += 1,
                ReservationOwner::Transaction(actor) => {
                    tx += 1;
                    let job = graph
                        .jobs
                        .for_actor(actor, false)?
                        .ok_or(Error::IdentityMismatch)?;
                    if job.state() != queue::JobState::RootReserved
                        || !job
                            .pair()
                            .ok_or(Error::IdentityMismatch)?
                            .iter()
                            .any(|p| p.page_no == ordinal as u64 && p.incarnation == generation)
                    {
                        return Err(Error::IdentityMismatch);
                    }
                }
            }
        }
    }
    let roots = (0..JOB_COUNT).try_fold(0, |count, index| {
        Ok::<_, Error>(
            count + usize::from(graph.jobs.job(index)?.state() == queue::JobState::RootReserved),
        )
    })?;
    let reserved = match (system, tx, roots) {
        (0, 0, 0) => ReservedInventory::None,
        (1, 0, 0) => ReservedInventory::SystemPage,
        (0, 2, 1) => ReservedInventory::CatalogCreate,
        _ => {
            return Err(Error::Inventory(
                crate::storage::ownership::InventoryError::Corrupt,
            ));
        }
    };
    if reserved != ReservedInventory::None
        && allocation.header().lsn.0 != graph.layout.header().capture_lsn
    {
        return Err(Error::OrderMismatch);
    }
    Ok(reserved)
}

pub(super) fn provenance(graph: &Graph<'_>, output: &mut [u8; PAGE_BYTES]) -> Result<()> {
    let header = graph.layout.header();
    for ordinal in 9..64 {
        let Some(provenance) = graph.layout.provenance(ordinal)? else {
            continue;
        };
        let cert = graph
            .layout
            .certificate(ordinal)?
            .ok_or(Error::ProvenanceMismatch)?;
        if provenance.writer_epoch() > header.source_epoch
            || provenance.init_lsn() > header.capture_lsn
        {
            return Err(Error::ProvenanceMismatch);
        }
        let initial = PageHeader {
            lsn: crate::storage::page::PageLsn(provenance.init_lsn()),
            ..cert.header
        };
        match provenance.source() {
            ReservationOwner::System(source) => {
                if source.id() > header.system_high_water[0] {
                    return Err(Error::ProvenanceMismatch);
                }
                match initial.kind {
                    PageKind::TransactionStatus => {
                        let mut cell = [0; 224];
                        StatusHome::new(TransactionStatusCell::Free(one()), 0)?
                            .encode(&mut cell)?;
                        page::encode_page(initial, &[&cell], output)?;
                    }
                    PageKind::Undo => undo_record::encode_initial_undo_page(
                        initial.identity,
                        initial.lsn,
                        output,
                    )?,
                    _ => return Err(Error::ProvenanceMismatch),
                }
            }
            ReservationOwner::Transaction(actor) => {
                actor_bound(actor, &header)?;
                let pair = provenance.pair().ok_or(Error::ProvenanceMismatch)?;
                if provenance.role() == 3 {
                    anchor::encode_anchor(
                        initial,
                        anchor::AnchorRoot {
                            tree: TreeKind::Row,
                            page: pair[1],
                            level: 0,
                            revision: 1,
                        },
                        output,
                    )?;
                } else {
                    let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
                    let cell =
                        tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut metadata)?;
                    tree_page::encode_tree_page(initial, 0, &[cell], output)?;
                }
            }
        }
        if content_digest(output) != *provenance.initial_digest() {
            return Err(Error::ProvenanceMismatch);
        }
        // Canonical initial bytes are necessary shape evidence only. Whether
        // this INIT occurred remains an original accepted-source obligation.
    }
    Ok(())
}

pub(super) fn source_bytes() -> usize {
    use std::mem::size_of;
    432 + 224
        + 32
        + 2048
        + 2 * size_of::<u128>()
        + 4 * size_of::<Header>()
        + 8 * size_of::<queue::JobView<'_>>()
        + 4 * size_of::<StatusHome>()
        + 4 * size_of::<TreePageView<'_>>()
        + 4 * size_of::<CatalogRecordView<'_>>()
        + 4 * size_of::<Result<Option<(CatalogRecordView<'_>, &[u8])>>>()
        + 4 * size_of::<undo_record::UndoRecordView<'_>>()
        + 4 * size_of::<undo_record::UndoRecordMetadata<'_>>()
        + 4 * size_of::<UndoRecordExpectation<'_>>()
        + 4 * size_of::<undo_record::UndoRecord<'_>>()
        + 8 * size_of::<PageHeader>()
        + 8 * size_of::<Certificate>()
        + 4 * size_of::<InitProvenance<'_>>()
        + 8 * size_of::<Result<()>>()
        + 2 * size_of::<[RemainingBudget; 5]>()
}
