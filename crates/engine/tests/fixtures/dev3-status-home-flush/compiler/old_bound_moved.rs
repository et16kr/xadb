#![allow(dead_code, unused_imports)]
use crate::storage::wal_io::BoundStatusBootstrap;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d,'o,'m,'w,W:WriterNamespace>(mut old: BoundStatusBootstrap<'s,'c,'p,'d,'o,'m,'w,W>) {
    let result = old.into_with_flush();
    // The consuming adapter owns the former bound.
    // Healthy Before returns it only through the result.
    // This diagnostic is about the newly consuming transition.
    // The result remains live below.
    let views = old.split(); // expected diagnostic
    drop(views); drop(result);
}
