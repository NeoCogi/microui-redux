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

use crate::atlas::AtlasHandle;
use crate::theme::Style;
use super::UiInputEvent;
pub use super::widget_context::{WidgetPaintCtx, WidgetUpdateCtx};

/// Concrete widget state and the only mutation marker needed by context-free typed handles.
///
/// The marker is written while the handle already owns this cell's exclusive borrow. Tree
/// traversal later consumes it and invalidates node-local caches on the recursive call stack.
pub(crate) struct WidgetStorage<W: ?Sized> {
    measurement_dirty: bool,
    pub(crate) widget: W,
}

impl<W> WidgetStorage<W> {
    pub(crate) fn new(widget: W) -> Self {
        Self { measurement_dirty: false, widget }
    }
}

impl<W: ?Sized> WidgetStorage<W> {
    pub(crate) fn mark_measurement_dirty(&mut self) {
        self.measurement_dirty = true;
    }

    pub(crate) fn is_measurement_dirty(&self) -> bool {
        self.measurement_dirty
    }

    pub(crate) fn take_measurement_dirty(&mut self) -> bool {
        std::mem::take(&mut self.measurement_dirty)
    }
}

bitflags! {
    #[derive(Copy, Clone)]
    /// Controls which widget states should draw a filled background.
    pub struct WidgetFillOption : u32 {
        /// Fill the background for the idle/normal state.
        const NORMAL = 1;
        /// Fill the background while hovered.
        const HOVER = 2;
        /// Fill the background while the widget owns focus.
        const CLICK = 4;
        /// Fill the background for every interaction state.
        const ALL = Self::NORMAL.bits() | Self::HOVER.bits() | Self::CLICK.bits();
    }
}

bitflags! {
    #[derive(Copy, Clone)]
    /// Widget-specific options that influence layout and interactivity.
    pub struct WidgetOption : u32 {
        /// Gives the widget a Style-owned outer border and inset content rectangle.
        const FRAME = 512;
        /// Keeps keyboard focus after release until routing moves it or the node becomes unavailable.
        const HOLD_FOCUS = 256;
        /// Consumes scroll input while the widget is hovered.
        const GRAB_SCROLL = 32;
        /// Disables interaction with this widget's own surface; eligible descendants remain interactive.
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

/// Marker trait for one-shot widget construction input.
///
/// Parameters seed and configure one concrete runtime. Values that applications mutate after
/// construction live directly in the builder's associated widget.
pub trait WidgetParameters: 'static {}

/// Cloneable, non-owning typed access to one concrete retained widget.
///
/// A [`crate::Node`] owns the only persistent strong reference after construction. Containers and
/// the runtime retains that widget through an erased leaf or container trait object, while
/// application code and coordinating widgets may keep this typed weak view. Removing the node
/// therefore makes every typed handle expire instead of keeping an invisible widget alive.
pub struct TypedWidgetHandle<W: Widget + 'static> {
    widget: Weak<RefCell<WidgetStorage<W>>>,
}

impl<W: Widget + 'static> Clone for TypedWidgetHandle<W> {
    fn clone(&self) -> Self {
        Self { widget: self.widget.clone() }
    }
}

impl<W: Widget + 'static> TypedWidgetHandle<W> {
    /// Creates a weak typed view of the allocation that will be erased into a retained node.
    pub(crate) fn new(widget: &Rc<RefCell<WidgetStorage<W>>>) -> Self {
        Self { widget: Rc::downgrade(widget) }
    }

    /// Reports whether the retained tree still owns this widget.
    pub fn is_alive(&self) -> bool {
        self.widget.upgrade().is_some()
    }

    /// Runs a widget-specific read operation when the widget is alive and not mutably borrowed.
    ///
    /// Built-in widgets expose common semantic operations as methods on their specialized handle.
    /// This closure form remains available for custom widget APIs and compound operations.
    pub fn try_read<R>(&self, f: impl FnOnce(&W) -> R) -> Option<R> {
        let widget = self.widget.upgrade()?;
        let widget = widget.try_borrow().ok()?;
        Some(f(&widget.widget))
    }

    /// Runs a widget-specific mutation when the widget is alive and not otherwise borrowed.
    pub fn try_update<R>(&self, f: impl FnOnce(&mut W) -> R) -> Option<R> {
        let widget = self.widget.upgrade()?;
        let mut widget = widget.try_borrow_mut().ok()?;
        let result = f(&mut widget.widget);
        widget.mark_measurement_dirty();
        Some(result)
    }

    /// Mutates a widget while preserving an owned input when access cannot begin.
    pub fn try_update_with<I, R>(&self, input: I, f: impl FnOnce(&mut W, I) -> R) -> Result<R, I> {
        let widget = match self.widget.upgrade() {
            Some(widget) => widget,
            None => return Err(input),
        };
        let mut widget = match widget.try_borrow_mut() {
            Ok(widget) => widget,
            Err(_) => return Err(input),
        };
        let result = f(&mut widget.widget, input);
        widget.mark_measurement_dirty();
        Ok(result)
    }

    /// Mutates derived or interaction state known not to affect preferred measurement.
    pub(crate) fn try_update_without_measurement<R>(&self, f: impl FnOnce(&mut W) -> R) -> Option<R> {
        let widget = self.widget.upgrade()?;
        let mut widget = widget.try_borrow_mut().ok()?;
        Some(f(&mut widget.widget))
    }
}

/// Associates one-shot construction parameters with one concrete widget runtime.
pub trait WidgetBuilder: Sized + 'static {
    /// One-shot input consumed during construction.
    type Parameters: WidgetParameters;
    /// Concrete runtime created by this builder.
    type W: LeafWidget + 'static;

    /// Consumes parameters and creates the concrete widget before it is mounted and erased.
    fn create_widget(parameters: Self::Parameters) -> Self::W;
}

/// Common retained runtime phase contract implemented by concrete widgets.
///
/// Widgets participate in the retained update and paint phases. Geometry is deliberately split by
/// node kind: [`LeafWidget`] measures intrinsic content, while [`crate::ContainerWidget`] measures
/// and places an authoritative child collection. A branch therefore has no meaningless leaf
/// measurement implementation.
///
/// The runtime routes an event before running that event's complete update traversal, then commits
/// layout before considering the next queued event. [`Widget::focus_policy`] is the sole focus
/// policy query. Event-kind filtering belongs to the dispatcher for ordinary widgets and to
/// [`crate::ContainerWidget`] for an overloaded container surface.
pub trait Widget {
    /// Returns the widget options for this state.
    fn widget_opt(&self) -> &WidgetOption;
    /// Updates retained widget state for exactly one normalized input event.
    ///
    /// Pointer positions in `input` are relative to the widget's derived content rectangle, using
    /// the same origin as [`WidgetUpdateCtx::local_rect`]. Outer frame pixels remain part of the
    /// runtime hit target, so a pointer event on the border may lie just outside the local content
    /// bounds. At most one eligible widget receives `Some(input)` during a traversal; every other
    /// eligible widget receives `None`. Held state is available from [`WidgetUpdateCtx`]. The
    /// update context intentionally cannot record drawing commands. Implementations that retain a
    /// local drag mode must reconcile it from [`WidgetUpdateCtx::active`] on every call; pointer
    /// capture is runtime-owned and has no separate widget lifecycle callback. Delivery of a
    /// consumed or captured event conservatively invalidates this node's retained measurement and
    /// its dependent ancestors; widgets do not report cache effects themselves.
    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, input: Option<&UiInputEvent>);
    /// Records paint commands through paint-only capabilities.
    ///
    /// Paint is observational with respect to application-authored semantic state, topology,
    /// interaction, and committed layout. Implementations may maintain private rendering caches or
    /// publish framework-owned, paint-derived read-only geometry for later application use, but
    /// neither may alter the current commit. Mutating retained UI through an independently captured
    /// typed widget handle is a contract violation rather than a deferred-next-frame operation.
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

/// Intrinsic geometry contract for a retained leaf widget.
///
/// A positive `avail` component is available for wrapping or other responsive content. A
/// non-positive component requests the unconstrained preferred size on that axis. Returned
/// components are clamped to zero before the node's frame and parent placement policy are applied.
pub trait LeafWidget: Widget {
    /// Returns this leaf's preferred content size.
    fn measure(&self, style: &Style, atlas: &AtlasHandle, avail: Dimensioni) -> Dimensioni;
}

impl Widget for WidgetOption {
    fn widget_opt(&self) -> &WidgetOption {
        self
    }
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for WidgetOption {
    fn measure(&self, style: &Style, atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
        // Internal placeholder widgets reserve enough room for text or an expand icon.
        let padding = style.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.font) as i32;
        let icon_height = atlas.get_icon_size(style.icons.expand_down).height;
        let content = max(font_height, icon_height);
        let height = (content + vertical_pad * 2).max(0);
        let width = (padding * 2 + content).max(0);
        Dimensioni::new(width, height)
    }
}

#[cfg(test)]
mod widget_ownership_tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::test_support::AllocationMeasurement;

    struct TestParameters {
        value: usize,
    }

    impl WidgetParameters for TestParameters {}

    struct TestWidget {
        value: usize,
        opt: WidgetOption,
    }

    impl Widget for TestWidget {
        fn widget_opt(&self) -> &WidgetOption {
            &self.opt
        }

        fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {
            self.value += 1;
        }

        fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
            let _ = self.value;
        }
    }

    impl LeafWidget for TestWidget {
        fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _avail: Dimensioni) -> Dimensioni {
            Dimensioni::new(1, 1)
        }
    }

    struct TestBuilder;

    impl WidgetBuilder for TestBuilder {
        type Parameters = TestParameters;
        type W = TestWidget;

        fn create_widget(parameters: Self::Parameters) -> Self::W {
            TestWidget {
                value: parameters.value,
                opt: WidgetOption::NONE,
            }
        }
    }

    #[test]
    fn runtime_retains_one_strong_owner_and_exposes_only_weak_handles() {
        let widget = TestBuilder::create_widget(TestParameters { value: 7 });
        let (state, node) = crate::Node::typed_widget(widget);

        assert_eq!(state.try_read(|state| state.value), Some(7));
        assert_eq!(state.try_update(|state| state.value += 1), Some(()));

        let clone = state.clone();
        assert_eq!(clone.try_read(|state| state.value), Some(8));

        drop(node);
        assert!(!state.is_alive());
        assert_eq!(state.try_read(|_| ()), None);
    }

    #[test]
    fn checked_widget_handle_reads_and_updates_allocate_nothing() {
        let widget = TestBuilder::create_widget(TestParameters { value: 0 });
        let (state, _node) = crate::Node::typed_widget(widget);

        // Warm the checked upgrade and borrow paths before isolating their steady-state cost.
        state.try_read(|state| state.value).unwrap();
        state.try_update(|state| state.value += 1).unwrap();

        let measurement = AllocationMeasurement::begin();
        for _ in 0..1_000 {
            state.try_update(|state| state.value += 1).unwrap();
            state.try_read(|state| state.value).unwrap();
        }
        let allocations = measurement.finish();

        assert_eq!(allocations.events, 0, "checked state access allocated {} bytes", allocations.bytes);
        assert_eq!(state.try_read(|state| state.value), Some(1_001));
    }

    #[test]
    fn handle_clone_does_not_require_clone_widget() {
        fn clone_handle(handle: &TypedWidgetHandle<TestWidget>) -> TypedWidgetHandle<TestWidget> {
            handle.clone()
        }

        let (handle, _node) = crate::Node::typed_widget(TestBuilder::create_widget(TestParameters { value: 0 }));
        let clone = clone_handle(&handle);

        assert!(clone.is_alive());
    }

    #[test]
    fn same_cell_conflicts_are_unavailable_and_cross_cell_access_succeeds() {
        let first_widget = TestBuilder::create_widget(TestParameters { value: 1 });
        let second_widget = TestBuilder::create_widget(TestParameters { value: 2 });
        let (first, _first_node) = crate::Node::typed_widget(first_widget);
        let first_clone = first.clone();
        let (second, _second_node) = crate::Node::typed_widget(second_widget);

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
        let (state, node) = crate::Node::typed_widget(widget);
        let widget = RefCell::new(Some(node));

        state
            .try_read(|_| {
                drop(widget.borrow_mut().take());
                assert!(state.is_alive());
            })
            .unwrap();

        assert!(!state.is_alive());

        let widget = TestBuilder::create_widget(TestParameters { value: 5 });
        let (state, node) = crate::Node::typed_widget(widget);
        let widget = RefCell::new(Some(node));
        state
            .try_update(|_| {
                drop(widget.borrow_mut().take());
                assert!(state.is_alive());
            })
            .unwrap();
        assert!(!state.is_alive());
    }

    #[test]
    fn update_with_preserves_input_when_widget_access_is_unavailable() {
        let widget = TestBuilder::create_widget(TestParameters { value: 0 });
        let (state, node) = crate::Node::typed_widget(widget);
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

        drop(node);
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
    fn discarded_handle_does_not_change_runtime_widget_ownership() {
        let (handle, node) = crate::Node::typed_widget(TestBuilder::create_widget(TestParameters { value: 9 }));
        let observer = handle.clone();
        drop(handle);
        assert_eq!(observer.try_read(|widget| widget.value), Some(9));
        drop(node);
        assert!(!observer.is_alive());
    }
}
