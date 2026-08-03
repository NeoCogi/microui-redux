/// Records one pending semantic event without wrapping at the counter boundary.
pub(crate) fn record_pending_event(pending: &mut u32) {
    *pending = pending.saturating_add(1);
}

/// Consumes exactly one pending semantic event.
pub(crate) fn take_pending_event(pending: &mut u32) -> bool {
    if *pending == 0 {
        return false;
    }
    *pending -= 1;
    true
}
