//! U2-C4 §3 projected source census: the current Profile2/base3 recovery
//! census with the post-switch terms substituted. A projection over source
//! carrier bounds, not the switched source and not compiler stack evidence.
//! Growth is charged conservatively; a smaller replacement is never credited.
//!
//! C4c: this test pins the base3 codec (`assert_eq!(codec,
//! base::catalog::source_bytes())`) and must be removed or converted to the
//! switched source census there. The real census then takes
//! `B4 = base::row::source_bytes()` and the retained delta
//! `size_of::<RowBaseView>() - size_of::<CatalogBaseView>()` from the types,
//! never from the literals below.
use super::*;
use crate::storage::{control, retention_genesis, retention_obligation, retention_page};

/// Conservative ceiling for the base4 `RowCensus` retained in
/// `VerifiedPlan4Base` through `RowBaseView`; U2-C2 is absent on this branch,
/// so the size is an explicit input. U2-C2 r2 measures `RowCensus` 72 B and
/// `RowBaseView` 248 B against `CatalogBaseView` 256 B (LayoutView 176 +
/// CatalogCensus 80): the real retained delta is -8 B, which is not credited.
const ROW_CENSUS_BYTES: usize = 96;
/// U2-C2's base4 codec source bound (r2 peak 59699 B; may still move with
/// its pending Row Reserved fix). Unfilled until that closure lands here;
/// the decode phases are then projected and asserted, otherwise only printed.
const B4: Option<usize> = None;
/// Semantic Catalog Profile3 census minus Profile2. U2-C3 asserts it is not
/// larger (catalog3_tests) and its reviewer measured >= 1405 B slack.
const DELTA_C3: usize = 0;
const CAP: usize = 65_536;

fn project<W: WriterNamespace>(label: &str) -> usize {
    let c = source_census::<W>();
    let live = super::super::super::plan4_pages::retention_recovery_source_census::<W>();
    let codec = c.codec_deep;
    assert_eq!(codec, base::catalog::source_bytes());
    // (term, current Profile2/base3 value, post-switch value)
    let terms = [
        // The Catalog queue view is replaced by its Profile3 view, which
        // wraps the Row codec.
        (
            "retention_page_codec",
            retention_page::catalog::source_bytes(),
            retention_page::catalog3::source_bytes(),
        ),
        (
            "retention_genesis",
            retention_genesis::current_source_bytes(),
            retention_genesis::row_current_source_bytes(),
        ),
        (
            "aging_obligation",
            retention_obligation::catalog::source_bytes(),
            retention_obligation::row::source_bytes(),
        ),
        (
            "job_view_x2",
            2 * size_of::<retention_page::catalog::JobView<'_>>(),
            2 * size_of::<retention_page::catalog3::JobView<'_>>(),
        ),
        (
            "root_view_x2",
            2 * size_of::<retention_page::catalog::RootView<'_>>(),
            2 * size_of::<retention_page::row::RootView<'_>>(),
        ),
        (
            "base_census",
            size_of::<base::catalog::CatalogCensus>(),
            ROW_CENSUS_BYTES,
        ),
    ];
    let mut growth = DELTA_C3;
    for (name, old, new) in terms {
        println!("U2C4_PROJECTED_TERM\t{label}\t{name}\told={old}\tnew={new}");
        growth += new.saturating_sub(old);
    }
    let census_growth = ROW_CENSUS_BYTES.saturating_sub(size_of::<base::catalog::CatalogCensus>());
    let profile3 = control::row_profile::source_bytes();
    let installed = control::installed_profile_source_bytes();
    let wrapper = installed - profile3;
    println!("U2C4_PROJECTED_PROFILE\t{label}\tprofile3={profile3}\tinstalled={installed}");

    // Recovered live: retained growth everywhere; the shared readers (page
    // load, flush readback, replay) also gain the installed-profile wrapper.
    let active = live.peak - live.retained - live.split_ports;
    let readers = live.load.max(live.flush).max(live.replay) + wrapper;
    let live_projected = c.live + growth + readers.saturating_sub(active);
    // Every other phase may hold the retained base and one wrapper frame.
    let other = [
        c.acquire,
        c.selection,
        c.analysis,
        c.base_capture,
        c.physical,
        c.epoch,
        c.handoff,
        c.bind,
    ]
    .into_iter()
    .max()
    .unwrap()
        + growth
        + wrapper;
    // Decode phases: base4 codec in place of base3; besides the retained
    // VerifiedPlan4Base (in `growth`) two Result<RowBaseView> carriers grow.
    let decode_outer = [
        c.selection_decode,
        c.analysis_decode,
        c.capture_encode,
        c.handoff_decode,
        c.epoch,
    ]
    .into_iter()
    .max()
    .unwrap()
        - codec
        + 2 * census_growth
        + growth
        + wrapper;
    let b4_ceiling = CAP.saturating_sub(decode_outer);
    let decode = B4.map(|b4| decode_outer + b4);
    let peak = live_projected.max(other).max(decode.unwrap_or(0));
    println!(
        "U2C4_PROJECTED_CENSUS\t{label}\tcurrent_peak={}\tcurrent_live={}\tactive={active}\treaders={readers}\tlive={live_projected}\tother={other}\tdecode_outer={decode_outer}\tbase3_codec={codec}\tb4_ceiling={b4_ceiling}\tb4={}\tprojected_peak={peak}",
        c.peak,
        c.live,
        B4.map_or("UNFILLED".to_owned(), |b4| b4.to_string())
    );
    assert!(
        live_projected <= CAP,
        "projected recovered live {live_projected}"
    );
    assert!(other <= CAP, "projected other phases {other}");
    if let Some(decode) = decode {
        assert!(decode <= CAP, "projected decode {decode}");
    }
    peak
}

#[test]
fn projected_switch_census_memory() {
    let peak = project::<port::Writer>("memory");
    assert!(peak <= CAP);
}

#[cfg(target_os = "linux")]
#[test]
fn projected_switch_census_native() {
    let peak = project::<super::super::super::plan4_pages::native::Writer>("native");
    assert!(peak <= CAP);
}
