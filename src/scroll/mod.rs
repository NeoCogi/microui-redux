//! Retained scroll viewport state and public scroll-area handles.

mod handle;
pub use handle::{ScrollAreaHandle, ScrollAreaView, ScrollAreaViewMut};

mod state;
pub(crate) use state::ScrollAreaState;
