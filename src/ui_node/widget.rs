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

use crate::{FrameRole, SurfaceRole};

use std::cell::RefCell;
use std::cmp::max;
use std::rc::{Rc, Weak};

use bitflags::bitflags;
use rs_math3d::Dimensioni;

use crate::atlas::AtlasHandle;
use crate::input::{Key, Modifiers};
use crate::theme::Skin;
use crate::Constraints;
use super::UiInputEvent;
pub use super::widget_context::{WidgetPaintCtx, WidgetUpdateCtx};

/// Concrete widget state and the only mutation marker needed by context-free typed handles.
///
/// The marker is written while the handle already owns this cell's exclusive borrow. Tree
/// traversal later consumes it and invalidates node-local caches on the recursive call stack.
pub(crate) struct WidgetStorage<W: ?Sized> {
    /// Whether typed application mutation invalidated this widget's preferred measurement.
    measurement_dirty: bool,
    /// Concrete or erased widget behavior stored behind this one common retained record.
    pub(crate) widget: W,
}

impl<W> WidgetStorage<W> {
    /// Wraps one newly mounted concrete widget with clean derived state.
    pub(crate) fn new(widget: W) -> Self {
        // A new node has no cached measurement to invalidate.
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
        /// Gives the widget a Skin-owned outer border and inset content rectangle.
        const FRAME = 512;
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

bitflags! {
    #[derive(Copy, Clone, Debug, Default, Eq, Hash, PartialEq)]
    /// Declarative keyboard capabilities exposed by one retained widget surface.
    ///
    /// Focus and traversal flags describe routing eligibility; action flags map normalized key
    /// presses through [`KeyboardBehavior::action`]. Pointer capture remains independent, and
    /// concrete widgets continue to own their semantic values and typed event ports. Built-in
    /// controls opt into the smallest applicable set; custom widgets are inert by default.
    pub struct KeyboardBehavior: u16 {
        /// The surface can own persistent keyboard focus after an eligible pointer press.
        const FOCUSABLE = 1;
        /// Sequential Tab traversal includes this surface.
        ///
        /// Every Tab stop is also treated as focusable; callers need not combine both flags.
        const TAB_STOP = 2;
        /// Enter requests the control's primary action.
        const ACTIVATE_ENTER = 4;
        /// Space requests the control's primary action.
        const ACTIVATE_SPACE = 8;
        /// Left and right request a one-step decrease and increase respectively.
        const ADJUST_HORIZONTAL = 16;
        /// Down and up request a one-step decrease and increase respectively.
        const ADJUST_VERTICAL = 32;
        /// Left collapses and right expands a hierarchical control.
        const EXPAND_COLLAPSE = 64;
        /// F4, Alt+Down, Alt+Up, and Escape control a popup-bearing surface.
        const POPUP = 128;
        /// No keyboard focus or traversal behavior.
        const NONE = 0;
    }
}

/// Widget-level meaning derived from a normalized keyboard transition.
///
/// This small semantic vocabulary prevents each built-in control from independently interpreting
/// platform keys. It also lets custom widgets reuse the same Windows-style mapping while retaining
/// their own state and typed application events.
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum KeyboardAction {
    /// Invoke the control's primary action.
    Activate,
    /// Move one logical step toward the lower value.
    Decrease,
    /// Move one logical step toward the higher value.
    Increase,
    /// Reveal a hierarchical branch or popup.
    Expand,
    /// Hide a hierarchical branch or popup.
    Collapse,
}

impl KeyboardBehavior {
    /// Returns whether pointer or programmatic routing may assign keyboard focus to this surface.
    pub const fn is_focusable(self) -> bool {
        self.intersects(Self::FOCUSABLE.union(Self::TAB_STOP))
    }

    /// Returns whether sequential traversal should visit this surface.
    pub const fn is_tab_stop(self) -> bool {
        self.intersects(Self::TAB_STOP)
    }

    /// Maps one routed key press to the action declared by these capabilities.
    pub fn action(self, input: Option<&UiInputEvent>) -> Option<KeyboardAction> {
        let UiInputEvent::Key { event } = input? else {
            return None;
        };
        if !event.is_pressed() {
            // Releases affect neither semantic values nor one-shot submission ports.
            return None;
        }

        let system_modifiers = Modifiers::ALT | Modifiers::CTRL | Modifiers::SUPER;
        let plain_command = !event.modifiers.intersects(system_modifiers);

        // Popup chords are intentionally resolved before plain directional behavior. Alt is part
        // of the command itself for Windows-style combo opening and closing.
        if self.intersects(Self::POPUP) {
            if event.key == Key::ArrowDown && event.modifiers.intersects(Modifiers::ALT) {
                return Some(KeyboardAction::Expand);
            }
            if event.key == Key::ArrowUp && event.modifiers.intersects(Modifiers::ALT) {
                return Some(KeyboardAction::Collapse);
            }
            if event.key == Key::Escape && !event.modifiers.intersects(Modifiers::CTRL | Modifiers::SUPER) {
                return Some(KeyboardAction::Collapse);
            }
            if event.key == Key::Function(4) && plain_command && !event.repeat {
                return Some(KeyboardAction::Activate);
            }
        }

        if !plain_command {
            // Ctrl, Alt, and Super chords remain available to application shortcuts. Shift alone
            // does not suppress ordinary control activation or directional adjustment.
            return None;
        }
        match event.key {
            // Activation is one-shot for a held key; directional actions intentionally retain
            // repeat so sliders, spin controls, and expanded trees respond continuously.
            Key::Enter if self.intersects(Self::ACTIVATE_ENTER) && !event.repeat => Some(KeyboardAction::Activate),
            Key::Space if self.intersects(Self::ACTIVATE_SPACE) && !event.repeat => Some(KeyboardAction::Activate),
            Key::ArrowLeft if self.intersects(Self::EXPAND_COLLAPSE) => Some(KeyboardAction::Collapse),
            Key::ArrowRight if self.intersects(Self::EXPAND_COLLAPSE) => Some(KeyboardAction::Expand),
            Key::ArrowLeft if self.intersects(Self::ADJUST_HORIZONTAL) => Some(KeyboardAction::Decrease),
            Key::ArrowRight if self.intersects(Self::ADJUST_HORIZONTAL) => Some(KeyboardAction::Increase),
            Key::ArrowDown if self.intersects(Self::ADJUST_VERTICAL) => Some(KeyboardAction::Decrease),
            Key::ArrowUp if self.intersects(Self::ADJUST_VERTICAL) => Some(KeyboardAction::Increase),
            _ => None,
        }
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
    ///
    /// Successful access conservatively dirties retained measurement before `f` runs. A visible
    /// tree therefore requires [`crate::Context::update_ui`] before its next render.
    pub fn try_update<R>(&self, f: impl FnOnce(&mut W) -> R) -> Option<R> {
        let widget = self.widget.upgrade()?;
        let mut widget = widget.try_borrow_mut().ok()?;
        widget.mark_measurement_dirty();
        let result = f(&mut widget.widget);
        Some(result)
    }

    /// Mutates a widget while preserving an owned input when access cannot begin.
    ///
    /// As with [`Self::try_update`], successful access dirties retained measurement before `f`.
    pub fn try_update_with<I, R>(&self, input: I, f: impl FnOnce(&mut W, I) -> R) -> Result<R, I> {
        let widget = match self.widget.upgrade() {
            Some(widget) => widget,
            None => return Err(input),
        };
        let mut widget = match widget.try_borrow_mut() {
            Ok(widget) => widget,
            Err(_) => return Err(input),
        };
        widget.mark_measurement_dirty();
        let result = f(&mut widget.widget, input);
        Ok(result)
    }

    /// Mutates derived or interaction state without invalidating the current UI commit.
    ///
    /// The mutation's derived update state may lag until the next Context update.
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
/// layout before considering the next queued event. [`Widget::keyboard_behavior`] is the sole
/// keyboard-eligibility query. Event-kind filtering belongs to the dispatcher for ordinary widgets
/// and to [`crate::ContainerWidget`] for an overloaded container surface.
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
    /// neither may alter the current commit nor publish application-coordination events. Mutating
    /// retained UI through an independently captured typed widget handle is a contract violation
    /// rather than a deferred-next-frame operation.
    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>);
    /// Returns the effective widget options used by generic dispatch.
    ///
    /// Widgets can override this to apply dynamic option adjustments.
    fn effective_widget_opt(&self) -> WidgetOption {
        *self.widget_opt()
    }
    /// Returns the semantic appearance used when [`WidgetOption::FRAME`] is effective.
    ///
    /// The runtime uses this one role for measurement, layout, input localization, and paint. A
    /// custom widget therefore changes themed border geometry without duplicating or bypassing the
    /// retained outer/content-box contract.
    fn frame_appearance_role(&self) -> FrameRole {
        // Generic framing remains the neutral default for application widgets that request FRAME
        // without opting into one of the built-in control meanings.
        FrameRole::Surface(SurfaceRole::GenericFrame)
    }
    /// Returns declarative keyboard routing capabilities for this widget surface.
    ///
    /// Keyboard focus is persistent and owned by retained routing rather than by pointer capture.
    /// Override this method for focusable custom widgets; the default keeps structural drawing and
    /// structural containers out of Tab order without another opt-out flag.
    fn keyboard_behavior(&self) -> KeyboardBehavior {
        KeyboardBehavior::NONE
    }
}

/// Intrinsic geometry contract for a retained leaf widget.
///
/// Each constraint axis explicitly distinguishes a finite maximum from an unbounded preferred-size
/// query. Returned components are clamped to zero before the node's frame is applied.
pub trait LeafWidget: Widget {
    /// Returns this leaf's preferred content size.
    fn measure(&self, skin: &Skin, atlas: &AtlasHandle, constraints: Constraints) -> Dimensioni;
}

impl Widget for WidgetOption {
    fn widget_opt(&self) -> &WidgetOption {
        self
    }
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for WidgetOption {
    fn measure(&self, style: &Skin, atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // Internal placeholder widgets reserve enough room for text or an expand icon.
        let padding = style.metrics.padding.max(0);
        let vertical_pad = max(1, padding / 2);
        let font_height = atlas.get_font_height(style.resolve_font_role(atlas, crate::FontRole::Body)) as i32;
        let icon_height = atlas.get_icon_size(style.resolve_icon_role(atlas, crate::IconRole::ExpandDown)).height;
        let content = max(font_height, icon_height).max(0);
        // Valid font metrics and application skin values may independently reach i32 limits;
        // preferred geometry clamps rather than wrapping before the parent applies constraints.
        let height = content.saturating_add(vertical_pad.saturating_mul(2));
        let width = padding.saturating_mul(2).saturating_add(content);
        Dimensioni::new(width, height)
    }
}

#[cfg(test)]
mod widget_tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;
    use crate::test_support::AllocationMeasurement;

    /// Constructs one routed key press for concise capability-mapping assertions.
    fn key(key: Key, modifiers: Modifiers) -> UiInputEvent {
        UiInputEvent::Key {
            event: crate::KeyEvent::pressed(key, modifiers),
        }
    }

    #[test]
    fn keyboard_actions_apply_shared_windows_control_mappings() {
        let push_button = KeyboardBehavior::ACTIVATE_ENTER | KeyboardBehavior::ACTIVATE_SPACE;
        assert_eq!(push_button.action(Some(&key(Key::Enter, Modifiers::NONE))), Some(KeyboardAction::Activate));
        assert_eq!(push_button.action(Some(&key(Key::Space, Modifiers::SHIFT))), Some(KeyboardAction::Activate));
        assert_eq!(push_button.action(Some(&key(Key::Enter, Modifiers::CTRL))), None);
        let repeated_space = UiInputEvent::Key {
            event: crate::KeyEvent::pressed(Key::Space, Modifiers::NONE).repeated(),
        };
        assert_eq!(push_button.action(Some(&repeated_space)), None, "held activation must remain one-shot");

        let checkbox = KeyboardBehavior::ACTIVATE_SPACE;
        assert_eq!(checkbox.action(Some(&key(Key::Enter, Modifiers::NONE))), None);
        assert_eq!(checkbox.action(Some(&key(Key::Space, Modifiers::NONE))), Some(KeyboardAction::Activate));

        let horizontal = KeyboardBehavior::ADJUST_HORIZONTAL;
        assert_eq!(horizontal.action(Some(&key(Key::ArrowLeft, Modifiers::NONE))), Some(KeyboardAction::Decrease));
        assert_eq!(horizontal.action(Some(&key(Key::ArrowRight, Modifiers::NONE))), Some(KeyboardAction::Increase));

        let popup = KeyboardBehavior::POPUP;
        assert_eq!(popup.action(Some(&key(Key::Function(4), Modifiers::NONE))), Some(KeyboardAction::Activate));
        assert_eq!(popup.action(Some(&key(Key::ArrowDown, Modifiers::ALT))), Some(KeyboardAction::Expand));
        assert_eq!(popup.action(Some(&key(Key::Escape, Modifiers::NONE))), Some(KeyboardAction::Collapse));
    }

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
        fn measure(&self, _style: &Skin, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
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
    fn panicking_typed_update_preserves_measurement_invalidation() {
        let widget = TestBuilder::create_widget(TestParameters { value: 0 });
        let (state, mut node) = crate::Node::typed_widget(widget);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), usize> = state.try_update_with(1, |state, value| {
                state.value = value;
                panic!("partial mutation");
            });
        }));

        assert!(result.is_err());
        assert!(node.synchronize_measurement_invalidation());
        assert!(!node.synchronize_measurement_invalidation());
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
