//! Current immutable Row-tree observation. Catalog/schema and every page are
//! selected from one LayoutView; no caller-supplied record or live authority.
use super::super::{AllocationEntry, Error, Result, identity, one};
use super::{LayoutView, image};
use crate::storage::{
    allocation_page,
    anchor::{self, TreeKind},
    catalog_record::{
        self, CatalogDefinitionView, CatalogExpectation, CatalogRecordView, TableDefinitionView,
    },
    page::{PAGE_BYTES, PageIdentity, PageKind},
    tree_page::{self, EntryPayload, TreeError, TreeExpectation, TreePageView},
};
use foundation::identity::RowId;

pub(super) struct Table<'a> {
    layout: LayoutView<'a>,
    record: CatalogRecordView<'a>,
    root: TreeExpectation,
    leaves: u64,
}
impl<'a> Table<'a> {
    pub(super) fn record(&self) -> CatalogRecordView<'a> {
        self.record
    }
    pub(super) fn schema(&self) -> TableDefinitionView<'a> {
        let CatalogDefinitionView::Table(schema) = self.record.definition() else {
            unreachable!("observed table")
        };
        schema
    }
    pub(super) fn has_leaf(&self, ordinal: usize) -> bool {
        ordinal < 64 && self.leaves & (1u64 << ordinal) != 0
    }
    pub(super) fn leaf(&self, ordinal: usize) -> Result<TreePageView<'a>> {
        if !self.has_leaf(ordinal) {
            return Err(Error::InvalidInput);
        }
        let cert = self
            .layout
            .certificate(ordinal)?
            .ok_or(Error::IdentityMismatch)?;
        let expected = TreeExpectation {
            identity: cert.header.identity,
            kind: PageKind::RowLeaf,
            level: 0,
            arity: 0,
        };
        Ok(tree_page::decode_tree_page(
            read(self.layout, expected)?,
            expected,
        )?)
    }
    pub(super) fn find(&self, row: RowId) -> Result<TreePageView<'a>> {
        Ok(tree_page::search::find_leaf_in_images(
            self.root,
            &row.get().to_be_bytes(),
            |expected| {
                read(self.layout, expected)
                    .map(|bytes| &bytes[..])
                    .map_err(|_| TreeError::Corrupt)
            },
        )?)
    }
}

fn read<'a>(layout: LayoutView<'a>, expected: TreeExpectation) -> Result<&'a [u8; PAGE_BYTES]> {
    let page = expected.identity.page;
    if page.file != super::super::first_file() || !(9..64).contains(&page.page_no) {
        return Err(Error::IdentityMismatch);
    }
    let cert = layout
        .certificate(page.page_no as usize)?
        .ok_or(Error::IdentityMismatch)?;
    if cert.header.identity != expected.identity
        || cert.header.kind != expected.kind
        || cert.header.level != expected.level
    {
        return Err(Error::IdentityMismatch);
    }
    Ok(image(layout.bytes(), page.page_no as usize))
}

/// Verify global reachability and reverse allocation ownership before returning
/// the compact leaf set. The input remains immutably borrowed by the result.
pub(super) fn observe(layout: LayoutView<'_>, index: usize) -> Result<Table<'_>> {
    let expected = layout.expected;
    let namespace = expected.profile.spec().namespace;
    let catalog = tree_page::decode_tree_page(
        image(layout.bytes(), 8),
        TreeExpectation {
            identity: identity(expected, 8, one(), 5),
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )?;
    let entry = catalog.entry(index)?;
    let key: [u8; 8] = entry.key.try_into().map_err(|_| Error::Corrupt)?;
    let object = foundation::identity::ObjectId::new(u64::from_be_bytes(key))
        .map_err(|_| Error::IdentityMismatch)?;
    let EntryPayload::Value(value) = entry.payload else {
        return Err(Error::Unsupported);
    };
    let record =
        catalog_record::decode_catalog_record(value, CatalogExpectation { namespace, object })?;
    let h = record.header();
    if h.retired
        || !matches!(record.definition(), CatalogDefinitionView::Table(_))
        || h.identity.generation != one()
    {
        return Err(Error::Unsupported);
    }
    let anchor_expected = TreeExpectation {
        identity: PageIdentity {
            database: namespace.database,
            page: h.anchor,
            owner: object,
            owner_generation: h.identity.generation,
        },
        kind: PageKind::TreeAnchor,
        level: 0,
        arity: 0,
    };
    let anchor_bytes = read(layout, anchor_expected)?;
    let anchor = anchor::decode_anchor(anchor_bytes, anchor_expected.identity)?;
    let retired = anchor::retired(anchor_bytes, anchor_expected.identity)?;
    let root = anchor.root();
    let high = layout.header().system_high_water[1];
    if root.tree != TreeKind::Row || u128::from(root.revision) > 1 + u128::from(high) {
        return Err(Error::IdentityMismatch);
    }
    let anchor_lsn = layout
        .certificate(h.anchor.page_no as usize)?
        .ok_or(Error::IdentityMismatch)?
        .header
        .lsn
        .0;
    let root = TreeExpectation {
        identity: PageIdentity {
            page: root.page,
            ..anchor_expected.identity
        },
        kind: if root.level == 0 {
            PageKind::RowLeaf
        } else {
            PageKind::RowInternal
        },
        level: root.level,
        arity: 0,
    };
    let mut visited = [None; 64];
    let count = tree_page::audit::audit_tree_in_images(root, &mut visited, |expected| {
        read(layout, expected)
            .map(|bytes| &bytes[..])
            .map_err(|_| TreeError::Corrupt)
    })?;
    let mut reached = 0u64;
    let mut leaves = 0u64;
    for page in visited[..count].iter().flatten() {
        reached |= 1u64 << page.identity.page.page_no;
        if page.level == 0 {
            leaves |= 1u64 << page.identity.page.page_no;
        }
    }
    let allocation = allocation_page::decode_allocation_page(
        image(layout.bytes(), 0),
        identity(expected, 0, one(), 1),
        namespace,
    )?;
    let mut owned = 0u64;
    for ordinal in 9..64 {
        let AllocationEntry::Allocated {
            object: owner,
            object_generation,
            kind,
            generation,
        } = allocation.entry(ordinal)?
        else {
            continue;
        };
        if owner != object {
            continue;
        }
        if object_generation != h.identity.generation {
            return Err(Error::IdentityMismatch);
        }
        if kind == PageKind::TreeAnchor {
            if ordinal as u64 != h.anchor.page_no || generation != h.anchor.incarnation {
                return Err(Error::IdentityMismatch);
            }
            continue;
        }
        if !matches!(kind, PageKind::RowLeaf | PageKind::RowInternal) {
            return Err(Error::Unsupported);
        }
        owned |= 1u64 << ordinal;
        let claim = layout
            .provenance(ordinal)?
            .ok_or(Error::ProvenanceMismatch)?;
        if claim.object() != Some(h.identity) {
            return Err(Error::ProvenanceMismatch);
        }
        if let Some(birth) = claim.structural() {
            if birth.anchor() != h.anchor
                || birth.revisions()[1] > anchor.root().revision
                || claim.init_lsn() > anchor_lsn
            {
                return Err(Error::ProvenanceMismatch);
            }
        } else if claim.pair().is_none_or(|pair| pair[0] != h.anchor) {
            return Err(Error::ProvenanceMismatch);
        }
    }
    // dev3-tree-smo.md#empty-leaf-unlink: reached ⊎ retired == owned. Each
    // retired item is owned at its exact incarnation/kind and was last
    // written before it retired: an empty leaf or a valid (unfollowed)
    // internal page. Actor, graph and D use only the reached leaves.
    let mut listed = 0u64;
    for index in 0..retired.len() {
        let item = retired.get(index).ok_or(Error::Corrupt)?;
        let ordinal = item.page.page_no as usize;
        let AllocationEntry::Allocated {
            object: owner,
            generation,
            kind,
            ..
        } = allocation.entry(ordinal)?
        else {
            return Err(Error::IdentityMismatch);
        };
        if owner != object
            || generation != item.page.incarnation
            || kind != item.kind
            || reached & (1u64 << ordinal) != 0
        {
            return Err(Error::IdentityMismatch);
        }
        let expected = TreeExpectation {
            identity: PageIdentity {
                page: item.page,
                ..anchor_expected.identity
            },
            kind: item.kind,
            level: item.level,
            arity: 0,
        };
        let page = tree_page::decode_tree_page(read(layout, expected)?, expected)?;
        if page.header().lsn.0 >= item.retire_lsn
            || item.kind == PageKind::RowLeaf && page.len() != 0
        {
            return Err(Error::IdentityMismatch);
        }
        listed |= 1u64 << ordinal;
    }
    if reached | listed != owned {
        return Err(Error::IdentityMismatch);
    }
    Ok(Table {
        layout,
        record,
        root,
        leaves,
    })
}

/// Complete source-value bound of observe/find/leaf. The backing Base image
/// stays caller-owned; no page copy or allocation is hidden in this observer.
pub(super) fn source_bytes() -> usize {
    use super::super::{Certificate, Expected, Header, InitProvenance};
    use super::graph::source;
    use crate::storage::catalog_record::CatalogHeader;
    use std::mem::size_of;
    let read = size_of::<(LayoutView<'_>, TreeExpectation)>()
        + 2 * size_of::<Certificate>()
        + 2 * size_of::<Result<Option<Certificate>>>()
        + source::slack()
        + source::certificate_read();
    let frame = 3 * size_of::<(LayoutView<'_>, Expected, Header)>()
        + 3 * size_of::<TreePageView<'_>>()
        + 3 * size_of::<Result<TreePageView<'_>>>()
        + 3 * size_of::<(CatalogRecordView<'_>, CatalogHeader)>()
        + 3 * size_of::<anchor::TreeAnchor>()
        + 2 * size_of::<anchor::RetiredList<'_>>()
        + 2 * size_of::<anchor::RetiredPage>()
        + 3 * size_of::<TreeExpectation>()
        + 2 * size_of::<(InitProvenance<'_>, Result<Option<InitProvenance<'_>>>)>()
        + 2 * size_of::<allocation_page::AllocationPage<'_>>()
        + 2 * size_of::<AllocationEntry>()
        + 2 * size_of::<Table<'_>>()
        + 2 * size_of::<Result<Table<'_>>>()
        + size_of::<[Option<TreeExpectation>; 64]>()
        + 4 * source::slack();
    let visit = tree_page::audit::frame_source_bytes()
        .max(tree_page::search::frame_source_bytes())
        + size_of::<LayoutView<'_>>() // callback capture
        + read.max(source::record_codec());
    frame
        + visit
            .max(source::header_read())
            .max(source::allocation_decode())
            .max(super::super::image_records::provenance_source_bytes())
            // The anchor decode, then `retired` with its whole nested decode
            // (UNL-C01).
            .max(anchor::retired_source_bytes(source::page_codec()))
}
