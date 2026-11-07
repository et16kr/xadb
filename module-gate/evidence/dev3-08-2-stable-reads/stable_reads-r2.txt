//! 08-2: keep the same fixed-q cursor/locator across actual system SMO.
use super::*;
use crate::storage::row::read::driver::live::{open_locator_live, read_locator_live};

macro_rules! next_row {
    ($cursor:expr, $reader:ident, $owner:expr, $private:expr, $output:expr) => {{
        next_live(
            $cursor,
            ReadCall {
                port: &$reader,
                owner: $owner,
                private: $private,
                residency: &mut Plan4ResidencyCursor::new(),
            },
            $output,
            &Cancellation::default(),
        )
    }};
}
macro_rules! locate {
    ($locator:expr, $reader:ident, $owner:expr, $private:expr, $key:expr, $output:expr) => {{
        read_locator_live(
            $locator,
            ReadCall {
                port: &$reader,
                owner: $owner,
                private: $private,
                residency: &mut Plan4ResidencyCursor::new(),
            },
            RowId::new($key).unwrap(),
            $output,
            &Cancellation::default(),
        )
    }};
}

#[test]
fn stable_reads_cursor_and_locator_cross_live_root_split() {
    case_within("STABLE-SPLIT", std::time::Duration::from_secs(900), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 48,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &spec());
                retire_create!(live, owner, private);
                for _ in 0..7 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let keys = fill!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    bigs(0..7)
                );
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let read = read_only!(writer, b, IsolationLevel::ReadCommitted, reader_private);
                let view = writer.begin_statement(b, &read).unwrap();
                let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut locator = open_locator_live(&view, &table).unwrap();
                let mut output = [0; 4096];
                let first = next_row!(&mut cursor, reader, b, reader_private, &mut output).unwrap();
                assert!(
                    matches!(first, ReadStep::Ready(CursorStep::Row(ref r)) if r.version().row.get() == keys[0])
                );
                for _ in 0..2 {
                    let located = locate!(
                        &mut locator,
                        reader,
                        b,
                        reader_private,
                        keys[6],
                        &mut output
                    )
                    .unwrap();
                    assert!(
                        matches!(located, ReadStep::Ready(PointRead::Found(ref r)) if tag(&output, r.len()) == tagged(6, BIG))
                    );
                }
                assert!(next_row!(&mut cursor, reader, b, reader_private, &mut []).is_err());
                assert!(matches!(
                    writer.split_row_live(owner, &table, None, 2048, private, &mut flush),
                    Ok(RowSplitStep::Split)
                ));
                assert_eq!(writer.catalog_test_with_pool(leaves).len(), 2);
                // Row 7 moved to another page and slot; the cached location
                // cannot authorize either stale bytes or an absent result.
                for clear in [false, true] {
                    if clear {
                        locator.clear();
                    }
                    let located = locate!(
                        &mut locator,
                        reader,
                        b,
                        reader_private,
                        keys[6],
                        &mut output
                    )
                    .unwrap();
                    assert!(
                        matches!(located, ReadStep::Ready(PointRead::Found(ref r)) if tag(&output, r.len()) == tagged(6, BIG))
                    );
                }
                for (at, key) in keys.iter().enumerate().skip(1) {
                    let row =
                        next_row!(&mut cursor, reader, b, reader_private, &mut output).unwrap();
                    assert!(
                        matches!(row, ReadStep::Ready(CursorStep::Row(ref r)) if r.version().row.get() == *key && tag(&output, r.len()) == tagged(at, BIG))
                    );
                }
                assert!(matches!(
                    next_row!(&mut cursor, reader, b, reader_private, &mut output),
                    Ok(ReadStep::Ready(CursorStep::End))
                ));
                locator.close().unwrap();
                cursor.close().unwrap();
                assert!(writer.finish_statement(b, view).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
            }
        );
    });
}
