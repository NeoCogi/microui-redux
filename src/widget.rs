//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
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
// -----------------------------------------------------------------------------
// Ported to rust from https://github.com/rxi/microui/ and the original license
//
// Copyright (c) 2020 rxi
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE.
//
//! Widget runtime contracts and per-frame result tracking.

use std::cell::RefCell;
use std::cmp::max;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use bitflags::bitflags;
use rs_math3d::Dimensioni;

use crate::atlas::{AtlasHandle, EXPAND_DOWN_ICON};
use crate::window_manager::RootId;
use crate::id::Id;
use crate::input::ResourceState;
use crate::style::Style;
use crate::ui_node::UiInputEvent;
pub use crate::widget_ctx::{WidgetInputEvents, WidgetPaintCtx, WidgetUpdateCtx};

bitflags! {
    #[derive(Copy, Clone)]
    /// Widget-specific options that influence layout and interactivity.
    pub struct WidgetOption : u32 {
        /// Gives the widget a Style-owned outer border and inset content rectangle.
        const FRAME = 512;
        /// Keeps keyboard focus while the widget is held.
        const HOLD_FOCUS = 256;
        /// Consumes scroll input while the widget is hovered.
        const GRAB_SCROLL = 32;
        /// Disables interaction for the widget.
        const NO_INTERACT = 4;
        /// Aligns the widget to the right side of the cell.
        const ALIGN_RIGHT = 2;
        /// Centers the widget inside the cell.
        const ALIGN_CENTER = 1;
        /// No special options.
        const NONE = 0;
    }
}

/// High-level focus behavior requested by a widget.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FocusPolicy {
    /// Focus is only needed for the click interaction and clears when the button is released.
    Momentary,
    /// Focus remains after release until the widget explicitly clears it or another click moves it.
    HoldUntilBlur,
    /// Focus captures a pointer drag and clears when the drag button is released.
    DragCapture,
}

impl FocusPolicy {
    /// Derives a policy from widget options.
    pub fn from_widget_options(opt: WidgetOption) -> Self {
        if opt.intersects(WidgetOption::HOLD_FOCUS) {
            Self::HoldUntilBlur
        } else {
            Self::Momentary
        }
    }

    /// Returns whether focus should clear when the pointer button is released.
    pub(crate) fn releases_on_mouse_up(self) -> bool {
        matches!(self, Self::Momentary | Self::DragCapture)
    }
}

/// Marker trait for application-facing state retained by a widget runtime.
///
/// State contains values, events, and commands that remain meaningful after construction. It does
/// not implement widget measurement, update, or painting; those phases belong to [`Widget`].
pub trait WidgetState: 'static {}

impl WidgetState for () {}

/// Marker trait for one-shot widget construction input.
///
/// Parameters seed application state and configure the runtime. Values that applications must
/// mutate after construction belong in the state owned by the builder's associated runtime.
pub trait WidgetParameters: 'static {}

/// A cloneable, non-owning capability for checked access to concrete widget state.
///
/// The concrete [`WidgetStateOwner`] runtime is the persistent owner. Consequently, cloning this
/// handle never keeps removed state alive and does not require `T: Clone`.
pub struct WidgetStateHandle<T: WidgetState> {
    /// Weak access to the state allocation retained by the concrete runtime.
    cell: Weak<RefCell<T>>,
}

impl<T: WidgetState> Clone for WidgetStateHandle<T> {
    fn clone(&self) -> Self {
        Self { cell: self.cell.clone() }
    }
}

impl<T: WidgetState> WidgetStateHandle<T> {
    /// Creates a weak state capability for a concrete runtime's private state allocation.
    ///
    /// This borrows the strong owner only long enough to downgrade it and never exposes that owner
    /// through the resulting handle.
    pub fn new(owner: &Rc<RefCell<T>>) -> Self {
        Self { cell: Rc::downgrade(owner) }
    }

    /// Reports whether the state allocation still has a strong owner or active access upgrade.
    ///
    /// This check does not borrow the state contents.
    pub fn is_alive(&self) -> bool {
        self.cell.upgrade().is_some()
    }

    /// Runs `f` with checked shared access, or returns `None` when state is unavailable.
    ///
    /// Call [`is_alive`](Self::is_alive) after a failure only when the application needs to
    /// distinguish an expired owner from a temporary same-cell borrow conflict.
    pub fn try_read<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let owner = self.cell.upgrade()?;
        let state = owner.try_borrow().ok()?;
        Some(f(&state))
    }

    /// Runs `f` with checked exclusive access, or returns `None` when state is unavailable.
    ///
    /// The closure is not invoked on failure.
    pub fn try_update<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let owner = self.cell.upgrade()?;
        let mut state = owner.try_borrow_mut().ok()?;
        Some(f(&mut state))
    }

    /// Runs an exclusive update while preserving `input` if access fails before `f` starts.
    pub fn try_update_with<I, R>(&self, input: I, f: impl FnOnce(&mut T, I) -> R) -> Result<R, I> {
        let owner = match self.cell.upgrade() {
            Some(owner) => owner,
            None => return Err(input),
        };
        let mut state = match owner.try_borrow_mut() {
            Ok(state) => state,
            Err(_) => return Err(input),
        };
        Ok(f(&mut state, input))
    }
}

/// Runtime widget that privately owns one concrete application-state allocation.
///
/// The returned handle must refer to the same allocation used by the runtime's [`Widget`] phases.
/// It is a safe conformance requirement that each retained runtime be the unique persistent owner of
/// that allocation: implementations must not expose a strong owner, raw weak pointer, or cloning path
/// that lets the same state allocation back multiple retained nodes.
pub trait WidgetStateOwner: Widget + 'static {
    /// Concrete state owned by this runtime.
    type State: WidgetState;

    /// Returns a non-owning checked capability for the runtime's state allocation.
    fn state_handle(&self) -> WidgetStateHandle<Self::State>;
}

/// Associates one-shot construction parameters with one concrete state-owning widget runtime.
pub trait WidgetBuilder: Sized + 'static {
    /// One-shot input consumed during construction.
    type Parameters: WidgetParameters;
    /// Concrete runtime created by this builder.
    type W: WidgetStateOwner;

    /// Consumes Parameters and creates the concrete runtime with its private strong state owner.
    fn create_widget(parameters: Self::Parameters) -> Self::W;
}

/// Reads associated state during retained runtime dispatch or reports an invariant violation.
pub(crate) fn runtime_read_state<T: WidgetState, R>(state: &Rc<RefCell<T>>, phase: &'static str, f: impl FnOnce(&T) -> R) -> R {
    let state = state.try_borrow().unwrap_or_else(|_| {
        panic!(
            "retained widget state invariant violated during {phase}: associated state is already borrowed; application state-access closures must finish before rendering"
        )
    });
    f(&state)
}

/// Updates associated state during retained runtime dispatch or reports an invariant violation.
pub(crate) fn runtime_update_state<T: WidgetState, R>(state: &Rc<RefCell<T>>, phase: &'static str, f: impl FnOnce(&mut T) -> R) -> R {
    let mut state = state.try_borrow_mut().unwrap_or_else(|_| {
        panic!(
            "retained widget state invariant violated during {phase}: associated state is already borrowed; application state-access closures must finish before rendering"
        )
    });
    f(&mut state)
}

/// Common retained runtime phase contract implemented by concrete widgets.
///
/// Widgets participate in three retained execution phases:
/// 1. `measure`, which reports intrinsic size for the current frame's layout pass.
/// 2. `update`, which samples interaction, mutates widget-local state, and produces the current
///    frame result.
/// 3. `paint`, which records paint commands for the updated widget state.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the intrinsic widget size for the current frame's layout pass.
    ///
    /// `avail` reports the current container body size visible to the widget.
    /// Values less than or equal to zero are treated as "use layout defaults" for that axis.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Updates retained widget state for the current frame and returns its interaction result.
    ///
    /// Pointer positions in `input` are relative to the widget's derived content rectangle, using
    /// the same origin as [`WidgetUpdateCtx::local_rect`]. Outer frame pixels remain part of the
    /// runtime hit target, so a pointer event on the border may lie just outside the local content
    /// bounds. The update context intentionally cannot record drawing commands.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Vec<UiInputEvent>) -> ResourceState;
    /// Records paint commands for the current frame through paint-only capabilities.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>);
    /// Returns the effective widget options used by generic dispatch.
    ///
    /// Widgets can override this to apply dynamic option adjustments.
    fn effective_widget_opt(&self) -> WidgetOption {
        *self.widget_opt()
    }
    /// Returns the focus behavior used by generic dispatch.
    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::from_widget_options(self.effective_widget_opt())
    }
}

/// Retained interaction identity used by focus, hover, and frame results.
///
/// Normal retained traversal uses `Node` identities. `Root` is available for root-level results
/// and future framework controls that do not naturally belong to a plain widget node.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RetainedId {
    /// Stable root-window, dialog, or popup identity.
    Root(RootId),
    /// Stable retained-tree node identity.
    Node(Id),
    /// Stable retained-tree node identity scoped to the owning root or scroll area.
    ScopedNode {
        /// Stable owner/root/scroll-area scope.
        scope: Id,
        /// Stable node ID within that scope.
        node: Id,
    },
}

impl RetainedId {
    /// Creates a retained root interaction ID.
    pub const fn root(root_id: RootId) -> Self {
        Self::Root(root_id)
    }

    /// Creates a retained node interaction ID.
    pub const fn node(node_id: Id) -> Self {
        Self::Node(node_id)
    }

    /// Creates a scoped retained node interaction ID.
    ///
    /// Root containers use a scope derived from their `RootId`; retained scroll areas use their
    /// node ID as the child-container scope.
    pub const fn scoped_node(scope: Id, node_id: Id) -> Self {
        Self::ScopedNode { scope, node: node_id }
    }

    /// Creates a retained node ID scoped to a registered root.
    pub fn root_node(root_id: RootId, node_id: Id) -> Self {
        Self::scoped_node(Id::new(root_id.raw() as u64), node_id)
    }
}

/// Per-frame widget interaction results keyed by retained identity.
///
/// The storage is split into two generations:
/// - the committed result set published at the end of the previous frame,
/// - and the current in-progress result set being written by this frame.
#[derive(Default)]
pub(crate) struct FrameResults {
    /// Result generation published after the previous frame.
    committed: FrameResultStore,
    /// Result generation being written by the current frame.
    current: FrameResultStore,
    /// Duplicate-dispatch detector for the current frame.
    current_dispatch: FrameDispatchTracker,
}

#[derive(Default)]
/// Mutable storage for one frame-result generation.
struct FrameResultStore {
    /// Primary public result storage keyed by fully scoped retained identity.
    entries: HashMap<RetainedId, ResourceState>,
}

impl FrameResultStore {
    /// Clears retained results.
    fn clear(&mut self) {
        self.entries.clear();
    }

    /// Records the state produced by one retained widget dispatch.
    fn record_retained(&mut self, retained_id: RetainedId, state: ResourceState) {
        let prev_state = self.entries.insert(retained_id, state);
        debug_assert!(prev_state.is_none(), "retained result for {:?} was recorded more than once", retained_id);
    }

    /// Returns a read-only view over this generation.
    fn generation(&self) -> FrameResultGeneration<'_> {
        FrameResultGeneration::new(&self.entries)
    }
}

#[derive(Default)]
/// Detects duplicate retained-id dispatch within one frame.
struct FrameDispatchTracker {
    /// Dispatch site for each retained ID seen in the current frame.
    retained_sites: HashMap<RetainedId, String>,
}

impl FrameDispatchTracker {
    /// Clears all dispatch sites before a new frame.
    fn clear(&mut self) {
        self.retained_sites.clear();
    }

    /// Records one retained-id dispatch and panics on duplicate use.
    fn record_retained(&mut self, retained_id: RetainedId, dispatch_site: String) {
        if let Some(first_site) = self.retained_sites.get(&retained_id) {
            panic!(
                "duplicate retained dispatch detected for {:?}. first dispatch: {}. duplicate dispatch: {}.",
                retained_id, first_site, dispatch_site
            );
        }

        self.retained_sites.insert(retained_id, dispatch_site);
    }
}

/// Read-only view over one frame-result generation.
#[derive(Copy, Clone)]
pub struct FrameResultGeneration<'a> {
    /// Retained result map for this generation.
    entries: &'a HashMap<RetainedId, ResourceState>,
}

impl<'a> FrameResultGeneration<'a> {
    /// Creates a read-only view over a specific result generation.
    fn new(entries: &'a HashMap<RetainedId, ResourceState>) -> Self {
        Self { entries }
    }

    /// Returns the state for a retained interaction ID in this generation.
    pub fn state_of_retained(&self, retained_id: RetainedId) -> ResourceState {
        self.entries.get(&retained_id).copied().unwrap_or(ResourceState::NONE)
    }
}

impl FrameResults {
    /// Clears the in-progress frame results for a new frame.
    ///
    /// Previously committed results remain available through [`FrameResults::committed`].
    pub(crate) fn begin_frame(&mut self) {
        self.current.clear();
        self.current_dispatch.clear();
    }

    /// Publishes the current frame as the next committed result generation.
    pub(crate) fn finish_frame(&mut self) {
        std::mem::swap(&mut self.committed, &mut self.current);
        self.current.clear();
        self.current_dispatch.clear();
    }

    /// Records a result from a directly owned runtime.
    pub(crate) fn record_direct_with_context(&mut self, retained_id: RetainedId, state: ResourceState, dispatch_site: impl Into<String>) {
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    /// Records an internal retained node result without a legacy widget identity.
    #[cfg(test)]
    pub(crate) fn record_node_with_context(&mut self, retained_id: RetainedId, state: ResourceState, dispatch_site: impl Into<String>) {
        self.record_retained_id_with_context(retained_id, state, dispatch_site);
    }

    /// Records a retained id after duplicate-dispatch validation.
    fn record_retained_id_with_context(&mut self, retained_id: RetainedId, state: ResourceState, dispatch_site: impl Into<String>) {
        let dispatch_site = dispatch_site.into();
        self.current_dispatch.record_retained(retained_id, dispatch_site);
        self.current.record_retained(retained_id, state);
    }

    /// Returns the committed result generation published by the previous frame.
    pub(crate) fn committed(&self) -> FrameResultGeneration<'_> {
        self.committed.generation()
    }

    /// Returns the in-progress result generation for the current frame.
    #[cfg(test)]
    pub(crate) fn current(&self) -> FrameResultGeneration<'_> {
        self.current.generation()
    }
}

impl Widget for WidgetOption {
    fn widget_opt(&self) -> &WidgetOption {
        self
    }

    fn measure(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        // Internal placeholder widgets reserve enough room for text or an expand icon.
        let padding = style.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon_height = atlas.get_icon_size(EXPAND_DOWN_ICON).height;
        let content = max(font_height, icon_height);
        let height = (content + vertical_pad * 2).max(0);
        let width = (padding * 2 + content).max(0);
        Dimensioni::new(width, height)
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
        ResourceState::NONE
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

#[cfg(test)]
mod state_ownership_tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    struct TestParameters {
        value: usize,
    }

    impl WidgetParameters for TestParameters {}

    struct TestState {
        value: usize,
    }

    impl WidgetState for TestState {}

    struct NonCloneState;

    impl WidgetState for NonCloneState {}

    struct TestWidget {
        state: Rc<RefCell<TestState>>,
        opt: WidgetOption,
    }

    impl Widget for TestWidget {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(1, 1)
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
            runtime_update_state(&self.state, "TestWidget::update", |state| state.value += 1);
            ResourceState::NONE
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
            runtime_read_state(&self.state, "TestWidget::paint", |state| state.value);
        }
    }

    impl WidgetStateOwner for TestWidget {
        type State = TestState;

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    struct TestBuilder;

    impl WidgetBuilder for TestBuilder {
        type Parameters = TestParameters;
        type W = TestWidget;

        fn create_widget(parameters: Self::Parameters) -> Self::W {
            TestWidget {
                state: Rc::new(RefCell::new(TestState { value: parameters.value })),
                opt: WidgetOption::NONE,
            }
        }
    }

    struct UnitParameters;

    impl WidgetParameters for UnitParameters {}

    struct UnitWidget {
        state: Rc<RefCell<()>>,
        opt: WidgetOption,
    }

    impl WidgetStateOwner for UnitWidget {
        type State = ();

        fn state_handle(&self) -> WidgetStateHandle<Self::State> {
            WidgetStateHandle::new(&self.state)
        }
    }

    impl Widget for UnitWidget {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            runtime_read_state(&self.state, "UnitWidget::measure", |_| Dimensioni::new(1, 1))
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Vec<UiInputEvent>) -> ResourceState {
            runtime_update_state(&self.state, "UnitWidget::update", |_| ());
            ResourceState::NONE
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
            runtime_read_state(&self.state, "UnitWidget::paint", |_| ());
        }
    }

    struct UnitBuilder;

    impl WidgetBuilder for UnitBuilder {
        type Parameters = UnitParameters;
        type W = UnitWidget;

        fn create_widget(_parameters: Self::Parameters) -> Self::W {
            UnitWidget {
                state: Rc::new(RefCell::new(())),
                opt: WidgetOption::NONE,
            }
        }
    }

    #[test]
    fn runtime_retains_one_strong_owner_and_exposes_only_weak_handles() {
        let widget = TestBuilder::create_widget(TestParameters { value: 7 });
        let state = widget.state_handle();

        assert_eq!(Rc::strong_count(&widget.state), 1);
        assert_eq!(state.try_read(|state| state.value), Some(7));
        assert_eq!(state.try_update(|state| state.value + 1), Some(8));

        let clone = state.clone();
        assert_eq!(Rc::strong_count(&widget.state), 1);
        assert_eq!(clone.try_read(|state| state.value), Some(7));

        drop(widget);
        assert!(!state.is_alive());
        assert_eq!(state.try_read(|_| ()), None);
    }

    #[test]
    fn handle_clone_does_not_require_clone_state() {
        fn clone_handle(handle: &WidgetStateHandle<NonCloneState>) -> WidgetStateHandle<NonCloneState> {
            handle.clone()
        }

        let owner = Rc::new(RefCell::new(NonCloneState));
        let handle = WidgetStateHandle::new(&owner);
        let clone = clone_handle(&handle);

        assert!(clone.is_alive());
        assert_eq!(Rc::strong_count(&owner), 1);
    }

    #[test]
    fn same_cell_conflicts_are_unavailable_and_cross_cell_access_succeeds() {
        let first_widget = TestBuilder::create_widget(TestParameters { value: 1 });
        let second_widget = TestBuilder::create_widget(TestParameters { value: 2 });
        let first = first_widget.state_handle();
        let first_clone = first.clone();
        let second = second_widget.state_handle();

        first
            .try_update(|first_state| {
                assert_eq!(first_clone.try_read(|_| ()), None);
                assert_eq!(first_clone.try_update(|_| ()), None);
                assert_eq!(second.try_read(|state| state.value), Some(2));
                first_state.value = 3;
            })
            .unwrap();

        assert_eq!(first.try_read(|state| state.value), Some(3));
    }

    #[test]
    fn active_access_keeps_state_alive_until_its_closure_returns() {
        let widget = TestBuilder::create_widget(TestParameters { value: 4 });
        let state = widget.state_handle();
        let widget = RefCell::new(Some(widget));

        state
            .try_read(|_| {
                drop(widget.borrow_mut().take());
                assert!(state.is_alive());
            })
            .unwrap();

        assert!(!state.is_alive());

        let widget = TestBuilder::create_widget(TestParameters { value: 5 });
        let state = widget.state_handle();
        let widget = RefCell::new(Some(widget));
        state
            .try_update(|_| {
                drop(widget.borrow_mut().take());
                assert!(state.is_alive());
            })
            .unwrap();
        assert!(!state.is_alive());
    }

    #[test]
    fn update_with_preserves_input_when_state_access_is_unavailable() {
        let widget = TestBuilder::create_widget(TestParameters { value: 0 });
        let state = widget.state_handle();
        let closure_called = Rc::new(Cell::new(false));

        let conflicted_input = state
            .try_read(|_| {
                let closure_called = closure_called.clone();
                state
                    .try_update_with(String::from("conflicted"), |_, _| {
                        closure_called.set(true);
                    })
                    .unwrap_err()
            })
            .unwrap();
        assert_eq!(conflicted_input, "conflicted");
        assert!(state.is_alive());
        assert!(!closure_called.get());

        let committed = state
            .try_update_with(String::from("committed"), |state, input| {
                state.value = input.len();
                input
            })
            .unwrap();
        assert_eq!(committed, "committed");

        drop(widget);
        let expired_input = state
            .try_update_with(String::from("expired"), |_, _| {
                closure_called.set(true);
            })
            .unwrap_err();
        assert_eq!(expired_input, "expired");
        assert!(!state.is_alive());
        assert!(!closure_called.get());
    }

    #[test]
    fn discarded_unit_handle_does_not_change_runtime_state_ownership() {
        let widget = UnitBuilder::create_widget(UnitParameters);
        let state = widget.state_handle();
        assert_eq!(Rc::strong_count(&widget.state), 1);

        drop(state);
        assert_eq!(Rc::strong_count(&widget.state), 1);
        runtime_read_state(&widget.state, "UnitWidget::test", |_| ());
    }
}
