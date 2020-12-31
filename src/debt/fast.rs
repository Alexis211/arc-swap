//! The fast slots for the primary strategy.
//!
//! They are faster, but fallible (in case the slots run out or if there's a collision with a
//! writer thread, this gives up and falls back to secondary strategy).

use std::cell::Cell;
use std::slice::Iter;
use std::sync::atomic::Ordering::*;

use super::Debt;

const DEBT_SLOT_CNT: usize = 8;

/// Thread-local information for the [`Slots`]
#[derive(Default)]
pub(super) struct Local {
    // The next slot in round-robin rotation. Heuristically tries to balance the load across them
    // instead of having all of them stuffed towards the start of the array which gets
    // unsuccessfully iterated through every time.
    offset: Cell<usize>,
}

/// Bunch of fast debt slots.
#[derive(Default)]
pub(super) struct Slots([Debt; DEBT_SLOT_CNT]);

impl Slots {
    /// Try to allocate one slot and get the pointer in it.
    ///
    /// Fails if there are no free slots.
    pub(super) fn get_debt(&self, ptr: usize, local: &Local) -> Option<&Debt> {
        // Trick with offsets: we rotate through the slots (save the value from last time)
        // so successive leases are likely to succeed on the first attempt (or soon after)
        // instead of going through the list of already held ones.
        let offset = local.offset.get();
        let len = self.0.len();
        for i in 0..len {
            let i = (i + offset) % len;
            // Note: the indexing check is almost certainly optimised out because the len
            // is used above. And using .get_unchecked was actually *slower*.
            let got_it = self.0[i]
                .0
                // Try to acquire the slot. Relaxed if it doesn't work is fine, as we don't
                // synchronize by it.
                .compare_exchange(Debt::NONE, ptr, SeqCst, Relaxed)
                .is_ok();
            if got_it {
                local.offset.set(i + 1);
                return Some(&self.0[i]);
            }
        }
        None
    }
}

impl<'a> IntoIterator for &'a Slots {
    type Item = &'a Debt;

    type IntoIter = Iter<'a, Debt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
