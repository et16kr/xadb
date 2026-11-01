//! MCB01-N01 native worker (consumers §9.2, boundaries `#row-mi03-s10`):
//! the committed six-row corpus over one native namespace in two processes,
//! with no A. `produce` (P1) is C's actual install, CREATE of T, the corpus
//! statement through C's checked ingress (`corpus!`, every frame and after
//! image the author's) and C's Commit (q 2), stopped there without shutdown.
//! `reenter` (P2) is C's recovery (RejectInitialLosers) of those files to
//! READY and B's point read of every RowId 1..7 and its cursor over T, under
//! a read-only snapshot. The expected values are the author's rows and gap
//! (`ROWS`, `GAP`); the controller compares the receipt with expected.json.
use super::super::super::super::super::plan4_pages::native;
use super::super::row_reopen::{AtReady, ready_row_sources, reopen, wal_witness};
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

/// The native namespace's files, read from its directory.
struct NativeFiles<'a>(&'a Path);

impl CorpusFiles for NativeFiles<'_> {
    fn wal_bytes(&self) -> Vec<u8> {
        let mut names: Vec<_> = fs::read_dir(self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .filter(|name| name.starts_with("d3-wal-"))
            .collect();
        names.sort();
        assert_eq!(names, ["d3-wal-00000001"], "one WAL file");
        fs::read(self.0.join(&names[0])).unwrap()
    }
    fn profile(&self) -> Vec<u8> {
        fs::read(self.0.join("d3-profile")).unwrap()
    }
}

/// What P2 read: READY Q, the snapshot's Q, the point of each RowId 1..7
/// and the cursor's rows.
#[derive(Debug, PartialEq)]
struct Reread {
    ready_q: u64,
    q: u64,
    points: Vec<Option<i64>>,
    rows: Vec<(u64, i64)>,
}

/// The author's answer: every corpus row by point, the gap Missing, and the
/// six rows in RowId order.
fn expected_reread() -> Reread {
    Reread {
        ready_q: Q,
        q: Q,
        points: (1..=7)
            .map(|row| ROWS.iter().find(|(_, r, _)| *r == row).map(|(_, _, v)| *v))
            .collect(),
        rows: expected(None, None),
    }
}

struct ReadAll;

impl AtReady for ReadAll {
    type Out = Reread;
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        _: &mut [u8; super::super::super::super::CATALOG_PRIVATE_BYTES],
    ) -> Reread {
        let ready_q = live.catalog_test_q();
        let _ = ready_row_sources(live, owner, flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let reader_private = &mut *reader_private;
        let (mut writer, reader, mut flush) = live.split();
        let b = read_only!(writer, owner, IsolationLevel::Serializable, reader_private);
        let view = writer.begin_statement(owner, &b).unwrap();
        let q = view.q().unwrap();
        let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
        let points: Vec<Option<i64>> = (1..=7)
            .map(|row| {
                point_loading!(
                    &reader,
                    owner,
                    reader_private,
                    &view,
                    &table,
                    row,
                    flush,
                    flusher
                )
                .unwrap()
                .0
            })
            .collect();
        let rows = scan!(reader, owner, reader_private, &view, &table, flush, flusher);
        assert_eq!(points[usize::try_from(GAP).unwrap() - 1], None, "the gap");
        assert!(writer.finish_statement(owner, view).is_ok());
        assert!(writer.finish_read_only(owner, b).is_ok());
        owner.check_idle().unwrap();
        Reread {
            ready_q,
            q,
            points,
            rows,
        }
    }
}

#[test]
#[ignore = "bounded S10 native controller supplies isolated paths and identity"]
fn s10_corpus_native_worker() {
    let env = |name| std::env::var(format!("XADB_S10_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    assert_eq!(id, "MCB01-N01");
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let head = format!(
        "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\n",
        std::process::id()
    );
    if phase == "produce" {
        let mut summary = String::new();
        case_within(&format!("{id}-produce"), Duration::from_secs(180), || {
            let corpus = Corpus::load();
            let files = NativeFiles(&db);
            with_directory!(
                &directory,
                frames = 28,
                |live, owner, flusher, status, undo, private, reader_private, _core, fence| {
                    let state = &files;
                    corpus!(corpus, live, owner, flusher, status, undo, private,
                        reader_private, state => writer, reader, flush, tx, view, table,
                        issued, targets, spare);
                    let _ = (&issued, &targets, &spare, &reader);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let q = match writer.commit_row(owner, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(_) => panic!("C's Commit of the corpus refused"),
                    };
                    assert_eq!(q, Q);
                    owner.check_idle().unwrap();
                    assert!(!fence.is_fenced());
                    let cut = wal_witness(&db);
                    summary = format!(
                        "result\tcommitted\nq\t{q}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                        cut.0, cut.1
                    );
                    let _ = (flush, flusher);
                }
            );
        });
        fs::write(output.join("receipt.tsv"), format!("{head}{summary}")).unwrap();
        println!("S10_NATIVE {id} produce {summary:?}");
        // The cut: no flush or shutdown write after C's Commit of the corpus.
        std::process::exit(0);
    }
    assert_eq!(phase, "reenter");
    let mut summary = String::new();
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        let reread = reopen(&directory, &mut ReadAll);
        println!("S10_NATIVE {id} reenter {reread:?}");
        assert_eq!(reread, expected_reread());
        let points = reread
            .points
            .iter()
            .map(|point| point.map_or("null".to_string(), |value| value.to_string()))
            .collect::<Vec<_>>()
            .join(",");
        let rows = reread
            .rows
            .iter()
            .map(|(row, value)| format!("{row}:{value}"))
            .collect::<Vec<_>>()
            .join(",");
        summary = format!(
            "result\tready\nq_ready\t{}\nq\t{}\npoints\t{points}\nrows\t{rows}\n",
            reread.ready_q, reread.q
        );
    });
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "{head}handles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n{summary}",
            receipt.handles, receipt.peak, receipt.fd_peak, receipt.mutations, receipt.log_bytes
        ),
    )
    .unwrap();
}
