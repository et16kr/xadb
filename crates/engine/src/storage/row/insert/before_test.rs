//! Call-scoped fault observations for A's actual Forward producer (MCA05-V29).
//! C still supplies its genuine lane and authorities. Only A's local before
//! bytes are substituted; no reservation or loan is changed.

use crate::storage::page::PAGE_BYTES;

type BeforeEdit = Box<dyn FnOnce(&mut [[u8; PAGE_BYTES]; 9])>;

thread_local! {
    static EDIT: std::cell::RefCell<Option<BeforeEdit>> = const { std::cell::RefCell::new(None) };
    static HITS: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Arms the next A Forward producer call on this thread. Lane issuance and
/// CLR do not consume the edit. Dropping the guard disarms an unused edit.
pub(in crate::storage) struct TestProducerBefore(std::marker::PhantomData<std::rc::Rc<()>>);

impl TestProducerBefore {
    pub(in crate::storage) fn arm(edit: impl FnOnce(&mut [[u8; PAGE_BYTES]; 9]) + 'static) -> Self {
        EDIT.with(|hook| {
            assert!(hook.replace(Some(Box::new(edit))).is_none());
        });
        HITS.with(|hits| hits.set(0));
        Self(std::marker::PhantomData)
    }

    pub(in crate::storage) fn hits(&self) -> u8 {
        HITS.with(std::cell::Cell::get)
    }
}

impl Drop for TestProducerBefore {
    fn drop(&mut self) {
        EDIT.with(|hook| hook.replace(None));
    }
}

/// A owns this heap copy until its producer returns (including error/unwind).
/// The allocation and its local owner are test instrumentation; production
/// resource censuses do not measure them.
#[inline(never)]
pub(super) fn edited_copy(before: &[[u8; PAGE_BYTES]; 9]) -> Option<Box<[[u8; PAGE_BYTES]; 9]>> {
    let edit = EDIT.with(|hook| hook.borrow_mut().take())?;
    let mut copy: Box<[[u8; PAGE_BYTES]; 9]> = vec![[0; PAGE_BYTES]; 9]
        .into_boxed_slice()
        .try_into()
        .expect("nine pages");
    for (to, from) in copy.iter_mut().zip(before) {
        to.copy_from_slice(from);
    }
    edit(&mut copy);
    HITS.with(|hits| hits.set(hits.get() + 1));
    Some(copy)
}
