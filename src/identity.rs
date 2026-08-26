//
// Copyright 2023-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//

//! Non-reused process-local identity for application-facing retained objects.
//!
//! Event ports describe observable behavior and lifetime; they are deliberately not object keys.
//! This module supplies the orthogonal identity primitive wrapped by concrete window, popup, and
//! menu-item identifier types. The raw value never crosses the public API boundary.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide source shared by every application-facing retained object kind.
///
/// Relaxed ordering is sufficient because allocation establishes uniqueness only. It neither
/// publishes retained object memory nor synchronizes manager operations, which remain confined to
/// their owning Context thread.
static NEXT_RETAINED_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

/// Returns the following representable identity without wrapping to a previously issued value.
const fn advance_retained_object_id(current: u64) -> Option<u64> {
    // Checked addition makes exhaustion terminal instead of turning the process-wide source back
    // into zero or another value that could alias a destroyed object.
    current.checked_add(1)
}

/// Opaque process-local identity assigned once to one retained application-facing object.
///
/// Concrete wrappers preserve kind safety, while the shared namespace prevents equal identities
/// from arising in separate Contexts. This value is intentionally not a persistent identifier:
/// uniqueness lasts for the current process, matching the lifetime of every retained handle.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub(crate) struct RetainedObjectId(
    /// Non-zero raw value kept private so application code cannot forge retained capabilities.
    NonZeroU64,
);

impl RetainedObjectId {
    /// Allocates one process-unique identity or panics after exhausting the usable `u64` space.
    pub(crate) fn allocate() -> Self {
        // `fetch_update` modifies the global source only when checked advancement succeeds. Once
        // exhausted, every later attempt therefore fails consistently rather than reusing an ID.
        let raw = NEXT_RETAINED_OBJECT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, advance_retained_object_id)
            .expect("retained object identity space exhausted");
        Self(NonZeroU64::new(raw).expect("retained object identity allocator returned zero"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that allocator advancement cannot cross the wraparound boundary.
    #[test]
    fn retained_object_identity_advancement_stops_before_reuse() {
        // Ordinary values advance exactly once, while the terminal value remains unmodified by
        // `fetch_update` because this helper rejects its transition.
        assert_eq!(advance_retained_object_id(1), Some(2));
        assert_eq!(advance_retained_object_id(u64::MAX), None);
    }

    /// Verifies process-wide allocation produces distinct non-recycled values.
    #[test]
    fn retained_object_identity_allocations_are_unique() {
        // Equality is the only operation concrete object identifiers require; their numeric
        // representation intentionally remains inaccessible even to this behavioral assertion.
        let first = RetainedObjectId::allocate();
        let second = RetainedObjectId::allocate();
        assert_ne!(first, second);
    }
}
