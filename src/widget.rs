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
//! Widget runtime contracts and retained event tracking.

use std::cell::RefCell;
use std::cmp::max;
use std::rc::{Rc, Weak};

use bitflags::bitflags;
use rs_math3d::Dimensioni;

use crate::atlas::{AtlasHandle, EXPAND_DOWN_ICON};
use crate::style::Style;
use crate::ui_node::UiInputEvent;
pub use crate::widget_ctx::{WidgetPaintCtx, WidgetUpdateCtx};

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
    /// Focus remains after release until routing moves it to another widget or the node is hidden or removed.
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
/// handle never keeps removed state alive and does not require `T: Clone`. Dropping every handle
/// likewise has no effect on the mounted runtime.
///
/// Access closures must finish before an update, layout, or paint traversal can reach the same
/// state. Same-cell reentrancy fails without invoking the inner closure; access to an independent
/// state cell may be nested. A [`crate::ContextFrame`] does not lock these handles and there is no
/// Context access token. If state that can affect layout changes after the last
/// [`crate::Context::update_ui`] commit, cancel any unsubmitted frame and commit again before
/// painting.
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
    /// through the resulting handle. This constructor is the advanced downstream
    /// [`WidgetStateOwner`] conformance boundary: the supplied `Rc<RefCell<T>>` must be the same
    /// private allocation used by every runtime phase, and the runtime must remain its only
    /// persistent strong owner. Applications ordinarily receive handles from built-in `create`
    /// constructors instead of calling this method.
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
    ///
    /// Use this operation when `input` transfers ownership, such as a still-unmounted
    /// [`crate::Node`]. Both an expired state owner and a same-cell borrow conflict return the exact
    /// input value unchanged.
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
            "retained widget state invariant violated during {phase}: associated state is already borrowed; application state-access closures must finish before retained update, layout, or paint traversal"
        )
    });
    f(&state)
}

/// Updates associated state during retained runtime dispatch or reports an invariant violation.
pub(crate) fn runtime_update_state<T: WidgetState, R>(state: &Rc<RefCell<T>>, phase: &'static str, f: impl FnOnce(&mut T) -> R) -> R {
    let mut state = state.try_borrow_mut().unwrap_or_else(|_| {
        panic!(
            "retained widget state invariant violated during {phase}: associated state is already borrowed; application state-access closures must finish before retained update, layout, or paint traversal"
        )
    });
    f(&mut state)
}

/// Common retained runtime phase contract implemented by concrete widgets.
///
/// Widgets participate in three retained execution phases:
/// 1. `measure`, which reports intrinsic size for an explicit layout commit.
/// 2. `update`, which applies exactly one routed event (or `None`) and mutates widget-local state.
/// 3. `paint`, which records paint commands for already committed widget state.
///
/// The runtime routes an event before running that event's complete update traversal, then commits
/// layout before considering the next queued event. [`Widget::focus_policy`] is the sole focus
/// policy query; container input helpers do not accept a parallel policy value.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Returns the intrinsic widget size for the current explicit layout pass.
    ///
    /// `avail` reports the current container body size visible to the widget.
    /// Values less than or equal to zero are treated as "use layout defaults" for that axis.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
    /// Updates retained widget state for exactly one normalized input event.
    ///
    /// Pointer positions in `input` are relative to the widget's derived content rectangle, using
    /// the same origin as [`WidgetUpdateCtx::local_rect`]. Outer frame pixels remain part of the
    /// runtime hit target, so a pointer event on the border may lie just outside the local content
    /// bounds. At most one eligible widget receives `Some(input)` during a traversal; every other
    /// eligible widget receives `None`. Held state is available from [`WidgetUpdateCtx`]. The
    /// update context intentionally cannot record drawing commands.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>);
    /// Records paint commands through paint-only capabilities.
    ///
    /// Paint is observational with respect to semantic widget state. Implementations may maintain
    /// rendering caches, but must not make behavior or future layout depend on paint having run.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>);
    /// Returns the effective widget options used by generic dispatch.
    ///
    /// Widgets can override this to apply dynamic option adjustments.
    fn effective_widget_opt(&self) -> WidgetOption {
        *self.widget_opt()
    }
    /// Returns the focus behavior used by generic dispatch.
    ///
    /// Override this when options alone do not describe the runtime's focus lifecycle. The
    /// retained tree owns focused-node identity; widgets can request policy but cannot read,
    /// replace, or transfer that identity.
    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::from_widget_options(self.effective_widget_opt())
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

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

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

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
            runtime_update_state(&self.state, "TestWidget::update", |state| state.value += 1);
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

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
            runtime_update_state(&self.state, "UnitWidget::update", |_| ());
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
