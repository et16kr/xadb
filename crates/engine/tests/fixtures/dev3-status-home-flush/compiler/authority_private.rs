#![allow(dead_code, unused_imports)]
use super::StatusFlushAuthority;
fn probe(authority: StatusFlushAuthority<'_,'_>) {
    // This sibling has type visibility, not private field access.
    // A real authority cannot make its constructor public.
    // FRU does not require naming hidden field types.
    // No caller identity is used to fabricate a grant.
    // All private-field primary diagnostics must stay here.
    // The ordinary struct is non-Copy/non-Clone.
    let copied = StatusFlushAuthority { ..authority }; // expected diagnostic
    drop(copied);
}
