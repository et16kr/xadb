use super::{
    ContentHasher, Generation, JOB_BYTES, JOB_COUNT, Namespace, PageRef, ROOT_BYTES, Result,
    RetentionError, RetentionGenesisPage, SystemWorkKind, SystemWorkRef, content_digest, free_job,
    get, header, put, zero,
};

#[derive(Clone, Copy)]
pub(crate) struct RootView<'a> {
    bytes: &'a [u8; ROOT_BYTES],
}
impl<'a> RootView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; ROOT_BYTES] {
        self.bytes
    }
    pub(crate) fn aging(self) -> Option<SystemWorkRef> {
        (self.bytes[6] == 1)
            .then(|| SystemWorkRef::decode(&self.bytes[16..64]).expect("validated Aging"))
    }
    pub(crate) fn revision(self) -> u64 {
        get(self.bytes, 64)
    }
    pub(crate) fn cursor(self) -> u64 {
        get(self.bytes, 72)
    }
    pub(crate) fn apply(self) -> u64 {
        get(self.bytes, 80)
    }
    pub(crate) fn issued(self) -> u64 {
        get(self.bytes, 88)
    }
    pub(crate) fn masks(self) -> [u64; 3] {
        [
            get(self.bytes, 96),
            get(self.bytes, 104),
            get(self.bytes, 112),
        ]
    }
    pub(crate) fn job_digest(self) -> &'a [u8; 32] {
        self.bytes[128..160].try_into().expect("checked field")
    }
    pub(crate) fn budget_digest(self) -> &'a [u8; 32] {
        self.bytes[160..192].try_into().expect("checked field")
    }
    pub(crate) fn completed(self) -> u64 {
        get(self.bytes, 376)
    }
    pub(crate) fn canceled(self) -> u64 {
        get(self.bytes, 384)
    }
    pub(crate) fn undo_high(self) -> u64 {
        get(self.bytes, 392)
    }
    pub(crate) fn object_high(self) -> u64 {
        get(self.bytes, 400)
    }
    pub(crate) fn commit_c(self) -> u64 {
        get(self.bytes, 408)
    }
    pub(crate) fn consumed_wal(self) -> u64 {
        get(self.bytes, 416)
    }
    pub(crate) fn consumed_control(self) -> u64 {
        get(self.bytes, 424)
    }
    pub(crate) fn non_free(self) -> usize {
        (self.masks()[1] | self.masks()[2]).count_ones() as usize
    }
}

pub(crate) fn initial_root(namespace: Namespace, output: &mut [u8; ROOT_BYTES]) -> Result<()> {
    if namespace.timeline != 1 {
        return Err(RetentionError::InvalidInput);
    }
    let mut bytes = [0; ROOT_BYTES];
    bytes[..4].copy_from_slice(b"XQR1");
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    bytes[8..12].copy_from_slice(&512u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&64u32.to_le_bytes());
    put(&mut bytes, 64, 1);
    put(&mut bytes, 96, u64::MAX);
    put(&mut bytes, 400, 5);
    bytes[128..160].copy_from_slice(&initial_jobs_digest());
    {
        let mut rows = [0; 640];
        for row in 0..5 {
            rows[row * 128] = row as u8 + 1;
        }
        bytes[160..192].copy_from_slice(&content_digest(&rows));
    }
    bytes[192..208].copy_from_slice(&namespace.database.bytes());
    for (at, value) in [(208, namespace.timeline), (216, 4), (224, 1)] {
        put(&mut bytes, at, value);
    }
    header(namespace, RetentionGenesisPage::Root, 0)?
        .identity
        .page
        .encode(&mut bytes[232..256])?;
    for index in 0..5 {
        header(namespace, RetentionGenesisPage::Jobs { index }, 0)?
            .identity
            .page
            .encode(&mut bytes[256 + usize::from(index) * 24..280 + usize::from(index) * 24])?;
    }
    output.copy_from_slice(&bytes);
    Ok(())
}

fn initial_jobs_digest() -> [u8; 32] {
    let mut job = [0; JOB_BYTES];
    free_job(Generation::new(1).expect("one"), &mut job);
    let mut hash = ContentHasher::new();
    for _ in 0..JOB_COUNT {
        hash.update(&job);
    }
    hash.digest()
}

pub(crate) fn encode_root(candidate: &[u8], namespace: Namespace, output: &mut [u8]) -> Result<()> {
    let root = decode_root(candidate, namespace)?;
    if output.len() != ROOT_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    output.copy_from_slice(root.bytes());
    Ok(())
}

pub(crate) fn decode_root(input: &[u8], namespace: Namespace) -> Result<RootView<'_>> {
    let bytes: &[u8; ROOT_BYTES] = input.try_into().map_err(|_| RetentionError::Corrupt)?;
    if &bytes[..4] != b"XQR1" {
        return Err(RetentionError::Corrupt);
    }
    if bytes[4..6] != [2, 0] {
        return Err(RetentionError::UnsupportedFormat);
    }
    if bytes[6] > 1 {
        return Err(RetentionError::UnsupportedState);
    }
    if namespace.timeline != 1 {
        return Err(RetentionError::InvalidInput);
    }
    if bytes[7] != 0
        || bytes[8..12] != 512u32.to_le_bytes()
        || bytes[12..16] != 64u32.to_le_bytes()
        || !zero(&bytes[120..128])
        || !zero(&bytes[432..512])
    {
        return Err(RetentionError::Corrupt);
    }
    if bytes[192..208] != namespace.database.bytes()
        || get(bytes, 208) != namespace.timeline
        || get(bytes, 216) != 4
        || get(bytes, 224) != 1
        || PageRef::decode(&bytes[232..256])?
            != header(namespace, RetentionGenesisPage::Root, 0)?
                .identity
                .page
    {
        return Err(RetentionError::IdentityMismatch);
    }
    for index in 0..5 {
        if PageRef::decode(&bytes[256 + usize::from(index) * 24..280 + usize::from(index) * 24])?
            != header(namespace, RetentionGenesisPage::Jobs { index }, 0)?
                .identity
                .page
        {
            return Err(RetentionError::IdentityMismatch);
        }
    }
    let root = RootView { bytes };
    let [free, leased, ready] = root.masks();
    if free & leased != 0
        || free & ready != 0
        || leased & ready != 0
        || free | leased | ready != u64::MAX
        || root.non_free() > 55
    {
        return Err(RetentionError::Corrupt);
    }
    if root.revision() == 0
        || root.cursor() >= 64
        || root.object_high() < 5
        || root
            .completed()
            .checked_add(root.canceled())
            .and_then(|v| v.checked_add(root.non_free() as u64))
            != Some(root.issued())
    {
        return Err(RetentionError::OrderMismatch);
    }
    if bytes[6] == 0 {
        let mut canonical = [0; ROOT_BYTES];
        initial_root(namespace, &mut canonical)?;
        if bytes != &canonical {
            return Err(RetentionError::Corrupt);
        }
    } else {
        let aging =
            SystemWorkRef::decode(&bytes[16..64]).map_err(|_| RetentionError::IdentityMismatch)?;
        if aging.namespace() != namespace
            || aging.kind() != SystemWorkKind::Aging
            || aging.id() != 1
            || aging.generation().get() != 1
        {
            return Err(RetentionError::IdentityMismatch);
        }
        if root.apply() == 0 {
            return Err(RetentionError::OrderMismatch);
        }
    }
    Ok(root)
}
