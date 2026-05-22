//! Layout scope frames and flow snapshots.

use crate::{rect, vec2, Recti, Vec2i};

use super::flow::{FlowState, FlowTemplate, ScopeState};

#[derive(Clone)]
pub(super) struct LayoutFrame {
    // Coordinates/cursors for one nested layout scope.
    pub(super) scope: ScopeState,
    // Placement logic used for this scope.
    pub(super) flow: FlowState,
}

impl LayoutFrame {
    /// Creates a new scope frame with scroll folded into local coordinate origin.
    pub(super) fn new(body: Recti, scroll: Vec2i) -> Self {
        Self {
            scope: ScopeState {
                // Scope body is shifted by scroll so local coordinates remain stable while content moves.
                body: rect(body.x - scroll.x, body.y - scroll.y, body.width, body.height),
                cursor: vec2(0, 0),
                max: None,
                next_row: 0,
                indent: 0,
            },
            flow: FlowState::default(),
        }
    }
}

pub(crate) struct FlowSnapshot {
    // Captures active flow configuration for scoped overrides.
    flow: FlowTemplate,
}

impl FlowSnapshot {
    /// Captures the flow template from a live layout frame.
    pub(super) fn from_layout(layout: &LayoutFrame) -> Self {
        Self { flow: layout.flow.as_template() }
    }

    /// Applies the captured flow template to a live layout frame.
    pub(super) fn apply(self, layout: &mut LayoutFrame) {
        layout.flow.apply_template(self.flow);
    }
}
