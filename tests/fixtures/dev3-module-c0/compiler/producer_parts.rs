#![allow(dead_code)]
use crate::storage::disk::{
    RowInputPlan, RowOperation, RowPageRole, RowProducerContext, RowProducerInput, RowProducerLane,
    RowProposal, RowSourceRole, RowValidationLane,
};

fn validation(lane: RowValidationLane<'_>) -> RowInputPlan {
    let (context, schema, leaf, candidate) = lane.parts();
    std::hint::black_box((
        context.profile(),
        context.namespace(),
        context.table(),
        context.catalog_expected(),
        context.schema_cell(),
        context.leaf_expected(),
        context.leaf_free_bytes(),
        context.canonical_capacity(),
        context.index_count(),
        context.catalog_source(),
        schema,
        leaf,
    ));
    candidate[0] = 7;
    RowInputPlan::new(5, context.schema_epoch(), [9; 32])
}

fn context_fields(context: &RowProducerContext) {
    std::hint::black_box((
        context.schema(),
        context.operation(),
        context.frame_expected(),
        context.header(),
        context.actor(),
        context.status(),
        context.row(),
        context.undo(),
        context.statement_sequence(),
        context.planned_mutation(),
        context.statement_mark(),
        context.predecessor(),
        context.root_job_ordinal(),
        context.terminal_job_ordinal(),
        context.before_count(),
        context.after_order(),
    ));
    for role in [
        RowSourceRole::Root,
        RowSourceRole::RootJob,
        RowSourceRole::TerminalJob,
        RowSourceRole::Status,
        RowSourceRole::Undo,
        RowSourceRole::RowLeaf,
        RowSourceRole::Allocator,
        RowSourceRole::TableAnchor,
        RowSourceRole::PreviousUndo,
    ] {
        if let Some(source) = context.source(role) {
            std::hint::black_box((source.before_index(), source.expected(), source.cell()));
        }
    }
    match context.input() {
        RowProducerInput::Insert {
            canonical_len,
            canonical_digest,
        } => {
            std::hint::black_box((canonical_len, canonical_digest));
        }
        RowProducerInput::Clr {
            current_row_len,
            current_projection,
            rollback_target,
            next,
        } => {
            std::hint::black_box((current_row_len, current_projection, rollback_target, next));
        }
    }
}

fn producer<'a>(lane: RowProducerLane<'a>) -> (&'a [u8; 1024], &'a mut [u8; 16384], RowProposal) {
    let (context, control, before, schema, frame, after, candidate) = lane.parts();
    context_fields(&context);
    std::hint::black_box((control.transaction(), control.aging(), before, schema));
    frame[0] = 1;
    after[0][0] = 2;
    candidate[0] = 3;
    let roles = [
        RowPageRole::Root,
        RowPageRole::RootJob,
        RowPageRole::Status,
        RowPageRole::Undo,
        RowPageRole::RowLeaf,
    ];
    let proposal = RowProposal::new(RowOperation::Forward, 1, 5, roles);
    std::hint::black_box(RowProposal::new(
        RowOperation::Clr,
        usize::MAX,
        usize::MAX,
        roles,
    ));
    (&control.jobs()[0], candidate, proposal)
}
