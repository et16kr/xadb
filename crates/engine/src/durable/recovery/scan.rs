//! Authenticate the selected predecessor closure and scan complete frame edges.
use super::{RecoveryPlan, RequiredFile, Segment, required_record, same_database, vector};
use crate::durable::{
    codec::{Identity, Link, Origin, Record, SEGMENT_BYTES},
    error::{DurableError as Error, Result},
    io::{self, FrameRead},
};
use xadb_foundation::{
    codec::content_digest,
    namespace::{OpenMode, WriterNamespace},
};

const MAX_SEGMENT_TRANSACTIONS: usize = 128 * 1024;

pub(super) fn inspect_chain<W: WriterNamespace>(
    plan: &mut RecoveryPlan,
    writer: &mut W,
    selected_segment: u64,
    selected_header: [u8; 32],
) -> Result<()> {
    let mut epoch = plan.identity.epoch;
    let mut number = selected_segment;
    loop {
        if plan.segments.len() >= plan.file_limit || plan.segments.len() == plan.segments.capacity()
        {
            return Err(Error::invalid(
                "WAL predecessors exceed frozen namespace inventory",
            ));
        }
        if plan
            .segments
            .iter()
            .any(|item| item.identity.epoch == epoch && item.header.segment == number)
        {
            return Err(Error::invalid("WAL predecessor loop"));
        }
        let name = io::wal_name(epoch, number)?;
        let metadata = writer
            .metadata(&name)
            .map_err(io::namespace)?
            .ok_or(Error::invalid("WAL predecessor missing"))?;
        if metadata.length > SEGMENT_BYTES {
            return Err(Error::limit("16 MiB WAL segment"));
        }
        let mut file = writer
            .open_existing(&name, OpenMode::ReadOnly)
            .map_err(io::namespace)?;
        let frame = match io::read_frame(&mut file, 0, metadata.length)? {
            FrameRead::Complete(frame) => frame,
            _ => return Err(Error::invalid("incomplete required WAL header")),
        };
        same_database(plan.identity, frame.identity)?;
        if frame.identity.epoch != epoch {
            return Err(Error::invalid("WAL filename epoch"));
        }
        let header_digest = content_digest(&frame.bytes);
        if plan.segments.is_empty() && header_digest != selected_header {
            return Err(Error::invalid("WRITER header digest"));
        }
        let Record::Header(header) = frame.record else {
            return Err(Error::invalid("WAL first record is not header"));
        };
        if header.segment != number {
            return Err(Error::invalid("WAL filename segment"));
        }
        let file_digest = io::hash_prefix(&mut file, metadata.length)?.digest();
        if writer.metadata(&name).map_err(io::namespace)? != Some(metadata) {
            return Err(Error::invalid("WAL identity changed during scan"));
        }
        plan.remember(RequiredFile {
            name,
            metadata,
            digest: file_digest,
        })?;
        if header.origin == Origin::Resume {
            // An anchor authenticates its own header. Its resume metadata remains
            // required, but the metadata's older WAL need not remain retained.
            let (resume, required) = required_record(writer, io::resume_name(epoch)?)?;
            if resume.identity != frame.identity || required.digest != header.resume_digest {
                return Err(Error::invalid("epoch resume identity/digest"));
            }
            let Record::Resume { segment, previous } = resume.record else {
                return Err(Error::invalid("epoch resume record kind"));
            };
            if segment != number || header.previous.as_ref() != Some(&previous) {
                return Err(Error::invalid("epoch resume predecessor"));
            }
            plan.remember(required)?;
        }
        let anchor =
            epoch == plan.checkpoint.anchor.epoch && number == plan.checkpoint.anchor.segment;
        let previous = header.previous.clone();
        plan.segments.push(Segment {
            identity: frame.identity,
            header,
            header_digest,
            header_end: frame.next,
            name,
            metadata,
            complete_end: frame.next,
            end_q: 0,
            prefix_digest: [0; 32],
            sealed: false,
        });
        if anchor {
            break;
        }
        let previous =
            previous.ok_or(Error::invalid("WAL chain does not reach checkpoint anchor"))?;
        epoch = previous.epoch;
        number = previous.segment;
    }
    plan.segments.reverse();
    for index in 0..plan.segments.len() {
        scan_segment(plan, writer, index)?;
    }
    let tip = plan
        .segments
        .last()
        .ok_or(Error::invalid("empty recovery chain"))?;
    plan.tip = Link {
        epoch: tip.identity.epoch,
        segment: tip.header.segment,
        end_offset: tip.complete_end,
        file_size: tip.metadata.length,
        q: tip.end_q,
        prefix_digest: tip.prefix_digest,
    };
    Ok(())
}

fn scan_segment<W: WriterNamespace>(
    plan: &mut RecoveryPlan,
    writer: &mut W,
    index: usize,
) -> Result<()> {
    let segment = &plan.segments[index];
    let successor = plan.segments.get(index + 1);
    let mut file = writer
        .open_existing(&segment.name, OpenMode::ReadOnly)
        .map_err(io::namespace)?;
    let mut offset = segment.header_end;
    let mut q = segment.header.start_q;
    let mut hasher = io::hash_prefix(&mut file, offset)?;
    let mut sealed = false;
    let mut anchor_seen = index != 0;
    let (mut transactions, _transactions_charge) =
        vector::<u64>(&plan.scratch, MAX_SEGMENT_TRANSACTIONS)?;
    if index > 0 {
        let prior = &plan.segments[index - 1];
        let expected = Link {
            epoch: prior.identity.epoch,
            segment: prior.header.segment,
            end_offset: prior.complete_end,
            file_size: prior.metadata.length,
            q: prior.end_q,
            prefix_digest: prior.prefix_digest,
        };
        if segment.header.previous.as_ref() != Some(&expected) || q != prior.end_q {
            return Err(Error::invalid("WAL predecessor contents or sequence"));
        }
        if segment.header.origin == Origin::Rotation && !prior.sealed {
            return Err(Error::invalid("rotation predecessor lacks seal"));
        }
    } else if segment.header_digest != plan.checkpoint.anchor.header_digest {
        return Err(Error::invalid("checkpoint anchor header digest"));
    }
    loop {
        if index == 0 && offset == plan.checkpoint.anchor.offset {
            if q != plan.checkpoint.cut || hasher.digest() != plan.checkpoint.anchor.prefix_digest {
                return Err(Error::invalid("checkpoint anchor prefix or sequence"));
            }
            anchor_seen = true;
        }
        if sealed && offset != segment.metadata.length {
            return Err(Error::invalid("bytes after WAL seal"));
        }
        let frame = match io::read_frame(&mut file, offset, segment.metadata.length)? {
            FrameRead::End => break,
            FrameRead::Incomplete => {
                let authorized = successor.is_none()
                    || successor.is_some_and(|next| {
                        next.header.origin == Origin::Resume
                            && next.header.previous.as_ref().is_some_and(|link| {
                                link.end_offset == offset
                                    && link.file_size == segment.metadata.length
                            })
                    });
                if sealed || !authorized {
                    return Err(Error::invalid("incomplete interior WAL frame"));
                }
                break;
            }
            FrameRead::Complete(frame) => frame,
        };
        if frame.identity != segment.identity {
            return Err(Error::invalid("WAL frame namespace"));
        }
        match frame.record {
            Record::Batch(batch) => {
                if batch.previous_q != q || q.checked_add(1) != Some(batch.q) {
                    return Err(Error::invalid("WAL commit sequence discontinuity"));
                }
                if transactions.len() == MAX_SEGMENT_TRANSACTIONS {
                    return Err(Error::limit("128k transaction identities per segment"));
                }
                transactions.push(batch.txn_local);
                q = batch.q;
            }
            Record::Seal {
                end_offset,
                q: seal_q,
                prefix_digest,
            } => {
                if end_offset != offset || seal_q != q || prefix_digest != hasher.digest() {
                    return Err(Error::invalid("WAL seal prefix or sequence"));
                }
                sealed = true;
            }
            _ => return Err(Error::invalid("unexpected record inside WAL")),
        }
        hasher.update(&frame.bytes);
        offset = frame.next;
    }
    if !anchor_seen {
        return Err(Error::invalid(
            "checkpoint anchor is not a complete frame boundary",
        ));
    }
    if successor.is_some_and(|next| next.header.origin == Origin::Rotation) && !sealed {
        return Err(Error::invalid("rotation missing final seal"));
    }
    transactions.sort_unstable();
    if transactions.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::invalid("duplicate transaction identity"));
    }
    // Begin/commit order may be inverted. Compare sets, never numeric monotonicity.
    for prior in &plan.segments[..index] {
        if prior.identity.epoch != segment.identity.epoch {
            continue;
        }
        let (mut earlier, _earlier_charge) =
            vector::<u64>(&plan.scratch, MAX_SEGMENT_TRANSACTIONS)?;
        collect_transactions(writer, prior, &mut earlier)?;
        earlier.sort_unstable();
        let (mut left, mut right) = (0, 0);
        while left < transactions.len() && right < earlier.len() {
            match transactions[left].cmp(&earlier[right]) {
                std::cmp::Ordering::Less => left += 1,
                std::cmp::Ordering::Greater => right += 1,
                std::cmp::Ordering::Equal => {
                    return Err(Error::invalid(
                        "transaction identity repeated across segments",
                    ));
                }
            }
        }
    }
    let segment = &mut plan.segments[index];
    segment.complete_end = offset;
    segment.end_q = q;
    segment.prefix_digest = hasher.digest();
    segment.sealed = sealed;
    Ok(())
}

fn collect_transactions<W: WriterNamespace>(
    writer: &mut W,
    segment: &Segment,
    result: &mut Vec<u64>,
) -> Result<()> {
    let mut file = writer
        .open_existing(&segment.name, OpenMode::ReadOnly)
        .map_err(io::namespace)?;
    let mut offset = segment.header_end;
    while offset < segment.complete_end {
        let FrameRead::Complete(frame) = io::read_frame(&mut file, offset, segment.complete_end)?
        else {
            return Err(Error::invalid("planned transaction range changed"));
        };
        if frame.identity != segment.identity {
            return Err(Error::invalid("transaction set namespace"));
        }
        if let Record::Batch(batch) = frame.record {
            if result.len() == MAX_SEGMENT_TRANSACTIONS {
                return Err(Error::limit("transaction set bound"));
            }
            result.push(batch.txn_local);
        }
        offset = frame.next;
    }
    Ok(())
}

/// Read only the planned replay range; the caller owns logical application.
pub(super) fn visit_batches<W: WriterNamespace>(
    plan: &RecoveryPlan,
    writer: &mut W,
    mut visit: impl FnMut(Identity, super::super::codec::Batch) -> Result<()>,
) -> Result<()> {
    let mut q = plan.checkpoint.cut;
    for (index, segment) in plan.segments.iter().enumerate() {
        let mut file = writer
            .open_existing(&segment.name, OpenMode::ReadOnly)
            .map_err(io::namespace)?;
        let mut offset = if index == 0 {
            plan.checkpoint.anchor.offset
        } else {
            segment.header_end
        };
        while offset < segment.complete_end {
            let FrameRead::Complete(frame) =
                io::read_frame(&mut file, offset, segment.complete_end)?
            else {
                return Err(Error::invalid("planned replay boundary changed"));
            };
            if frame.identity != segment.identity {
                return Err(Error::invalid("replay namespace changed"));
            }
            match frame.record {
                Record::Batch(batch) => {
                    if batch.previous_q != q || q.checked_add(1) != Some(batch.q) {
                        return Err(Error::invalid("replay sequence changed"));
                    }
                    q = batch.q;
                    visit(frame.identity, batch)?;
                }
                Record::Seal { .. } => {}
                _ => return Err(Error::invalid("replay record kind changed")),
            }
            offset = frame.next;
        }
    }
    if q != plan.tip.q {
        return Err(Error::invalid("replay did not reach planned tip"));
    }
    Ok(())
}
