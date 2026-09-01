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

use super::*;

use crate::test_support::{AllocationMeasurement, NoopRenderer, RenderEvent, recording_backend, test_atlas, test_style};
use crate::{
    color, rect, AppearanceRole, AtlasHandle, Button, ButtonParameters, ButtonSubmitted, Checkbox, CheckboxParameters, Combo, ComboParameters, ComboSubmitted,
    ControlColor, Custom, Color, CustomParameters, Constraints, Context, DecimalPrecision, Dimensioni, Disclosure, DisclosureParameters, Ui, Grid,
    GridParameters, Key, KeyEvent, KeyboardBehavior, Linear, LinearItem, LinearParameters, Menu, MenuBar, MenuItem, MenuItemMark, MenuItemParameters,
    MenuItemSubmitted, MouseButton, Node, NinePatch, ScrollArea, ScrollAreaOption, ListItem, ListItemParameters, ScrollAreaParameters, Slider,
    SliderParameters, StatefulAppearance, Style, TextArea, TextAreaParameters, Textbox, TextboxChanged, TextBlock, TextBlockParameters, TextboxParameters,
    TrackSize, TypedWidgetHandle, UiInputEvent, Vec2i, Widget, WidgetOption, WidgetPaintCtx, VisualState, WidgetUpdateCtx, Modifiers,
};
use crate::render::{FrameInfo, RenderError};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn context() -> Context<NoopRenderer> {
    Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240))
}

/// Constructs the smallest complete theme atlas with deliberately maximal text metrics.
fn extreme_metric_atlas() -> AtlasHandle {
    // Every semantic icon may share the one opaque-white texel: identity is name based, while this
    // fixture is concerned only with the propagation of valid font metrics through Context.
    let pixels = [0xFF, 0xFF, 0xFF, 0xFF];
    let icons = [
        ("white", Recti::new(0, 0, 1, 1)),
        ("close", Recti::new(0, 0, 1, 1)),
        ("expand", Recti::new(0, 0, 1, 1)),
        ("collapse", Recti::new(0, 0, 1, 1)),
        ("check", Recti::new(0, 0, 1, 1)),
        ("expand_down", Recti::new(0, 0, 1, 1)),
        ("open_folder", Recti::new(0, 0, 1, 1)),
        ("closed_folder", Recti::new(0, 0, 1, 1)),
        ("file", Recti::new(0, 0, 1, 1)),
    ];
    let glyphs = [(
        '_',
        crate::CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(i32::MAX, 0),
            rect: Recti::new(0, 0, 1, 1),
        },
    )];
    let fonts = [(
        "body",
        crate::FontEntry {
            line_size: i32::MAX as usize,
            baseline: i32::MAX,
            font_size: 1,
            entries: &glyphs,
        },
    )];
    let source = crate::AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: crate::SourceFormat::Raw,
    };

    AtlasHandle::try_from(&source).expect("maximal representable font metrics must form a valid atlas")
}

/// Verifies valid maximal font metrics remain total through layout, input routing, and rendering.
#[test]
fn context_handles_extreme_font_metrics_across_a_complete_commit() {
    let (_, button) = Button::create(ButtonParameters::new("_"));
    let button_id = button.id();
    let (_, content) = Linear::create(LinearParameters::horizontal([button]));
    let mut context = Context::new_test(NoopRenderer { atlas: extreme_metric_atlas() }, Dimensioni::new(320, 240));
    let root = context.ui().create_window(Window::new("extreme metrics", rect(20, 20, 140, 100), content));
    context
        .ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    // A Content track preserves intrinsic overflow, so the child's positive screen origin plus
    // i32::MAX extent crosses the integer boundary that the dependency's Recti methods cannot
    // evaluate. The complete empty-input commit must still update, clip, and render successfully.
    context.update_and_render_ui();
    let child = context.debug_root_node_rect(root.id(), button_id).expect("the extreme child must be laid out");
    assert!(child.x > 0 && child.y > 0);
    assert_eq!((child.width, child.height), (i32::MAX, i32::MAX));

    // Route a real pointer event through both window and retained-node hit tests, then repeat layout
    // and rendering so every production geometry consumer observes the same extreme rectangle.
    let body = context.debug_root_body(root.id()).expect("the fixed window must expose its body");
    context.mousemove(body.x.saturating_add(1), body.y.saturating_add(1));
    context.update_and_render_ui();
}

/// Delivers one complete left-button click to the center of a committed test rectangle.
fn click_rect(context: &mut Context<NoopRenderer>, target: Recti) {
    // Use the center rather than an edge so adjacent menu rows and popup borders cannot receive the
    // test input because of an inclusive/exclusive boundary detail.
    let x = target.x + target.width / 2;
    let y = target.y + target.height / 2;
    context.mousedown(x, y, MouseButton::LEFT);
    context.update_and_render_ui();
    context.mouseup(x, y, MouseButton::LEFT);
    context.update_and_render_ui();
}

fn empty_content() -> Node {
    Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1
}

fn frame_info(dimensions: Dimensioni) -> FrameInfo {
    FrameInfo::try_new(dimensions, color(0, 0, 0, 255)).unwrap()
}

/// Converts a Style color into the byte representation recorded by the renderer fixture.
fn recorded_color(color: Color) -> [u8; 4] {
    // Rendering preserves the public eight-bit color channels without normalization loss.
    [color.r, color.g, color.b, color.a]
}

/// Returns the uniform tint of one recorded atlas quad.
fn atlas_quad_color(event: &RenderEvent) -> Option<[u8; 4]> {
    let RenderEvent::AtlasQuad(vertices) = event else {
        return None;
    };
    let color = vertices[0].color;
    vertices.iter().all(|vertex| vertex.color == color).then_some(color)
}

/// Collects atlas quads whose complete tint matches one semantic Style color.
fn atlas_quads_with_color(events: &[RenderEvent], color: Color) -> Vec<&RenderEvent> {
    let expected = recorded_color(color);
    events.iter().filter(|event| atlas_quad_color(event) == Some(expected)).collect()
}

/// Converts external rectangle geometry into an equality-friendly test representation.
fn rect_values(rect: Recti) -> (i32, i32, i32, i32) {
    (rect.x, rect.y, rect.width, rect.height)
}

/// Leaf used when a test needs content with an exact intrinsic size.
struct DesiredSize(Dimensioni);

impl Widget for DesiredSize {
    fn widget_opt(&self) -> &WidgetOption {
        &WidgetOption::NONE
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for DesiredSize {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.0
    }
}

/// Measurement probe whose preferred width observes a style field omitted by the old cache key.
struct CompleteStyleMeasureProbe {
    /// Shared counter used after the uniquely owned probe moves into the retained tree.
    measures: Rc<Cell<usize>>,
    /// Stable noninteractive policy returned by reference through [`Widget::widget_opt`].
    opt: WidgetOption,
}

impl Widget for CompleteStyleMeasureProbe {
    /// Returns the probe's fixed interaction policy.
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    /// The probe has no eventless or routed semantic work.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {}

    /// Rendering is irrelevant to the measurement-cache contract under test.
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for CompleteStyleMeasureProbe {
    /// Derives preferred width from the complete public Style passed to custom leaf measurement.
    fn measure(&self, style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        // `menu_background` was intentionally absent from MeasurementStyleKey. Observing it here
        // proves global replacement invalidates custom measurements rather than merely the fields
        // currently used for sizing by built-in widgets.
        self.measures.set(self.measures.get() + 1);
        Dimensioni::new(i32::from(style.menu_background.r).max(1), 10)
    }
}

fn desired_size_node(width: i32, height: i32) -> Node {
    Node::widget(DesiredSize(Dimensioni::new(width, height)))
}

fn increment_event_counter<E>(count: &mut usize, _: &E) {
    *count += 1;
}

fn event_counter<E: crate::WidgetEvent>(port: crate::WidgetEventPortHandle<E>) -> crate::event::WidgetEventDispatcher<usize> {
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    dispatcher.subscribe(port, increment_event_counter::<E>).unwrap();
    dispatcher
}

struct OrderedProbe {
    events: Vec<&'static str>,
    held_buttons: Vec<u32>,
    held_keys: Vec<u8>,
    measures: Cell<usize>,
    updates: usize,
    paints: usize,
    hovered: bool,
    opt: WidgetOption,
}

impl OrderedProbe {
    fn create(opt: WidgetOption) -> (TypedWidgetHandle<Self>, Node) {
        Node::typed_widget(Self {
            events: Vec::new(),
            held_buttons: Vec::new(),
            held_keys: Vec::new(),
            measures: Cell::new(0),
            updates: 0,
            paints: 0,
            hovered: false,
            opt,
        })
    }
}

impl Widget for OrderedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        self.updates += 1;
        self.held_buttons.push(ctx.mouse_buttons().bits());
        self.held_keys.push(ctx.modifiers().bits());
        self.hovered = ctx.hovered();
        if let Some(event) = event {
            self.events.push(match event {
                UiInputEvent::MouseMove { .. } => "move",
                UiInputEvent::MouseDrag { .. } => "drag",
                UiInputEvent::MouseDown { .. } => "down",
                UiInputEvent::MouseUp { .. } => "up",
                UiInputEvent::Scroll { .. } => "scroll",
                UiInputEvent::Key { event } if event.is_pressed() => "key-down",
                UiInputEvent::Key { .. } => "key-up",
                UiInputEvent::Text { .. } => "text",
            });
        }
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {
        self.paints += 1;
    }

    fn keyboard_behavior(&self) -> KeyboardBehavior {
        // Root routing probes model an ordinary focusable application control. Individual tests
        // can still disable the complete surface through WidgetOption::NO_INTERACT.
        KeyboardBehavior::TAB_STOP
    }
}

impl crate::LeafWidget for OrderedProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        self.measures.set(self.measures.get() + 1);
        Dimensioni::new(80, 60)
    }
}

struct CommitProbe {
    intrinsic_height: i32,
    grow_to: Option<i32>,
    presses: usize,
    painted_rects: Rc<RefCell<Vec<Recti>>>,
    opt: WidgetOption,
}

impl CommitProbe {
    fn new(intrinsic_height: i32, grow_to: Option<i32>) -> (TypedWidgetHandle<Self>, Node, Rc<RefCell<Vec<Recti>>>) {
        let painted_rects = Rc::new(RefCell::new(Vec::new()));
        let probe = Self {
            intrinsic_height,
            grow_to,
            presses: 0,
            painted_rects: painted_rects.clone(),
            opt: WidgetOption::NONE,
        };
        let (handle, node) = Node::typed_widget(probe);
        (handle, node, painted_rects)
    }
}

impl Widget for CommitProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, event: Option<&UiInputEvent>) {
        if event.is_some()
            && let Some(grow_to) = self.grow_to.take()
        {
            self.intrinsic_height = grow_to;
        }
        if matches!(event, Some(UiInputEvent::MouseDown { .. })) {
            self.presses += 1;
        }
    }

    fn paint(&mut self, ctx: &mut WidgetPaintCtx<'_>) {
        // This is deliberately a rendering-only test cache: semantic state remains observational.
        self.painted_rects.borrow_mut().push(ctx.screen_content_rect());
    }
}

impl crate::LeafWidget for CommitProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(40, self.intrinsic_height)
    }
}

struct SiblingMutationProbe {
    value: i32,
    observed_during_update: Vec<i32>,
    target: Option<(TypedWidgetHandle<SiblingMutationProbe>, i32)>,
    opt: WidgetOption,
}

impl SiblingMutationProbe {
    fn new(value: i32, target: Option<(TypedWidgetHandle<SiblingMutationProbe>, i32)>) -> (TypedWidgetHandle<Self>, Node) {
        let probe = Self {
            value,
            observed_during_update: Vec::new(),
            target,
            opt: WidgetOption::NONE,
        };
        Node::typed_widget(probe)
    }
}

impl Widget for SiblingMutationProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        if let Some((target, value)) = &self.target {
            target
                .try_update(|state| state.value = *value)
                .expect("the sibling target must not be borrowed yet or anymore");
        }
        self.observed_during_update.push(self.value);
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for SiblingMutationProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

struct CountedProbe {
    updates: Rc<Cell<usize>>,
    opt: WidgetOption,
}

impl CountedProbe {
    fn new(updates: Rc<Cell<usize>>) -> Self {
        Self { updates, opt: WidgetOption::NONE }
    }
}

impl Widget for CountedProbe {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        self.updates.set(self.updates.get() + 1);
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for CountedProbe {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

struct TopologyMutator {
    same_container_blocked: bool,
    other_container_changed: bool,
    same_container: Rc<RefCell<Option<TypedWidgetHandle<Linear>>>>,
    other_container: TypedWidgetHandle<Linear>,
    candidate: Option<Node>,
    opt: WidgetOption,
}

impl Widget for TopologyMutator {
    fn widget_opt(&self) -> &WidgetOption {
        &self.opt
    }

    #[allow(clippy::result_large_err)] // Failed topology changes must preserve the unique node owner.
    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _event: Option<&UiInputEvent>) {
        let Some(candidate) = self.candidate.take() else { return };
        let same_container = self
            .same_container
            .borrow()
            .as_ref()
            .expect("outer container handle must be installed before traversal")
            .clone();
        let same_container_blocked = same_container.try_update(|state| state.remove_drop(usize::MAX)).flatten().is_none();
        let other_container_changed = match self
            .other_container
            .try_update_with(candidate, |state, node| state.push(node).map_err(crate::LinearItem::into_node))
        {
            Ok(Ok(())) => true,
            Ok(Err(candidate)) | Err(candidate) => {
                self.candidate = Some(candidate);
                false
            }
        };
        self.same_container_blocked = same_container_blocked;
        self.other_container_changed = other_container_changed;
    }

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl crate::LeafWidget for TopologyMutator {
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(20, 10)
    }
}

#[test]
fn routed_recipient_gets_one_event_while_every_node_still_updates_in_fifo_order() {
    let (state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 100, 80), probe));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(20, 20);
    ctx.mousedown(20, 20, MouseButton::LEFT);
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("x");
    ctx.key(KeyEvent::released(Key::Shift, Modifiers::NONE));
    ctx.mouseup(20, 20, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    state
        .try_read(|state| {
            assert_eq!(state.updates, 6);
            assert_eq!(state.events, ["move", "down", "key-down", "text", "key-up", "up"]);
            assert_eq!(
                state.held_buttons,
                [
                    MouseButton::NONE.bits(),
                    MouseButton::LEFT.bits(),
                    MouseButton::LEFT.bits(),
                    MouseButton::LEFT.bits(),
                    MouseButton::LEFT.bits(),
                    MouseButton::NONE.bits(),
                ]
            );
            assert_eq!(
                state.held_keys,
                [
                    Modifiers::NONE.bits(),
                    Modifiers::NONE.bits(),
                    Modifiers::SHIFT.bits(),
                    Modifiers::SHIFT.bits(),
                    Modifiers::NONE.bits(),
                    Modifiers::NONE.bits(),
                ]
            );
        })
        .unwrap();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.updates, 6, "the application node receives each event exactly once");
}

#[test]
fn tab_and_shift_tab_move_window_focus_without_reaching_widget_input() {
    let (first_state, first) = OrderedProbe::create(WidgetOption::NONE);
    let (second_state, second) = OrderedProbe::create(WidgetOption::NONE);
    let (_, content) = Linear::create(LinearParameters::vertical([first, second]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 100, 80), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    // The first forward command starts at the first eligible retained node. Tab's release is also
    // manager-owned and must not appear as a raw key-up on the newly focused widget.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::released(Key::Tab, Modifiers::NONE));
    ctx.text("first");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(first_state.try_read(|state| state.events.clone()), Some(vec!["text"]));
    assert_eq!(second_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    // A second forward command selects the second control, and Shift+Tab returns to the first.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.text("second");
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::SHIFT));
    ctx.text("first-again");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(first_state.try_read(|state| state.events.clone()), Some(vec!["text", "text"]));
    assert_eq!(second_state.try_read(|state| state.events.clone()), Some(vec!["text"]));
}

#[test]
/// Proves that a popup owns keyboard traversal until dismissal restores its parent's focused node.
fn application_popup_takes_keyboard_focus_and_restores_its_parent_surface() {
    let (owner_state, owner_body) = OrderedProbe::create(WidgetOption::NONE);
    let owner_body_id = owner_body.id();
    let (first_popup_state, first_popup) = OrderedProbe::create(WidgetOption::NONE);
    let (second_popup_state, second_popup) = OrderedProbe::create(WidgetOption::NONE);
    let (_, popup_body) = Linear::create(LinearParameters::vertical([first_popup, second_popup]));
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(10, 10, 120, 90), owner_body));
    let popup = ctx.ui().create_popup(&owner, "popup", popup_body).unwrap();

    // Establish a remembered owner focus before showing the popup. Showing commits geometry and
    // selects the popup's first eligible target without an extra application-authored Tab press.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.text("owner");
    ctx.update_and_render_ui();
    let owner_rect = ctx.debug_root_node_rect(owner.id(), owner_body_id).unwrap();
    ctx.mousedown(owner_rect.x + 1, owner_rect.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    ctx.ui().show_popup_at(&popup, rect(40, 40, 100, 80)).unwrap();
    ctx.update_and_render_ui();
    ctx.text("popup first");
    ctx.update_and_render_ui();
    assert_eq!(owner_state.try_read(|state| state.events.clone()), Some(vec!["text", "down"]));
    assert_eq!(first_popup_state.try_read(|state| state.events.clone()), Some(vec!["text"]));

    // Tab remains inside the active popup surface and advances only its own focused node ID.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::released(Key::Tab, Modifiers::NONE));
    ctx.text("popup second");
    ctx.update_and_render_ui();
    assert_eq!(second_popup_state.try_read(|state| state.events.clone()), Some(vec!["text"]));

    // The owner's unreleased pointer capture remains responsible only for its eventual pointer
    // tail; it cannot steal keyboard activation from the popup. The initial Escape press dismisses
    // the popup and restores the parent's remembered focus. Its later raw repeat and release are
    // consequently delivered to that restored parent before the following text transition.
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE).repeated());
    ctx.key(KeyEvent::released(Key::Escape, Modifiers::NONE));
    ctx.text("owner again");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert_eq!(
        owner_state.try_read(|state| state.events.clone()),
        Some(vec!["text", "down", "key-down", "key-up", "text"])
    );
    assert_eq!(first_popup_state.try_read(|state| state.events.clone()), Some(vec!["text"]));
    assert_eq!(second_popup_state.try_read(|state| state.events.clone()), Some(vec!["text"]));
}

#[test]
/// Proves that popup dismissal policy does not reserve ordinary Escape transitions globally.
fn escape_transitions_reach_focused_widget_without_application_popup() {
    // Use the generic ordered probe because custom widgets receive raw press, repeat, and release
    // transitions even when built-in controls choose to act only on selected key-down events.
    let (state, body) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    ctx.ui().create_window(Window::new("escape routing", rect(10, 10, 120, 90), body));

    // Establish persistent widget focus through the same public Tab path used by applications.
    // Tab itself is manager-owned, so it does not contribute an event to the probe's log.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::released(Key::Tab, Modifiers::NONE));

    // With no active application popup, all three Escape transitions belong to the focused widget.
    // This distinguishes scoped popup command state from a global policy that swallows every
    // Escape repeat and release after inspecting only the key identity.
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE).repeated());
    ctx.key(KeyEvent::released(Key::Escape, Modifiers::NONE));
    ctx.update_and_render_ui();

    assert_eq!(
        state.try_read(|state| state.events.clone()),
        Some(vec!["key-down", "key-down", "key-up"]),
        "ordinary Escape press, repeat, and release must remain raw focused-widget input"
    );
}

#[test]
/// Proves that entering a menu replaces an application popup and retains one surface focus route.
fn menu_keyboard_scope_replaces_an_application_popup_surface() {
    let (owner_state, owner_body) = OrderedProbe::create(WidgetOption::NONE);
    let (_, menu_item) = MenuItem::create(MenuItemParameters::new("Open"));
    let menu_bar = MenuBar::new([Menu::new("File").item(menu_item)]);
    let mut ctx = context();
    let owner = ctx
        .ui()
        .create_window(Window::new("surface switching", rect(10, 10, 160, 100), owner_body).menu_bar(menu_bar));
    let popup = ctx.ui().create_popup(&owner, "choices", empty_content()).unwrap();

    // Preserve application focus, then make the application popup the exact active surface.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.text("owner");
    ctx.update_and_render_ui();
    ctx.ui().show_popup_at(&popup, rect(40, 40, 80, 50)).unwrap();
    ctx.update_and_render_ui();

    // F10 transfers selection to the root-owned menu container and closes the incompatible popup
    // branch. Text is consumed by that menu focus route rather than reaching the owner widget.
    ctx.key(KeyEvent::pressed(Key::Function(10), Modifiers::NONE));
    ctx.text("blocked");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(owner_state.try_read(|state| state.events.clone()), Some(vec!["text"]));

    // ArrowDown moves the same active route into the concrete menu-popup surface. Escape restores
    // the bar, and a second F10 exits to the owner's independently remembered widget path.
    ctx.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["surface switching File Menu"]);
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Function(10), Modifiers::NONE));
    ctx.text("owner again");
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(owner_state.try_read(|state| state.events.clone()), Some(vec!["text", "text"]));
}

#[test]
fn ctrl_f6_cycles_visible_windows_in_both_directions_and_wraps() {
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(10, 10, 80, 60), empty_content()));
    let second = ctx.ui().create_window(Window::new("second", rect(110, 10, 80, 60), empty_content()));
    let third = ctx.ui().create_window(Window::new("third", rect(210, 10, 80, 60), empty_content()));
    ctx.update_and_render_ui();

    // With no explicit activation, the newest visible root supplies keyboard routing. Forward
    // traversal wraps from that third root to the oldest and raises the selected root in its layer.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(first.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["second", "third", "first"]);

    // Raising rotates the same activation chronology, so another forward command reaches the next
    // root. Reverse traversal then returns to the preceding root using Ctrl+Shift+F6.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::CTRL));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(second.id()));
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL | Modifiers::SHIFT));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(first.id()));

    // Hidden roots retain their window state but leave the cycle immediately. Starting from the
    // front fallback after hiding the active root therefore wraps directly to the third root.
    ctx.ui().set_window_visible(&first, false).unwrap();
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(third.id()));
}

#[test]
fn ctrl_f6_preserves_each_window_focus_and_consumes_the_complete_chord() {
    let (first_state, first_body) = OrderedProbe::create(WidgetOption::NONE);
    let (second_state, second_body) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(10, 10, 100, 80), first_body));
    let second = ctx.ui().create_window(Window::new("second", rect(150, 10, 100, 80), second_body));

    // The newest window initially owns keyboard routing. Focus its probe and establish observable
    // text delivery before changing the active root entirely through the keyboard.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.text("second");
    ctx.update_and_render_ui();
    assert_eq!(second_state.try_read(|state| state.events.clone()), Some(vec!["text"]));

    // One physical chord selects the first root. Repeat and release remain manager-owned even after
    // Control is absent from the release snapshot, so neither probe sees raw F6 transitions.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL).repeated());
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.text("first");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(first.id()));
    assert_eq!(first_state.try_read(|state| state.events.clone()), Some(vec!["text"]));

    // Returning to the second root immediately restores its retained probe focus without another
    // Tab command. Text proves the inactive runtime remembered rather than discarded that target.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.text("second again");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(second.id()));
    assert_eq!(first_state.try_read(|state| state.events.clone()), Some(vec!["text"]));
    assert_eq!(second_state.try_read(|state| state.events.clone()), Some(vec!["text", "text"]));
}

#[test]
fn ctrl_f6_dismisses_application_popups_but_cannot_escape_a_modal_dialog() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(10, 10, 100, 80), empty_content()));
    let other = ctx.ui().create_window(Window::new("other", rect(150, 10, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&owner, "popup", empty_content()).unwrap();
    let dialog = ctx
        .ui()
        .create_dialog(&owner, Window::new("dialog", rect(60, 60, 120, 90), empty_content()))
        .unwrap();
    ctx.ui().show_popup_at(&popup, rect(20, 20, 60, 40)).unwrap();
    ctx.update_and_render_ui();

    // The popup identifies its ordinary owner as the source scope. Cycling closes that transient
    // branch first and activates the adjacent ordinary window.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert_eq!(ctx.debug_active_root(), Some(other.id()));

    // A visible dialog becomes the concrete active keyboard surface. The same recognized chord is
    // consumed without escaping or dismissing that modal root.
    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));
    assert_eq!(ctx.debug_active_root(), Some(dialog.id()));
}

#[test]
fn tab_focused_builtins_share_windows_activation_and_arrow_adjustment() {
    let (button, button_node) = Button::create(ButtonParameters::new("submit"));
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("enabled", false));
    let (slider, slider_node) = Slider::create(
        SliderParameters::with_opt(5.0, 0.0, 10.0, 1.0, DecimalPrecision::ZERO, WidgetOption::FRAME).expect("finite ascending slider parameters must validate"),
    );
    let (_, content) = Linear::create(LinearParameters::vertical([button_node, checkbox_node, slider_node]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let mut submissions = 0;
    let mut dispatcher = event_counter(button.submitted());

    // The first Tab selects the button and Enter emits its ordinary typed submission event.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Enter, Modifiers::NONE));
    ctx.update_ui(Dimensioni::new(320, 240));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);

    // Checkbox Enter is intentionally inert; Space performs the single conventional toggle.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Enter, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Space, Modifiers::NONE));
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(checkbox.checked(), Some(true));

    // Horizontal sliders advance by one configured step on Right Arrow.
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowRight, Modifiers::NONE));
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(slider.value(), Some(6.0));
}

#[test]
fn active_window_and_only_its_remembered_widget_use_style_focus_accents() {
    // Preserve one atlas identity across backend construction and the customized Style so the
    // focus-color assertions cannot accidentally rely on globally meaningful resource slots.
    let atlas = test_atlas();
    let mut style = Style {
        focus_color: color(7, 17, 29, 255),
        window_focus_color: color(31, 47, 61, 255),
        ..test_style(&atlas)
    };
    let active_frame = NinePatch::framed(
        style.appearance(AppearanceRole::WindowFrame, VisualState::Normal).insets,
        style.window_focus_color,
        Some(style.colors[ControlColor::WindowBG as usize]),
    );
    style.appearances.set(AppearanceRole::WindowFrameActive, StatefulAppearance::all(active_frame));
    style.appearances.set(
        AppearanceRole::WindowTitleActive,
        StatefulAppearance::all(NinePatch::solid(style.window_focus_color)),
    );
    let (backend, log) = recording_backend(atlas);
    let mut ctx = Context::<_>::new(backend);
    ctx.set_style(style.clone());

    let (_, first_node) = OrderedProbe::create(WidgetOption::NONE);
    let first_id = first_node.id();
    let (_, second_node) = OrderedProbe::create(WidgetOption::NONE);
    let second_id = second_node.id();
    let first = ctx.ui().create_window(Window::new("first", rect(10, 10, 140, 100), first_node));
    let second = ctx.ui().create_window(Window::new("second", rect(220, 10, 140, 100), second_node));
    let dimensions = Dimensioni::new(400, 240);
    ctx.update_ui(dimensions);
    let first_rect = ctx.debug_root_node_rect(first.id(), first_id).unwrap();
    let second_rect = ctx.debug_root_node_rect(second.id(), second_id).unwrap();

    // Focus both independent runtimes, then return activation to the first. The second runtime must
    // remember its target without painting a second fill, caret, or outline.
    for target in [first_rect, second_rect, first_rect] {
        let x = target.x + target.width / 2;
        let y = target.y + target.height / 2;
        ctx.mousedown(x, y, MouseButton::LEFT);
        ctx.mouseup(x, y, MouseButton::LEFT);
        ctx.update_ui(dimensions);
    }

    log.clear();
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let events = log.snapshot();
    let widget_focus = atlas_quads_with_color(&events, style.focus_color);
    assert_eq!(
        widget_focus.len(),
        8,
        "one inside-aligned widget outline has eight visible nine-patch border cells"
    );
    assert!(
        widget_focus.iter().all(|event| {
            let RenderEvent::AtlasQuad(vertices) = event else { unreachable!() };
            vertices.iter().all(|vertex| vertex.position[0] <= 150.0)
        }),
        "the inactive second runtime must not expose its remembered focus"
    );

    let window_focus = atlas_quads_with_color(&events, style.window_focus_color);
    assert_eq!(
        window_focus.len(),
        9,
        "the active framed/title window records eight border cells and one title fill"
    );
    assert!(
        window_focus.iter().all(|event| {
            let RenderEvent::AtlasQuad(vertices) = event else { unreachable!() };
            vertices.iter().all(|vertex| vertex.position[0] <= 150.0)
        }),
        "only the reactivated first window may use the window focus accent"
    );
}

#[test]
fn tab_focused_disclosure_uses_focus_fill_in_addition_to_the_shared_outline() {
    // Resolve the customized palette from the exact atlas moved into the recording backend.
    let atlas = test_atlas();
    let mut style = Style {
        focus_color: color(67, 83, 101, 255),
        window_focus_color: color(109, 127, 149, 255),
        ..test_style(&atlas)
    };
    let row = style.appearances.get(AppearanceRole::DisclosureHeader);
    let mut focused_row = row;
    focused_row.set(VisualState::Focused, NinePatch::solid(style.focus_color));
    style.appearances.set(AppearanceRole::DisclosureHeader, focused_row);
    let (backend, log) = recording_backend(atlas);
    let mut ctx = Context::<_>::new(backend);
    ctx.set_style(style.clone());

    let (_, disclosure) = Disclosure::create(DisclosureParameters::tree("focused tree row", false, std::iter::empty::<LinearItem>()));
    let root = ctx.ui().create_window(Window::new("disclosure", rect(20, 20, 180, 100), disclosure));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    ctx.key(KeyEvent::pressed(Key::Tab, Modifiers::NONE));
    ctx.update_ui(dimensions);

    log.clear();
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let events = log.snapshot();
    let focus_quads = atlas_quads_with_color(&events, style.focus_color);
    assert_eq!(
        focus_quads.len(),
        9,
        "the disclosure row fill plus the eight-cell retained outline must use one focus color"
    );
}

#[test]
fn empty_public_update_consumes_programmatic_text_area_caret_reveal() {
    // Use enough unwrapped lines to guarantee vertical overflow inside the fixed window body.
    let document = (0..20).map(|index| format!("line {index}")).collect::<Vec<_>>().join("\n");
    let (text_area, content) = TextArea::create(TextAreaParameters::new(document));
    let mut ctx = context();
    let style = Style {
        padding: 0,
        scrollbar_size: 10,
        ..ctx.style().clone()
    };
    ctx.set_style(style);
    let root = ctx.ui().create_window(Window::new("text area", rect(10, 10, 100, 60), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);

    // Both cursor mutations are programmatic and queue no UiInputEvent. The synchronization update
    // must still let TextArea hand its caret rectangle to the parent-owned ScrollArea.
    text_area.set_cursor(0).unwrap();
    ctx.update_ui(dimensions);
    assert_eq!(text_area.scroll().map(|scroll| (scroll.x, scroll.y)), Some((0, 0)));

    text_area.move_cursor_to_end().unwrap();
    ctx.update_ui(dimensions);
    let scroll = text_area.scroll().expect("the composed TextArea must retain its ScrollArea");
    assert_eq!(scroll.x, 0);
    assert!(scroll.y > 0, "an empty-input Context update must reveal the final caret");
}

#[test]
fn global_style_replacement_invalidates_measurements_in_hidden_surfaces() {
    let measures = Rc::new(Cell::new(0));
    let probe = Node::widget(CompleteStyleMeasureProbe {
        measures: measures.clone(),
        opt: WidgetOption::NO_INTERACT,
    });
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("style probe", rect(10, 10, 120, 90), probe));
    let dimensions = Dimensioni::new(320, 240);

    // Warm the retained entry and prove an unchanged synchronization reuses it.
    ctx.update_ui(dimensions);
    let warmed = measures.get();
    ctx.update_ui(dimensions);
    assert_eq!(measures.get(), warmed, "unchanged style and constraints must retain the cached preference");

    // Replace only a value that the former partial style key omitted while the tree is hidden.
    // Revealing it later must not revive the entry measured under the previous complete Style.
    ctx.ui().set_window_visible(&window, false).unwrap();
    let mut replacement = ctx.style().clone();
    replacement.menu_background.r = replacement.menu_background.r.wrapping_add(1);
    ctx.set_style(replacement);
    ctx.update_ui(dimensions);
    assert_eq!(measures.get(), warmed, "hidden widget trees must not be traversed during the style commit");

    ctx.ui().set_window_visible(&window, true).unwrap();
    ctx.update_ui(dimensions);
    assert!(measures.get() > warmed, "revealed content must measure against the replacement Style");
}

#[test]
fn update_drains_each_input_into_one_full_update_and_one_followup_layout() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
    let dimensions = Dimensioni::new(320, 240);

    ctx.mousemove(20, 20);
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("x");
    ctx.update_ui(dimensions);

    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "one synchronization layout plus one per input event");
    assert_eq!(metrics.updates, 3, "three events update the application tree");
    assert_eq!(metrics.paints, 0, "update_ui must not paint");

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 4, "render must not lay out");
    assert_eq!(metrics.updates, 3, "render must not update widgets");
    assert_eq!(metrics.paints, 1, "render paints the application tree exactly once");
}

#[test]
fn render_preflight_requires_a_matching_commit_and_never_acquires_backend_on_error() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::<_>::new(backend);
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
    let dimensions = Dimensioni::new(320, 240);

    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.update_ui(dimensions);
    ctx.text("pending");
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.update_ui(dimensions);
    let other = Dimensioni::new(640, 480);
    assert_eq!(ctx.frame(frame_info(other)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());

    ctx.ui().set_window_rect(&root, rect(20, 20, 120, 90)).unwrap();
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty());
}

#[test]
fn render_preflight_rejects_nested_typed_mutation_until_update_clears_it() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::<_>::new(backend);
    let (text, text_node) = TextBlock::create(TextBlockParameters::new("before"));
    let (column, content) = Linear::create(LinearParameters::vertical([text_node]));
    ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), content));
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    text.try_update(|text| text.set_text("after")).unwrap();

    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty(), "a rejected dirty commit must not acquire the backend");

    ctx.update_ui(dimensions);
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    assert!(!log.snapshot().is_empty(), "the context walk must clear the measurement marker");

    log.clear();
    column.try_update(|_| {}).unwrap();
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    assert!(log.snapshot().is_empty(), "container storage must use the same render guard");
    ctx.update_ui(dimensions);
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
}

#[test]
fn hidden_typed_mutation_does_not_block_the_visible_commit() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::<_>::new(backend);
    ctx.ui().create_window(Window::new("visible", rect(10, 10, 120, 90), empty_content()));
    let (hidden_text, hidden_node) = TextBlock::create(TextBlockParameters::new("before"));
    let hidden = ctx.ui().create_window(Window::new("hidden", rect(20, 20, 120, 90), hidden_node));
    ctx.ui().set_window_visible(&hidden, false).unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    hidden_text.try_update(|text| text.set_text("after")).unwrap();

    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    assert!(!log.snapshot().is_empty(), "hidden widget state cannot stale the visible frame");

    ctx.ui().set_window_visible(&hidden, true).unwrap();
    assert_eq!(ctx.frame(frame_info(dimensions)).render_ui(), Err(RenderError::UiUpdateRequired));
    ctx.update_ui(dimensions);
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
}

#[test]
fn invalid_update_dimensions_panic_before_dequeue_and_preserve_pending_input() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));
    ctx.text("still pending");

    // Catch only to keep exercising the same context after the rejected commit; the companion
    // `should_panic` test below verifies the diagnostic without inspecting an erased payload.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ctx.update_ui(Dimensioni::new(0, 240));
    }));
    assert!(result.is_err(), "invalid dimensions must panic");

    ctx.update_ui(Dimensioni::new(320, 240));
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 2);
    assert_eq!(metrics.updates, 1, "the event queued before the panic must still be drained");
}

#[test]
#[should_panic(expected = "update_ui dimensions must be positive")]
fn invalid_update_dimensions_reports_the_expected_diagnostic() {
    // Drive the public commit entry point directly so `should_panic` checks the emitted text while
    // keeping the test independent of panic-payload representation.
    context().update_ui(Dimensioni::new(0, 240));
}

#[test]
fn every_event_layout_commit_updates_hit_geometry_for_the_next_queued_event() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let (title, close, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let close = close.unwrap();
    let shift = crate::vec2(18, 11);

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.mousemove(title.x + 2 + shift.x, title.y + 2 + shift.y);
    ctx.mouseup(title.x + 2 + shift.x, title.y + 2 + shift.y, MouseButton::LEFT);
    ctx.mousedown(close.x + close.width / 2 + shift.x, close.y + close.height / 2 + shift.y, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
}

#[test]
fn disclosure_update_commits_child_geometry_before_the_next_queued_press() {
    let (button, child) = button_content("child");
    let mut dispatcher = event_counter(button);
    let mut submissions = 0;
    let (disclosure, node) = Disclosure::create(DisclosureParameters::header("section", false, [child]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.mousedown(10, 28, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    assert_eq!(disclosure.try_read(Disclosure::is_expanded), Some(true));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
}

#[test]
fn collapsed_disclosure_skips_descendant_phases_and_drops_targets_only_on_removal() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));

    let (probe_state, probe_node) = OrderedProbe::create(WidgetOption::NONE);
    let probe_id = probe_node.id();

    let custom = ctx
        .register_custom_renderer(|frame, _args| frame.record_marker("disclosure custom child"))
        .unwrap();
    let custom_runtime = Custom::create(CustomParameters::new("custom"));
    let (custom_state, custom_node) = Node::typed_custom_render(custom_runtime, custom);
    let (disclosure, content) = Disclosure::create(DisclosureParameters::header(
        "section",
        true,
        [LinearItem::content(probe_node), LinearItem::fixed(custom_node, 10).with_fixed_cross(20)],
    ));
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 160, 140), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.update_and_render_ui();
    let probe_rect = ctx.debug_root_node_rect(root.id(), probe_id).unwrap();

    ctx.mousedown(probe_rect.x + 2, probe_rect.y + 2, MouseButton::LEFT);
    ctx.mouseup(probe_rect.x + 2, probe_rect.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    let visible_counts = probe_state
        .try_read(|state| (state.measures.get(), state.updates, state.paints, state.events.clone()))
        .unwrap();
    assert_eq!(visible_counts.3, ["down", "up"]);

    disclosure.try_update(Disclosure::collapse).unwrap();
    log.clear();
    ctx.mousemove(probe_rect.x + 2, probe_rect.y + 2);
    ctx.update_and_render_ui();

    assert_eq!(
        probe_state.try_read(|state| (state.measures.get(), state.updates, state.paints)),
        Some((visible_counts.0, visible_counts.1, visible_counts.2)),
        "collapsed descendants must skip measure, update, and paint"
    );
    assert!(
        !log.snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::Marker(name) if name == "disclosure custom child")),
        "collapsed descendants must skip custom rendering"
    );
    assert!(probe_state.is_alive() && custom_state.is_alive(), "collapse must retain descendant ownership");

    disclosure.try_update(Disclosure::expand).unwrap();
    log.clear();
    ctx.text("focus must not return");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    assert!(
        log.snapshot()
            .iter()
            .any(|event| matches!(event, RenderEvent::Marker(name) if name == "disclosure custom child"))
    );

    disclosure.try_update(Disclosure::clear).unwrap();
    assert!(!probe_state.is_alive() && !custom_state.is_alive(), "removal must drop descendant runtimes");
}

#[test]
fn nested_scroll_bubbles_at_the_inner_boundary_and_moves_only_the_outer_area() {
    let inner_content = desired_size_node(50, 180);
    let (inner, inner_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, inner_content));
    let inner_id = inner_node.id();
    let outer_tail = Node::widget(Custom::create(CustomParameters::new("outer tail")));
    let (_, outer_content) = Linear::create(LinearParameters::vertical([
        LinearItem::fixed(inner_node, 60).with_fixed_cross(60),
        LinearItem::fixed(outer_tail, 120).with_fixed_cross(60),
    ]));
    let (outer, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, outer_content));

    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    inner.try_update(|state| state.set_offset(crate::vec2(0, i32::MAX))).unwrap();
    ctx.update_ui(dimensions);
    let inner_offset = inner.try_read(|state| state.offset()).unwrap();
    assert!(inner_offset.y > 0);
    let inner_rect = ctx.debug_root_node_rect(root.id(), inner_id).unwrap();

    ctx.mousemove(inner_rect.x + 5, inner_rect.y + 5);
    ctx.scroll(0, 12);
    ctx.update_ui(dimensions);

    assert_eq!(
        inner.try_read(|state| (state.offset().x, state.offset().y)),
        Some((inner_offset.x, inner_offset.y))
    );
    assert_eq!(outer.try_read(|state| state.offset().y), Some(12));
}

#[test]
fn intrinsic_mutation_is_laid_out_before_the_next_event_and_painted_from_that_commit() {
    let (growing_state, growing, growing_paints) = CommitProbe::new(10, Some(30));
    let growing_id = growing.id();
    let (target_state, target, _) = CommitProbe::new(10, None);
    let target_id = target.id();
    let (_, content) = Linear::create(LinearParameters::vertical([growing, target]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    let growing_before = ctx.debug_root_node_rect(root.id(), growing_id).unwrap();
    let target_before = ctx.debug_root_node_rect(root.id(), target_id).unwrap();
    assert_eq!(growing_before.height, 10);

    // The move grows the first sibling. Its post-event layout moves the second sibling before the
    // queued press is routed, so this point is inside only the new target rectangle.
    ctx.mousemove(growing_before.x + 1, growing_before.y + 1);
    ctx.mousedown(target_before.x + 1, target_before.y + 21, MouseButton::LEFT);
    ctx.update_ui(dimensions);

    let target_after = ctx.debug_root_node_rect(root.id(), target_id).unwrap();
    assert_eq!(growing_state.try_read(|state| state.intrinsic_height), Some(30));
    assert_eq!(target_after.y, target_before.y + 20);
    assert_eq!(target_state.try_read(|state| state.presses), Some(1));

    let committed_metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();
    let rendered_metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(rendered_metrics.tree_layouts, committed_metrics.tree_layouts);
    assert_eq!(rendered_metrics.updates, committed_metrics.updates);
    assert_eq!(growing_paints.borrow().last().map(|rect| rect.height), Some(30));
}

#[test]
fn sibling_mutation_observes_parent_first_forward_traversal_without_reruns() {
    let (later_state, later) = SiblingMutationProbe::new(0, None);
    let (earlier_state, earlier) = SiblingMutationProbe::new(0, Some((later_state.clone(), 11)));
    let (already_updated_state, already_updated) = SiblingMutationProbe::new(0, None);
    let (_late_mutator_state, late_mutator) = SiblingMutationProbe::new(0, Some((already_updated_state.clone(), 22)));
    let (_, content) = Linear::create(LinearParameters::vertical([earlier, later, already_updated, late_mutator]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(5, 5);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(earlier_state.try_read(|state| state.observed_during_update.clone()), Some(vec![0]));
    assert_eq!(later_state.try_read(|state| state.observed_during_update.clone()), Some(vec![11]));
    assert_eq!(already_updated_state.try_read(|state| state.value), Some(22));
    assert_eq!(
        already_updated_state.try_read(|state| state.observed_during_update.clone()),
        Some(vec![0]),
        "the earlier sibling changes but is not rerun after the later sibling mutates it"
    );
}

#[test]
fn topology_mutation_is_blocked_for_the_active_container_and_visible_in_a_later_subtree() {
    let inserted_updates = Rc::new(Cell::new(0));
    let candidate = Node::widget(CountedProbe::new(inserted_updates.clone()));
    let (other_container, other_node) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
    let same_container = Rc::new(RefCell::new(None));
    let mutator = TopologyMutator {
        same_container_blocked: false,
        other_container_changed: false,
        same_container: same_container.clone(),
        other_container: other_container.clone(),
        candidate: Some(candidate),
        opt: WidgetOption::NONE,
    };
    let (mutator_state, mutator) = Node::typed_widget(mutator);
    let (outer_container, content) = Linear::create(LinearParameters::vertical([mutator, other_node]));
    *same_container.borrow_mut() = Some(outer_container.clone());
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousemove(5, 5);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(
        mutator_state.try_read(|state| (state.same_container_blocked, state.other_container_changed)),
        Some((true, true))
    );
    assert_eq!(outer_container.try_read(Linear::len), Some(Some(2)));
    assert_eq!(other_container.try_read(Linear::len), Some(Some(1)));
    assert_eq!(inserted_updates.get(), 1, "the newly inserted later descendant participates in the same update");
}

#[test]
#[allow(clippy::result_large_err)] // The assertion exercises the ownership-preserving mutation result.
fn programmatic_topology_mutation_commits_during_an_empty_queue_synchronization() {
    let (_, first, _) = CommitProbe::new(10, None);
    let (column, content) = Linear::create(LinearParameters::vertical([first]));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);

    let (_, appended, _) = CommitProbe::new(18, None);
    let appended_id = appended.id();
    assert!(
        column.try_update_with(appended, |state, node| state.push(node)).is_ok(),
        "the programmatic topology mutation must commit before traversal"
    );
    ctx.update_ui(dimensions);

    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    let appended_rect = ctx.debug_root_node_rect(root.id(), appended_id).unwrap();
    assert_eq!(metrics.tree_layouts, 2, "eventless synchronization lays out before and after widget work");
    assert_eq!(metrics.updates, 3, "the container and both children receive the one eventless traversal");
    assert_eq!(appended_rect.height, 18);
}

#[test]
fn traversal_recovers_after_widget_access_closure_borrows_are_released() {
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);

    // Catch only so this test can prove the context remains usable after the diagnostic. Dedicated
    // `should_panic` tests below verify the diagnostic text without examining an erased payload.
    let update_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.update_ui(dimensions));
    }));
    assert!(update_result.is_err(), "layout must diagnose the active TextBlock borrow");

    // Once the access closure has unwound and released its borrow, synchronization can commit.
    ctx.update_ui(dimensions);
    let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
    }));
    assert!(render_result.is_err(), "render preflight must diagnose the active TextBlock borrow");

    // A typed update marks measurement dirty before invoking application code, so unwinding from
    // the nested render attempt conservatively requires a fresh synchronization commit.
    ctx.update_ui(dimensions);
    ctx.frame(frame_info(dimensions)).render_ui().unwrap();

    // A shared access closure is likewise incompatible when the routed update needs to mutate the
    // same cell, even though the synchronization layout's shared reads are allowed by RefCell.
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let checkbox_id = checkbox_node.id();
    let mut checkbox_ctx = context();
    let checkbox_root = checkbox_ctx.ui().create_window(Window::new("checkbox", rect(0, 0, 140, 100), checkbox_node));
    checkbox_ctx
        .ui()
        .set_window_options(&checkbox_root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    checkbox_ctx.update_ui(dimensions);
    let checkbox_rect = checkbox_ctx.debug_root_node_rect(checkbox_root.id(), checkbox_id).unwrap();
    checkbox_ctx.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    let read_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        checkbox.try_read(|_| checkbox_ctx.update_ui(dimensions));
    }));
    assert!(read_result.is_err(), "Checkbox::update must diagnose the active shared Checkbox borrow");
}

#[test]
#[should_panic(expected = "typed access closure must finish before runtime traversal")]
fn update_traversal_reports_an_active_typed_access_closure() {
    // Hold the concrete widget's mutable access guard across update traversal to provoke the
    // invariant diagnostic at the same boundary used by applications.
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    text.try_update(|_| ctx.update_ui(Dimensioni::new(320, 240)));
}

#[test]
#[should_panic(expected = "retained widget invariant violated")]
fn render_preflight_reports_an_active_typed_access_closure() {
    // Commit once so render validation reaches the visible-tree measurement scan while the concrete
    // widget's mutable access guard remains held.
    let (text, widget) = crate::TextBlock::create(crate::TextBlockParameters::new("borrowed"));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("window", rect(0, 0, 140, 100), widget));
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    text.try_update(|_| ctx.frame(frame_info(dimensions)).render_ui().unwrap());
}

#[test]
#[should_panic(expected = "typed access closure must finish before runtime traversal")]
fn routed_update_reports_an_active_shared_access_closure() {
    // Queue a routed checkbox event, then retain a concrete shared access guard while update needs
    // mutable access to that same widget state.
    let (checkbox, checkbox_node) = Checkbox::create(CheckboxParameters::new("checkbox", false));
    let checkbox_id = checkbox_node.id();
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("checkbox", rect(0, 0, 140, 100), checkbox_node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    ctx.update_ui(dimensions);
    let checkbox_rect = ctx.debug_root_node_rect(root.id(), checkbox_id).unwrap();
    ctx.mousedown(checkbox_rect.x + 1, checkbox_rect.y + 1, MouseButton::LEFT);
    checkbox.try_read(|_| ctx.update_ui(dimensions));
}

fn button_content(label: &str) -> (crate::WidgetEventPortHandle<ButtonSubmitted>, Node) {
    let (widget, node) = Button::create(ButtonParameters::new(label));
    (widget.submitted(), node)
}

#[test]
fn widget_handle_events_invoke_state_methods_without_polling() {
    #[derive(Default)]
    struct Model {
        submissions: Vec<&'static str>,
    }

    impl Model {
        fn record(&mut self, label: &&'static str, _: &ButtonSubmitted) {
            self.submissions.push(*label);
        }
    }

    let (first_widget, first) = Button::create(ButtonParameters::new("first"));
    let first_submitted = first_widget.submitted();
    let first_id = first.id();
    let (second_widget, second) = Button::create(ButtonParameters::new("second"));
    let second_submitted = second_widget.submitted();
    let second_id = second.id();
    let (_, content) = Linear::create(LinearParameters::horizontal([LinearItem::fixed(first, 60), LinearItem::fixed(second, 60)]));
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240));
    let root = ctx.ui().create_window(Window::new("signal", rect(0, 0, 140, 100), content));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);
    let first_rect = ctx.debug_root_node_rect(root.id(), first_id).unwrap();
    let second_rect = ctx.debug_root_node_rect(root.id(), second_id).unwrap();

    ctx.subscribe_with(first_submitted, "first", Model::record).unwrap();
    ctx.subscribe_with(second_submitted, "second", Model::record).unwrap();
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mouseup(second_rect.x + 1, second_rect.y + 1, MouseButton::LEFT);
    ctx.mousedown(first_rect.x + 1, first_rect.y + 1, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);

    assert_eq!(model.submissions, ["first", "second", "first"]);
}

#[test]
fn context_aware_handler_creates_and_mutates_every_root_kind_before_layout() {
    #[derive(Default)]
    struct Model {
        window: Option<WindowHandle>,
        dialog: Option<WindowHandle>,
        popup: Option<PopupHandle>,
    }

    impl Model {
        fn create_roots(&mut self, context: &mut Ui<'_>, _: &ButtonSubmitted) {
            // Construct every root kind through the same dispatch capability. The nodes move into
            // Context ownership exactly as they do through the ordinary Context façade.
            let window = context.create_window(Window::new("event window", rect(30, 40, 90, 70), empty_content()));
            let dialog = context
                .create_dialog(&window, Window::new("event dialog", rect(50, 60, 100, 80), empty_content()))
                .unwrap();
            let popup = context.create_popup(&dialog, "event popup", empty_content()).unwrap();

            // Exercise generic root mutation while the event boundary owns exclusive WindowManager
            // access. The layout following dispatch must observe every change.
            context.set_window_size(&window, Dimensioni::new(110, 75)).unwrap();
            context.set_window_visible(&dialog, true).unwrap();
            context.show_popup_at(&popup, rect(180, 30, 1, 1)).unwrap();

            self.window = Some(window);
            self.dialog = Some(dialog);
            self.popup = Some(popup);
        }
    }

    let (submitted, button) = button_content("create roots");
    let button_id = button.id();
    let dimensions = Dimensioni::new(320, 240);
    let mut context: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let source = context.ui().create_window(Window::new("source", rect(0, 0, 140, 100), button));
    context
        .ui()
        .set_window_options(&source, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    context.subscribe_context(submitted, Model::create_roots).unwrap();
    let mut model = Model::default();
    context.update_ui_state(dimensions, &mut model);
    let button_rect = context.debug_root_node_rect(source.id(), button_id).unwrap();

    context.mousedown(button_rect.x + 1, button_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);

    let window = model.window.as_ref().expect("event handler must retain the non-owning window handle");
    let dialog = model.dialog.as_ref().expect("event handler must retain the non-owning dialog handle");
    let popup = model.popup.as_ref().expect("event handler must retain the non-owning popup handle");
    assert_eq!(
        context.debug_root_rect(window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((30, 40, 110, 75))
    );
    assert_eq!(context.debug_root_visible(dialog.id()), Some(true));
    assert_eq!(context.debug_popup_visible(popup), Some(true));
    assert_eq!(context.debug_popup_rect(popup).map(|rect| (rect.x, rect.y)), Some((180, 30)));
    assert!(context.debug_root_node_count(window.id()).is_some());
    assert!(context.debug_root_node_count(dialog.id()).is_some());
    assert!(popup.events().is_alive());
}

#[test]
fn typed_events_keep_composed_combo_and_popup_state_synchronized() {
    struct Model {
        combo: TypedWidgetHandle<Combo>,
        popup: PopupHandle,
        submitted_anchor: Option<Recti>,
    }

    impl Model {
        fn combo_submitted(&mut self, context: &mut Ui<'_>, event: &ComboSubmitted) {
            // Compose the semantic Combo with an ordinary window-owned popup.
            if event.open {
                context.show_popup_at(&self.popup, event.anchor).unwrap();
            } else {
                context.hide_popup(&self.popup).unwrap();
            }
            self.submitted_anchor = Some(event.anchor);
        }

        fn popup_submitted(&mut self, event: &PopupEvent) {
            // Popup dismissal becomes typed application input after traversal, where the
            // composed widget can safely reconcile its retained semantic state.
            if matches!(event, PopupEvent::Dismissed) {
                self.combo.close_popup().expect("mounted combo must remain available");
            }
        }
    }

    let dimensions = Dimensioni::new(320, 240);
    let mut context: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let (combo, combo_node) = Combo::create(ComboParameters::new());
    let combo_id = combo_node.id();
    let (_, menu_item) = MenuItem::create(MenuItemParameters::new("Menu action"));
    let source = context
        .ui()
        .create_window(Window::new("combo source", rect(10, 10, 140, 90), combo_node).menu_bar(MenuBar::new([Menu::new("File").item(menu_item)])));
    context.ui().set_window_options(&source, WindowOption::FRAME).unwrap();
    let popup = context
        .ui()
        .create_popup(&source, "combo choices", Node::widget(DesiredSize(Dimensioni::new(100, 60))))
        .unwrap();
    let replacement = context
        .ui()
        .create_popup(&source, "replacement", Node::widget(DesiredSize(Dimensioni::new(80, 40))))
        .unwrap();
    let mut model = Model {
        combo: combo.clone(),
        popup: popup.clone(),
        submitted_anchor: None,
    };
    context.subscribe_context(combo.submitted(), Model::combo_submitted).unwrap();
    context.subscribe(popup.events(), Model::popup_submitted).unwrap();
    context.update_ui_state(dimensions, &mut model);
    let combo_rect = context.debug_root_node_rect(source.id(), combo_id).unwrap();

    context.mousedown(combo_rect.x + 1, combo_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);

    let anchor = model.submitted_anchor.expect("combo submission must carry its routed anchor");
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(context.debug_popup_rect(&popup).map(|rect| (rect.x, rect.y)), Some((anchor.x, anchor.y)));
    assert_eq!(combo.is_open(), Some(true));

    // Root chrome is outside the composed popup. Its press dismisses the popup and reconciles the
    // shared Combo state before the subsequent drag can move the source window.
    let title = context.debug_root_chrome(source.id()).unwrap().0.unwrap();
    let title_x = title.x + title.width / 2;
    let title_y = title.y + title.height / 2;
    context.mousedown(title_x, title_y, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(combo.is_open(), Some(false));
    assert_eq!(context.debug_root_moving(source.id()), Some(true));

    context.mousemove(title_x + 15, title_y + 10);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_root_rect(source.id()).map(|rect| (rect.x, rect.y)), Some((25, 20)));
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(combo.is_open(), Some(false));

    // Reopen the composed popup, then replace its owner's active popup. The displaced popup's
    // dismissal event must close Combo's semantic state in the same update transaction.
    context.mouseup(title_x + 15, title_y + 10, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    let moved_combo_rect = context.debug_root_node_rect(source.id(), combo_id).unwrap();
    context.mousedown(moved_combo_rect.x + 1, moved_combo_rect.y + 1, MouseButton::LEFT);
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(combo.is_open(), Some(true));

    context.ui().show_popup(&replacement).unwrap();
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(context.debug_popup_visible(&replacement), Some(true));
    assert_eq!(combo.is_open(), Some(false));

    // The replacement popup is now the concrete keyboard surface, so F4 cannot leak through to the
    // suspended Combo header. Escape dismisses it and restores the header's remembered focus; only
    // then can F4 open the composed popup and a later Escape close it.
    context.key(KeyEvent::pressed(Key::Function(4), Modifiers::NONE));
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(context.debug_popup_visible(&replacement), Some(true));
    assert_eq!(combo.is_open(), Some(false));

    context.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    context.key(KeyEvent::released(Key::Escape, Modifiers::NONE));
    context.key(KeyEvent::pressed(Key::Function(4), Modifiers::NONE));
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(context.debug_popup_visible(&replacement), Some(false));
    assert_eq!(combo.is_open(), Some(true));

    context.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(false));
    assert_eq!(combo.is_open(), Some(false));

    // Alt+Down is a combo chord, not an Alt tap. The intervening arrow cancels pending menu-bar
    // activation, opens the composed popup, and leaves the intrinsic File menu closed.
    context.key(KeyEvent::pressed(Key::Alt, Modifiers::ALT));
    context.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::ALT));
    context.key(KeyEvent::released(Key::Alt, Modifiers::NONE));
    context.update_ui_state(dimensions, &mut model);
    assert_eq!(context.debug_popup_visible(&popup), Some(true));
    assert_eq!(combo.is_open(), Some(true));
    assert_eq!(context.debug_active_popup_names(), ["combo choices"]);
}

#[test]
fn textbox_handle_event_dispatches_a_complete_snapshot_to_state() {
    #[derive(Default)]
    struct Model {
        changes: Vec<(String, usize)>,
    }

    impl Model {
        fn changed(&mut self, event: &TextboxChanged) {
            self.changes.push((event.text.clone(), event.cursor));
        }
    }

    let (widget, node) = Textbox::create(TextboxParameters::new(""));
    let changed = widget.changed();
    let node_id = node.id();
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(320, 240));
    let root = ctx.ui().create_window(Window::new("textbox signal", rect(0, 0, 140, 100), node));
    ctx.ui()
        .set_window_options(&root, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dimensions = Dimensioni::new(320, 240);
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);
    let textbox_rect = ctx.debug_root_node_rect(root.id(), node_id).unwrap();

    ctx.subscribe(changed, Model::changed).unwrap();
    ctx.mousedown(textbox_rect.x + 1, textbox_rect.y + 1, MouseButton::LEFT);
    ctx.text("é");
    ctx.update_ui_state(dimensions, &mut model);

    assert_eq!(model.changes, [(String::from("é"), "é".len())]);
}

#[test]
fn creation_returns_persistent_root_handle() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(20, 30, 120, 90), empty_content()));

    assert!(root.events().is_alive());
    assert_eq!(ctx.debug_root_name(root.id()), Some("window".to_owned()));
    assert_eq!(
        ctx.debug_root_rect(root.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((20, 30, 120, 90))
    );
    assert_eq!(ctx.debug_root_visible(root.id()), Some(true));
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(1));
}

#[test]
fn one_child_scroll_area_retains_its_three_structural_children() {
    let child = Node::widget(Custom::create(CustomParameters::new("content")));
    let (_, content) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child));
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("scroll", rect(0, 0, 100, 80), content));

    // The application child lives below a scroll surface, beside two real scrollbar widgets.
    assert_eq!(ctx.debug_root_node_count(root.id()), Some(5));
}

#[test]
fn hide_and_show_preserve_root_and_descendant_state() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), content));

    ctx.ui().set_window_visible(&root, false).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
    assert!(button.is_alive());

    ctx.ui().set_window_visible(&root, true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(true));
    assert!(button.is_alive());
}

/// Verifies object IDs remain retired after every weak window-event endpoint has been released.
#[test]
fn destroy_expires_event_endpoints_and_stable_ids_survive_address_release() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), content));
    let clone = root.clone();
    let destroyed_id = root.id();

    assert_eq!(ctx.ui().destroy_window(&root), Ok(()));
    assert_eq!(ctx.ui().destroy_window(&root), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().bring_window_to_front(&root), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().set_window_rect(&root, rect(1, 2, 3, 4)), Err(SurfaceMutationError::UnknownWindow));
    assert!(!root.events().is_alive());
    assert!(!clone.events().is_alive());
    assert_eq!(ctx.ui().set_window_visible(&clone, true), Err(SurfaceMutationError::UnknownWindow));
    assert!(!button.is_alive());

    // Drop every weak event endpoint owner before allocating the replacement. Its Rc control block
    // and address may now be reclaimed, but stable identity remains permanently retired.
    drop(root);
    drop(clone);
    let replacement = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    assert_ne!(replacement.id(), destroyed_id);
}

#[test]
fn parent_visibility_and_destruction_cover_the_complete_child_family() {
    let mut ctx = context();
    let parent = ctx.ui().create_window(Window::new("parent", rect(0, 0, 220, 180), empty_content()));
    let child = ctx
        .ui()
        .create_child_window(&parent, Window::new("child", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let grandchild = ctx
        .ui()
        .create_child_window(&child, Window::new("grandchild", rect(30, 30, 80, 60), empty_content()))
        .unwrap();
    let child_popup = ctx.ui().create_popup(&child, "child popup", empty_content()).unwrap();

    assert_eq!(ctx.debug_rendered_root_names(), ["parent", "child", "grandchild"]);
    ctx.ui().set_window_visible(&parent, false).unwrap();
    assert!(ctx.debug_rendered_root_names().is_empty());
    assert_eq!(ctx.debug_root_visible(child.id()), Some(true), "child visibility intent must be retained");
    assert_eq!(ctx.debug_root_visible(grandchild.id()), Some(true));

    ctx.ui().set_window_visible(&parent, true).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["parent", "child", "grandchild"]);

    ctx.ui().destroy_window(&parent).unwrap();
    assert_eq!(ctx.ui().set_window_visible(&child, true), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().bring_window_to_front(&grandchild), Err(SurfaceMutationError::UnknownWindow));
    assert_eq!(ctx.ui().show_popup(&child_popup), Err(SurfaceMutationError::UnknownPopup));
    assert!(!parent.events().is_alive());
    assert!(!child.events().is_alive());
    assert!(!grandchild.events().is_alive());
    assert!(!child_popup.events().is_alive());
}

/// Verifies identical definitions in separate Contexts cannot cross stable-ID membership boundaries.
#[test]
fn window_and_popup_capabilities_cannot_resolve_another_contexts_surfaces() {
    let mut first = context();
    let first_window = first.ui().create_window(Window::new("same window", rect(0, 0, 100, 80), empty_content()));
    let first_popup = first.ui().create_popup(&first_window, "same popup", empty_content()).unwrap();

    let mut second = context();
    let second_window = second.ui().create_window(Window::new("same window", rect(0, 0, 100, 80), empty_content()));
    let second_popup = second.ui().create_popup(&second_window, "same popup", empty_content()).unwrap();

    // Process-wide stable IDs differ even though both managers own their first local surface.
    // Membership checks therefore reject a foreign capability without consulting an allocation
    // address or adding a Context pointer to the public handle.
    assert_eq!(
        second.ui().set_window_rect(&first_window, rect(1, 2, 3, 4)),
        Err(SurfaceMutationError::UnknownWindow)
    );
    assert_eq!(second.ui().show_popup(&first_popup), Err(SurfaceMutationError::UnknownPopup));

    // Rejected foreign mutations leave the local records fully usable, demonstrating that failed
    // identity validation neither selects nor partially changes the local surface.
    second.ui().set_window_rect(&second_window, rect(5, 6, 70, 60)).unwrap();
    second.ui().show_popup(&second_popup).unwrap();
    assert_eq!(
        second.debug_root_rect(second_window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((5, 6, 70, 60))
    );
    assert_eq!(second.debug_popup_visible(&second_popup), Some(true));
}

#[test]
#[allow(clippy::result_large_err)] // The assertion exercises the ownership-preserving mutation result.
fn dynamic_container_root_changes_descendants_without_replacing_the_root() {
    let mut ctx = context();
    let (column, content) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
    let root = ctx.ui().create_window(Window::new("dynamic", rect(0, 0, 140, 100), content));
    let root_id = root.id();
    let (button, widget) = Button::create(ButtonParameters::new("new child"));

    let inserted = column.try_update(|column| column.push(widget)).unwrap();
    assert!(inserted.is_ok());
    ctx.update_and_render_ui();
    assert_eq!(root.id(), root_id);
    assert!(button.is_alive());
    assert_eq!(ctx.debug_root_node_count(root_id), Some(2));

    assert_eq!(column.try_update(|linear: &mut Linear| linear.remove_drop(0)), Some(Some(true)));
    assert!(!button.is_alive());
    assert!(root.events().is_alive());
}

#[test]
fn showing_a_popup_atomically_hides_and_dismisses_the_previous_one() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let first = ctx.ui().create_popup(&source, "first", empty_content()).unwrap();
    let second = ctx.ui().create_popup(&source, "second", empty_content()).unwrap();
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    dispatcher.subscribe(first.events(), record).unwrap();
    let mut submissions = Vec::new();

    ctx.ui().show_popup_at(&first, rect(12, 18, 90, 1)).unwrap();
    assert_eq!(ctx.debug_popup_visible(&first), Some(true));
    ctx.ui().show_popup_at(&second, rect(40, 55, 120, 1)).unwrap();

    assert_eq!(ctx.debug_popup_visible(&first), Some(false));
    assert_eq!(ctx.debug_popup_visible(&second), Some(true));
    assert_eq!(
        ctx.debug_popup_rect(&second).map(|anchor| (anchor.x, anchor.y, anchor.width, anchor.height)),
        Some((40, 55, 120, 1))
    );
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
}

#[test]
fn hiding_a_window_hides_its_active_popup() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let first = ctx.ui().create_popup(&source, "first", empty_content()).unwrap();
    let second = ctx.ui().create_popup(&source, "second", empty_content()).unwrap();
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    dispatcher.subscribe(second.events(), record).unwrap();
    let mut submissions = Vec::new();

    ctx.ui().show_popup(&first).unwrap();
    ctx.ui().hide_popup(&first).unwrap();
    ctx.ui().show_popup(&second).unwrap();
    ctx.ui().set_window_visible(&source, false).unwrap();

    assert_eq!(ctx.debug_popup_visible(&first), Some(false));
    assert_eq!(ctx.debug_popup_visible(&second), Some(false));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
}

/// Verifies popup handles become stale and their endpoints expire when their owner is destroyed.
#[test]
fn stale_popup_mutations_fail_after_owner_destruction() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    let clone = popup.clone();
    let destroyed_id = popup.id();
    assert_eq!(ctx.ui().destroy_window(&source), Ok(()));

    // Popup definitions have no independent destruction operation. Destroying the owning window
    // makes its handles stale, expires their endpoints, and makes every mutation fail consistently.
    assert_eq!(ctx.ui().show_popup_at(&popup, rect(20, 30, 40, 1)), Err(SurfaceMutationError::UnknownPopup));
    assert_eq!(ctx.ui().set_popup_options(&popup, WindowOption::FRAME), Err(SurfaceMutationError::UnknownPopup));
    assert_eq!(ctx.ui().hide_popup(&popup), Err(SurfaceMutationError::UnknownPopup));
    assert!(!popup.events().is_alive());
    assert!(!clone.events().is_alive());
    assert_eq!(ctx.ui().show_popup(&clone), Err(SurfaceMutationError::UnknownPopup));

    // Releasing every weak endpoint allows the old allocation address to be reused without making
    // its permanently retired stable ID eligible for a later popup.
    drop(source);
    drop(popup);
    drop(clone);
    let replacement_owner = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let replacement = ctx.ui().create_popup(&replacement_owner, "popup", empty_content()).unwrap();
    assert_ne!(replacement.id(), destroyed_id);
}

#[test]
fn outside_popup_press_hides_and_records_typed_submission() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().show_popup_at(&popup, rect(20, 20, 80, 60)).unwrap();
    let mut widget_event_dispatcher = crate::event::WidgetEventDispatcher::new();
    fn record(events: &mut Vec<PopupEvent>, event: &PopupEvent) {
        events.push(*event);
    }
    widget_event_dispatcher.subscribe(popup.events(), record).unwrap();
    let mut submissions = Vec::new();
    ctx.update_and_render_ui();

    ctx.mousedown(200, 180, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert!(widget_event_dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed]);
    ctx.ui().show_popup(&popup).unwrap();
    ctx.ui().hide_popup(&popup).unwrap();
    assert!(widget_event_dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, [PopupEvent::Dismissed, PopupEvent::Dismissed]);
}

#[test]
fn outside_popup_press_dismisses_then_routes_once_to_the_revealed_root() {
    let mut ctx = context();
    let (button, content) = button_content("behind");
    let mut dispatcher = event_counter(button);
    let mut submissions = 0;
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 180, 120), content));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let popup = ctx.ui().create_popup(&window, "popup", empty_content()).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().show_popup_at(&popup, rect(80, 60, 60, 40)).unwrap();
    ctx.update_ui(Dimensioni::new(320, 240));

    ctx.mousedown(15, 15, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
}

#[test]
fn eventless_update_and_paint_have_separate_phase_counts() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(10, 10, 120, 90), empty_content()));

    ctx.update_and_render_ui();
    let metrics = ctx.debug_root_runtime_metrics(root.id()).unwrap();
    assert_eq!(metrics.tree_layouts, 2);
    assert_eq!(metrics.updates, 1);
    assert_eq!(metrics.paints, 1);
}

#[test]
fn warmed_container_measurement_and_layout_allocate_nothing() {
    let child = |name| Node::widget(Custom::create(CustomParameters::new(name)));
    let (_, row) = Linear::create(LinearParameters::horizontal([LinearItem::flex(child("row"), 1.0)]));
    let (_, grid) = Grid::create(GridParameters::new([TrackSize::Flex(1.0)], [TrackSize::Content], [child("grid")]));
    let (_, fixed_column) = Linear::create(LinearParameters::vertical([LinearItem::fixed(child("column"), 20)]));
    let (_, disclosure) = Disclosure::create(DisclosureParameters::header("expanded", true, [child("disclosure")]));
    let (_, scroll) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, child("scroll")));
    let (_, content) = Linear::create(LinearParameters::vertical([row, grid, fixed_column, disclosure, scroll]));
    let mut ctx = context();
    ctx.ui().create_window(Window::new("allocation probe", rect(10, 10, 300, 220), content));
    let dimensions = Dimensioni::new(640, 480);

    ctx.update_ui(dimensions);
    ctx.update_ui(dimensions);
    let measurement = AllocationMeasurement::begin();
    ctx.update_ui(dimensions);
    let allocations = measurement.finish();

    assert_eq!(allocations.events, 0, "steady measurement/layout allocated {} bytes", allocations.bytes);
}

#[test]
fn warmed_recursive_menu_layout_reuses_every_slot_and_path_allocation() {
    let (_, recent_item) = MenuItem::create(MenuItemParameters::new("Recent project"));
    let menu_bar = MenuBar::new([Menu::new("File").submenu(Menu::new("Recent").item(recent_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("allocation menu", rect(10, 10, 300, 220), empty_content()).menu_bar(menu_bar));
    let dimensions = Dimensioni::new(640, 480);

    // Open both levels once so bar slots, popup slots, visible order, and popup-path scratch all
    // reach their steady capacities before the allocation counter begins.
    ctx.update_ui(dimensions);
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let submenu = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, submenu);
    assert_eq!(ctx.debug_active_menu_row_rects().len(), 2);
    ctx.update_ui(dimensions);

    let measurement = AllocationMeasurement::begin();
    ctx.update_ui(dimensions);
    let allocations = measurement.finish();

    assert_eq!(allocations.events, 0, "steady menu layout allocated {} bytes", allocations.bytes);
}

#[test]
fn fronting_changes_only_cross_root_z_order() {
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let _second = ctx.ui().create_window(Window::new("second", rect(20, 20, 100, 80), empty_content()));
    assert_eq!(ctx.debug_rendered_root_names(), ["first", "second"]);

    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["second", "first"]);
}

#[test]
fn fixed_layers_validate_and_managed_roots_reject_direct_assignment() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()));
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(20, 20, 100, 80), empty_content()))
        .unwrap();

    assert_eq!(ctx.ui().window_layer(&window), Ok(LayerBinding::Fixed(DEFAULT_LAYER)));
    assert_eq!(ctx.ui().window_layer(&dialog), Ok(LayerBinding::Modal));
    assert_eq!(
        ctx.ui().set_window_layer(&window, MAX_LAYER + 1),
        Err(SurfaceMutationError::InvalidLayer(MAX_LAYER + 1))
    );
    assert_eq!(ctx.ui().set_window_layer(&dialog, 3), Err(SurfaceMutationError::ManagedLayer));

    ctx.ui().set_window_layer(&window, MIN_LAYER).unwrap();
    assert_eq!(ctx.ui().window_layer(&window), Ok(LayerBinding::Fixed(MIN_LAYER)));
}

#[test]
fn child_windows_inherit_the_family_layer_and_front_only_among_siblings() {
    let mut ctx = context();
    let parent = ctx.ui().create_window(Window::new("parent", rect(0, 0, 220, 180), empty_content()));
    ctx.ui().set_window_layer(&parent, 3).unwrap();
    let first = ctx
        .ui()
        .create_child_window(&parent, Window::new("first child", rect(10, 30, 80, 60), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_child_window(&parent, Window::new("second child", rect(30, 50, 80, 60), empty_content()))
        .unwrap();
    let peer = ctx.ui().create_window(Window::new("peer", rect(0, 0, 80, 60), empty_content()));
    ctx.ui().set_window_layer(&peer, 3).unwrap();

    assert_eq!(ctx.ui().window_layer(&first), Ok(LayerBinding::Fixed(3)));
    assert_eq!(ctx.ui().set_window_layer(&first, 5), Err(SurfaceMutationError::ManagedLayer));
    assert_eq!(ctx.debug_rendered_root_names(), ["parent", "first child", "second child", "peer"]);

    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(
        ctx.debug_rendered_root_names(),
        ["parent", "second child", "first child", "peer"],
        "fronting a child changes only its direct sibling chronology"
    );

    ctx.ui().set_window_layer(&parent, 7).unwrap();
    assert_eq!(ctx.ui().window_layer(&first), Ok(LayerBinding::Fixed(7)));
    assert_eq!(ctx.ui().window_layer(&second), Ok(LayerBinding::Fixed(7)));
}

#[test]
fn failed_child_and_dialog_creation_preserves_each_window_for_retry() {
    let mut ctx = context();
    let parent = ctx.ui().create_window(Window::new("parent", rect(0, 0, 220, 180), empty_content()));
    let dialog = ctx
        .ui()
        .create_dialog(&parent, Window::new("dialog", rect(20, 20, 120, 90), empty_content()))
        .unwrap();

    // A modal parent is invalid for both kinds of owned root. Each error must retain the exact
    // unique Window, including its live typed body, so the caller can retry under `parent`.
    let (child_body, child_content) = TextBlock::create(TextBlockParameters::new("child body"));
    let child_error = ctx
        .ui()
        .create_child_window(&dialog, Window::new("invalid child", rect(0, 0, 20, 20), child_content))
        .expect_err("a modal dialog cannot own a structural child window");
    assert_eq!(child_error.reason(), SurfaceMutationError::InvalidChildWindowParent);
    assert!(child_body.is_alive(), "the rejected child Window must still own its body");
    ctx.ui().create_child_window(&parent, child_error.into_input()).unwrap();
    assert!(child_body.is_alive(), "retry must transfer the same body into the child surface");

    let (dialog_body, dialog_content) = TextBlock::create(TextBlockParameters::new("nested dialog body"));
    let dialog_error = ctx
        .ui()
        .create_dialog(&dialog, Window::new("invalid dialog", rect(5, 5, 30, 30), dialog_content))
        .expect_err("a modal dialog cannot own another modal dialog");
    assert_eq!(dialog_error.reason(), SurfaceMutationError::InvalidDialogOwner);
    assert!(dialog_body.is_alive(), "the rejected dialog Window must still own its body");
    ctx.ui().create_dialog(&parent, dialog_error.into_input()).unwrap();
    assert!(dialog_body.is_alive(), "retry must transfer the same body into the dialog surface");
}

#[test]
fn failed_popup_creation_preserves_its_node_for_retry() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 220, 180), empty_content()));
    let mut foreign = context();
    let foreign_owner = foreign.ui().create_window(Window::new("foreign", rect(0, 0, 20, 20), empty_content()));
    let (popup_body, popup_content) = TextBlock::create(TextBlockParameters::new("popup body"));

    // Authentication must fail before the Node moves into the receiving forest. Standard error
    // chaining exposes the concrete reason, while the typed recovery method returns only a Node.
    let popup_error = ctx
        .ui()
        .create_popup(&foreign_owner, "invalid popup", popup_content)
        .expect_err("a foreign window cannot own a popup in this Context");
    assert_eq!(popup_error.reason(), SurfaceMutationError::UnknownWindow);
    assert_eq!(
        std::error::Error::source(&popup_error).map(ToString::to_string),
        Some(SurfaceMutationError::UnknownWindow.to_string())
    );
    assert!(popup_body.is_alive(), "the rejected popup error must retain its unique Node");

    ctx.ui().create_popup(&owner, "retried popup", popup_error.into_input()).unwrap();
    assert!(popup_body.is_alive(), "retry must transfer the same Node into the popup surface");
}

#[test]
fn parent_content_clip_accumulates_through_nested_child_windows() {
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("root", rect(10, 10, 240, 190), empty_content()).child_window_clip(ChildWindowClip::Content));
    let child = ctx
        .ui()
        .create_child_window(
            &root,
            Window::new("child", rect(30, 40, 180, 130), empty_content()).child_window_clip(ChildWindowClip::Content),
        )
        .unwrap();
    let grandchild = ctx
        .ui()
        .create_child_window(&child, Window::new("grandchild", rect(0, 0, 320, 240), empty_content()))
        .unwrap();
    ctx.update_and_render_ui();

    let viewport = rect(0, 0, 320, 240);
    let root_body = ctx.debug_root_body(root.id()).unwrap();
    let child_body = ctx.debug_root_body(child.id()).unwrap();
    let root_clip = viewport.intersect(&root_body).unwrap();
    let grandchild_clip = root_clip.intersect(&child_body).unwrap();
    assert_eq!(ctx.debug_root_clip(root.id()).map(rect_values), Some(rect_values(viewport)));
    assert_eq!(ctx.debug_root_clip(child.id()).map(rect_values), Some(rect_values(root_clip)));
    assert_eq!(ctx.debug_root_clip(grandchild.id()).map(rect_values), Some(rect_values(grandchild_clip)));
}

#[test]
fn unclipped_parent_passes_only_its_inherited_clip_to_children() {
    let mut ctx = context();
    let parent = ctx.ui().create_window(Window::new("parent", rect(40, 40, 120, 90), empty_content()));
    let child = ctx
        .ui()
        .create_child_window(&parent, Window::new("child", rect(0, 0, 320, 240), empty_content()))
        .unwrap();
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_root_clip(parent.id()).map(rect_values), Some((0, 0, 320, 240)));
    assert_eq!(ctx.debug_root_clip(child.id()).map(rect_values), Some((0, 0, 320, 240)));
}

#[test]
fn raising_reorders_only_inside_a_fixed_layer() {
    let mut ctx = context();
    let _high = ctx.ui().create_window(Window::new("high", rect(0, 0, 100, 80), empty_content()));
    let low_first = ctx.ui().create_window(Window::new("low first", rect(0, 0, 100, 80), empty_content()));
    let low_second = ctx.ui().create_window(Window::new("low second", rect(0, 0, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&low_first, 2).unwrap();
    ctx.ui().set_window_layer(&low_second, 2).unwrap();

    assert_eq!(ctx.debug_rendered_root_names(), ["low first", "low second", "high"]);
    ctx.ui().bring_window_to_front(&low_first).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["low second", "low first", "high"]);
}

#[test]
fn popup_inherits_its_parent_layer_and_uses_only_that_layers_transient_tier() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let same_layer = ctx.ui().create_window(Window::new("same layer", rect(0, 0, 100, 80), empty_content()));
    let higher = ctx.ui().create_window(Window::new("higher", rect(0, 0, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&source, 2).unwrap();
    ctx.ui().set_window_layer(&same_layer, 2).unwrap();
    ctx.ui().set_window_layer(&higher, 3).unwrap();
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();

    ctx.ui().show_popup_at(&popup, rect(10, 10, 60, 40)).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["source", "same layer", "popup", "higher"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);
    ctx.update_and_render_ui();

    // Window ownership remains live while the popup is visible: moving the owner moves its active
    // popup to that window's transient tier without giving it an independent layer.
    ctx.ui().set_window_layer(&source, 4).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["same layer", "higher", "source", "popup"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);
}

#[test]
fn clipped_child_windows_stay_between_parent_content_and_parent_menu() {
    let (item, item_value) = MenuItem::create(MenuItemParameters::new("Parent action"));
    let mut item_dispatcher = event_counter(item.submitted());
    let menu_bar = MenuBar::new([Menu::new("Root").item(item_value)]);
    let (parent_state, parent_content) = OrderedProbe::create(WidgetOption::NONE);
    let (child_state, child_content) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let parent = ctx.ui().create_window(
        Window::new("parent", rect(0, 0, 220, 180), parent_content)
            .menu_bar(menu_bar)
            .child_window_clip(ChildWindowClip::Content),
    );
    ctx.ui()
        .set_window_options(
            &parent,
            WindowOption::NO_TITLE | WindowOption::NO_CLOSE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING,
        )
        .unwrap();
    let child = ctx
        .ui()
        .create_child_window(&parent, Window::new("child", rect(0, 0, 110, 180), child_content))
        .unwrap();
    ctx.ui()
        .set_window_options(
            &child,
            WindowOption::NO_TITLE | WindowOption::NO_CLOSE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING,
        )
        .unwrap();
    ctx.update_and_render_ui();

    let bar = ctx.debug_menu_bar_rect(parent.id()).unwrap();
    let heading = ctx.debug_menu_anchor_rects(parent.id()).unwrap()[0].unwrap();
    let body = ctx.debug_root_body(parent.id()).unwrap();
    assert_eq!(ctx.debug_root_clip(child.id()).map(rect_values), Some(rect_values(body)));

    // The child geometrically overlaps the bar, but its inherited content clip makes the parent
    // heading the front input surface and lets the click open the parent-owned popup.
    assert!(bar.contains(&crate::vec2(heading.x, heading.y)));
    click_rect(&mut ctx, heading);
    assert_eq!(ctx.debug_active_popup_names(), ["parent Root Menu"]);
    assert_eq!(child_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    // The popup occupies the parent's transient tier above its child family, so an item inside the
    // parent body cannot fall through to the clipped child behind it.
    let item_row = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, item_row);
    let mut submissions = 0;
    assert!(item_dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);
    assert_eq!(child_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    // Within the application body, the child wins where it has geometry; uncovered body pixels
    // fall through to the parent content, exactly reversing the recursive paint relationship.
    ctx.mousedown(body.x + 10, body.y + 10, MouseButton::LEFT);
    ctx.mouseup(body.x + 10, body.y + 10, MouseButton::LEFT);
    ctx.mousedown(body.x + body.width - 10, body.y + 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(child_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    assert_eq!(parent_state.try_read(|state| state.events.clone()), Some(vec!["down"]));
}

#[test]
fn parent_chrome_preempts_an_overlapping_unclipped_child() {
    let (child_state, child_content) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let parent = ctx.ui().create_window(Window::new("parent", rect(30, 30, 160, 120), empty_content()));
    let child = ctx
        .ui()
        .create_child_window(&parent, Window::new("child", rect(30, 30, 160, 120), child_content))
        .unwrap();
    ctx.ui()
        .set_window_options(
            &child,
            WindowOption::NO_TITLE | WindowOption::NO_CLOSE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING,
        )
        .unwrap();
    ctx.update_and_render_ui();

    // The visible frame border is a parent-owned hit shield even though it has no drag action. This
    // keeps input consistent with the border pixels repainted after the child content.
    let border = crate::vec2(30, 30);
    ctx.mousedown(border.x, border.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(parent.id()));
    assert_eq!(child_state.try_read(|state| state.events.clone()), Some(Vec::new()));
    ctx.mouseup(border.x, border.y, MouseButton::LEFT);
    ctx.update_and_render_ui();

    // Actionable chrome uses the same precedence and begins the parent's resize capture instead of
    // delivering the overlapping press to the child body.
    let resize = ctx.debug_root_chrome(parent.id()).unwrap().2.unwrap();
    let point = crate::vec2(resize.x + 1, resize.y + 1);
    ctx.mousedown(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_root_resizing(parent.id()), Some(true));
    assert_eq!(child_state.try_read(|state| state.events.clone()), Some(Vec::new()));
}

#[test]
fn higher_layer_window_occludes_lower_popup_for_outside_dismissal() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let higher = ctx.ui().create_window(Window::new("higher", rect(0, 0, 100, 80), empty_content()));
    let chromeless = WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE;
    ctx.ui().set_window_options(&source, chromeless).unwrap();
    ctx.ui().set_window_options(&higher, chromeless).unwrap();
    ctx.ui().set_window_layer(&source, 2).unwrap();
    ctx.ui().set_window_layer(&higher, 3).unwrap();
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();
    ctx.ui().set_popup_options(&popup, chromeless).unwrap();
    ctx.ui().show_popup_at(&popup, rect(10, 10, 60, 40)).unwrap();
    ctx.update_and_render_ui();

    // The point lies inside the popup rectangle, but the higher-layer window is the visible input
    // target there. Treating raw popup bounds as visibility would leave an occluded menu open.
    ctx.mousedown(20, 20, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
    assert_eq!(ctx.debug_active_root(), Some(higher.id()));
}

/// Verifies hidden popup handles become stale and their event endpoints expire with their owner.
#[test]
fn owner_destruction_stales_popup_handles_and_expires_their_endpoints() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(0, 0, 100, 80), empty_content()));
    let popup = ctx.ui().create_popup(&source, "popup", empty_content()).unwrap();

    // Popup definitions and their endpoints remain alive while hidden. Owner destruction removes
    // the definitions, makes the handles stale, and expires the endpoints.
    ctx.ui().show_popup(&popup).unwrap();
    ctx.ui().hide_popup(&popup).unwrap();
    assert!(popup.events().is_alive());

    assert_eq!(ctx.ui().destroy_window(&source), Ok(()));
    assert!(!popup.events().is_alive());
}

#[test]
fn active_root_routes_keyboard_without_crossing_layer_boundaries() {
    let (low_state, low_content) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let low = ctx.ui().create_window(Window::new("low", rect(0, 0, 100, 80), low_content));
    let _high = ctx.ui().create_window(Window::new("high", rect(160, 120, 100, 80), empty_content()));
    ctx.ui().set_window_layer(&low, 0).unwrap();
    ctx.update_and_render_ui();
    let low_body = ctx.debug_root_body(low.id()).unwrap();

    ctx.mousedown(low_body.x + 1, low_body.y + 1, MouseButton::LEFT);
    ctx.mouseup(low_body.x + 1, low_body.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_root(), Some(low.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["low", "high"]);

    // The higher-layer window remains visually frontmost, but keyboard input belongs to the root
    // explicitly activated by the completed pointer press.
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("low layer");
    ctx.update_and_render_ui();
    assert_eq!(low_state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "key-down", "text"]));
}

#[test]
fn blank_root_press_confines_drag_to_the_pressed_root() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), probe));
    for window in [&first, &second] {
        ctx.ui()
            .set_window_options(window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
    }
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    let second_body = ctx.debug_root_body(second.id()).unwrap();
    let first_point = crate::vec2(first_body.x + 1, first_body.y + 1);
    let second_point = crate::vec2(second_body.x + 1, second_body.y + 1);

    ctx.mousedown(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(first.id()), Some(false));

    ctx.mousemove(second_point.x, second_point.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));
}

#[test]
fn scroll_and_new_press_reach_a_hovered_lower_layer_independently_of_activation() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::GRAB_SCROLL);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), probe));
    for window in [&first, &second] {
        ctx.ui()
            .set_window_options(window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
    }
    ctx.ui().set_window_layer(&second, MIN_LAYER).unwrap();
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    let second_body = ctx.debug_root_body(second.id()).unwrap();
    let first_point = crate::vec2(first_body.x + 1, first_body.y + 1);
    let second_point = crate::vec2(second_body.x + 1, second_body.y + 1);

    ctx.mousedown(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.mouseup(first_point.x, first_point.y, MouseButton::LEFT);
    ctx.mousemove(second_point.x, second_point.y);
    ctx.scroll(0, 1);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move", "scroll"]));

    ctx.mousedown(second_point.x, second_point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["move", "scroll", "down"]));
}

#[test]
fn pointer_captured_root_remains_the_keyboard_and_text_input_root() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let first = ctx.ui().create_window(Window::new("first", rect(0, 0, 100, 80), probe));
    let second = ctx.ui().create_window(Window::new("second", rect(160, 120, 100, 80), empty_content()));
    ctx.update_and_render_ui();

    let first_body = ctx.debug_root_body(first.id()).unwrap();
    ctx.mousedown(first_body.x + 1, first_body.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(first.id()), Some(true));

    ctx.ui().bring_window_to_front(&second).unwrap();
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("captured");
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "key-down", "text"]));
}

#[test]
fn visible_dialog_is_the_sole_pointer_root_and_remains_frontmost() {
    let mut ctx = context();
    let (behind_button, behind_content) = button_content("behind");
    let mut behind_dispatcher = event_counter(behind_button);
    let mut behind_submissions = 0;
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), behind_content));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let (dialog_button, dialog_content) = button_content("dialog");
    let mut dialog_dispatcher = event_counter(dialog_button);
    let mut dialog_submissions = 0;
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), dialog_content))
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.update_and_render_ui();

    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(!behind_dispatcher.dispatch(&mut behind_submissions));

    ctx.mousedown(130, 110, MouseButton::LEFT);
    ctx.mouseup(130, 110, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(dialog_dispatcher.dispatch(&mut dialog_submissions));
    assert_eq!(dialog_submissions, 1);

    ctx.ui().bring_window_to_front(&window).unwrap();
    ctx.ui().set_window_visible(&window, true).unwrap();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog"]);
    // Numeric z-indices are local ordering values; the modal band remains structurally above the
    // fixed band even after the ordinary window receives the newer number.
    assert_eq!(ctx.debug_modal_root(), Some(dialog.id()));

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), None);
    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(behind_dispatcher.dispatch(&mut behind_submissions));
    assert_eq!(behind_submissions, 1);
}

#[test]
fn active_dialog_accepts_only_its_own_popup_in_the_modal_input_group() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()));
    let window_popup = ctx.ui().create_popup(&window, "window popup", empty_content()).unwrap();
    let (popup_button, popup_content) = button_content("popup");
    let mut dispatcher = event_counter(popup_button);
    let mut submissions = 0;
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), empty_content()))
        .unwrap();
    let popup = ctx.ui().create_popup(&dialog, "popup", popup_content).unwrap();
    ctx.ui()
        .set_popup_options(&popup, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.mousemove(10, 10);
    ctx.update_and_render_ui();

    assert_eq!(ctx.ui().show_popup(&window_popup), Err(SurfaceMutationError::InvalidPopupParent));
    ctx.ui().show_popup_at(&popup, rect(0, 0, 100, 80)).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_rendered_root_names(), ["window", "dialog", "popup"]);
    assert_eq!(ctx.debug_active_popup_names(), ["popup"]);

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(dispatcher.dispatch(&mut submissions));
    assert_eq!(submissions, 1);

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
}

#[test]
fn modal_activation_suspends_underlying_focus_and_restores_it_when_closed() {
    let (state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(0, 0, 100, 80), probe));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(120, 100, 100, 80), empty_content()))
        .unwrap();
    ctx.ui()
        .set_window_options(&dialog, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.mouseup(10, 10, MouseButton::LEFT);
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.ui().set_window_visible(&dialog, true).unwrap();
    let updates_before_modal_input = state.try_read(|state| state.updates).unwrap();
    ctx.key(KeyEvent::pressed(Key::Shift, Modifiers::SHIFT));
    ctx.text("blocked");
    ctx.key(KeyEvent::released(Key::Shift, Modifiers::NONE));
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.updates), Some(updates_before_modal_input));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    ctx.ui().set_window_visible(&dialog, false).unwrap();
    ctx.text("restored");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text"]));

    ctx.mousedown(10, 10, MouseButton::LEFT);
    ctx.text("accepted");
    ctx.update_ui(Dimensioni::new(320, 240));
    assert_eq!(state.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text", "down", "text"]));
}

#[test]
fn modal_routing_revokes_underlying_chrome_capture_before_drag_continues() {
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    let dialog = ctx
        .ui()
        .create_dialog(&window, Window::new("dialog", rect(170, 120, 100, 80), empty_content()))
        .unwrap();
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(window.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(true));
    assert_eq!(ctx.debug_root_moving(window.id()), Some(true));
    let before = ctx.debug_root_rect(window.id()).unwrap();

    ctx.ui().set_window_visible(&dialog, true).unwrap();
    ctx.mousemove(title.x + 20, title.y + 20);
    ctx.mouseup(title.x + 20, title.y + 20, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(window.id()), Some(false));
    assert_eq!(ctx.debug_root_active(window.id()), Some(false));
    assert_eq!(
        ctx.debug_root_rect(window.id()).map(|rect| (rect.x, rect.y, rect.width, rect.height)),
        Some((before.x, before.y, before.width, before.height))
    );
}

#[test]
fn hiding_or_destroying_the_front_dialog_reveals_the_next_visible_dialog() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();

    ctx.ui().set_window_visible(&first, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.ui().set_window_visible(&second, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));

    ctx.update_and_render_ui();
    let close = ctx.debug_root_chrome(second.id()).unwrap().1.unwrap();
    ctx.mousedown(close.x + close.width / 2, close.y + close.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(second.id()), Some(false));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.ui().set_window_visible(&second, true).unwrap();
    assert_eq!(ctx.ui().destroy_window(&second), Ok(()));
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.ui().destroy_window(&first), Ok(()));
    assert_eq!(ctx.debug_modal_root(), None);
}

#[test]
fn fronting_a_visible_dialog_makes_it_the_active_modal() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let middle = ctx
        .ui()
        .create_dialog(&owner, Window::new("middle", rect(30, 30, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();
    ctx.ui().set_window_visible(&first, true).unwrap();
    ctx.ui().set_window_visible(&middle, true).unwrap();
    ctx.ui().set_window_visible(&second, true).unwrap();

    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.debug_rendered_root_names(), ["owner", "middle", "second", "first"]);
    // Raising the ordinary owner moves only its fixed-band subtree and leaves modal sibling order
    // untouched.
    ctx.ui().bring_window_to_front(&owner).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));

    ctx.ui().set_window_visible(&first, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
    ctx.ui().set_window_visible(&second, false).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(middle.id()));
    ctx.ui().set_window_visible(&second, true).unwrap();

    ctx.ui().bring_window_to_front(&second).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(second.id()));
}

#[test]
fn fronting_a_dialog_closes_the_previous_modal_groups_popup() {
    let mut ctx = context();
    let owner = ctx.ui().create_window(Window::new("owner", rect(0, 0, 10, 10), empty_content()));
    let first = ctx
        .ui()
        .create_dialog(&owner, Window::new("first", rect(20, 20, 120, 90), empty_content()))
        .unwrap();
    let second = ctx
        .ui()
        .create_dialog(&owner, Window::new("second", rect(40, 40, 120, 90), empty_content()))
        .unwrap();
    ctx.ui().set_window_visible(&first, true).unwrap();
    ctx.ui().set_window_visible(&second, true).unwrap();
    let popup = ctx.ui().create_popup(&second, "popup", empty_content()).unwrap();
    ctx.ui().show_popup_at(&popup, rect(50, 50, 40, 30)).unwrap();

    // Switching dialog groups dismisses the transient that belonged to the previously active group
    // before changing modal z-order.
    ctx.ui().bring_window_to_front(&first).unwrap();
    assert_eq!(ctx.debug_modal_root(), Some(first.id()));
    assert_eq!(ctx.debug_popup_visible(&popup), Some(false));
}

#[test]
fn title_drag_and_close_record_typed_window_events() {
    /// Preserves the exact order in which one window's combined event stream is dispatched.
    fn record(events: &mut Vec<WindowEvent>, event: &WindowEvent) {
        // Copy the concrete event value so the assertions cover both variants without an erased or
        // auxiliary test-only representation.
        events.push(*event);
    }

    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    let mut dispatcher = crate::event::WidgetEventDispatcher::new();
    dispatcher.subscribe(root.events(), record).unwrap();
    let mut events = Vec::new();
    ctx.update_and_render_ui();
    let (title, _, _) = ctx.debug_root_chrome(root.id()).unwrap();
    let title = title.unwrap();
    let drag_x = title.x + 2;
    let drag_y = title.y + 2;

    ctx.mousedown(drag_x, drag_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_moving(root.id()), Some(true));
    assert!(!dispatcher.dispatch(&mut events));

    ctx.mousemove(drag_x + 10, drag_y + 8);
    ctx.update_and_render_ui();
    assert!(dispatcher.dispatch(&mut events));
    assert!(matches!(
        events.as_slice(),
        [WindowEvent::GeometryChanged { rect }]
            if (rect.x, rect.y, rect.width, rect.height) == (40, 38, 140, 100)
    ));

    ctx.mouseup(drag_x + 10, drag_y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));

    let close = ctx.debug_root_chrome(root.id()).unwrap().1.unwrap();
    let close_x = close.x + close.width / 2;
    let close_y = close.y + close.height / 2;
    ctx.mousedown(close_x, close_y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_visible(root.id()), Some(false));
    assert!(dispatcher.dispatch(&mut events));
    assert!(matches!(
        events.as_slice(),
        [WindowEvent::GeometryChanged { rect }, WindowEvent::CloseRequested]
            if (rect.x, rect.y, rect.width, rect.height) == (40, 38, 140, 100)
    ));
}

#[test]
fn resize_overlay_preempts_content_where_the_grip_overlaps_the_root_body() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), probe));
    ctx.update_and_render_ui();

    let resize = ctx.debug_root_chrome(root.id()).unwrap().2.unwrap();
    let body = ctx.debug_root_body(root.id()).unwrap();
    let press = crate::vec2(resize.x + 1, resize.y + 1);
    assert!(
        resize.contains(&press) && body.contains(&press),
        "the regression requires the painted grip to overlap content"
    );

    ctx.mousemove(press.x, press.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.hovered), Some(false));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));
    ctx.scroll(0, 1);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    let before = ctx.debug_root_rect(root.id()).unwrap();
    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_resizing(root.id()), Some(true));
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousemove(press.x + 12, press.y + 8);
    ctx.mouseup(press.x + 12, press.y + 8, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(
        ctx.debug_root_rect(root.id()).map(|rect| (rect.width, rect.height)),
        Some((before.width + 12, before.height + 8))
    );
}

#[test]
fn content_capture_remains_exclusive_while_dragging_across_root_chrome() {
    let (probe_state, probe) = OrderedProbe::create(WidgetOption::NONE);
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), probe));
    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    let resize = ctx.debug_root_chrome(root.id()).unwrap().2.unwrap();
    let press = crate::vec2(body.x + 1, body.y + 1);
    let over_chrome = crate::vec2(resize.x + 1, resize.y + 1);
    assert!(!resize.contains(&press));
    assert!(body.contains(&over_chrome), "the capture regression requires chrome overlapping content");

    ctx.mousedown(press.x, press.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down"]));
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));

    ctx.mousemove(over_chrome.x, over_chrome.y);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "drag"]));
    assert_eq!(probe_state.try_read(|state| state.hovered), Some(true));
    assert_eq!(ctx.debug_root_resizing(root.id()), Some(false));

    ctx.mouseup(over_chrome.x, over_chrome.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(probe_state.try_read(|state| state.events.clone()), Some(vec!["down", "drag", "up"]));
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
}

#[test]
fn hiding_and_showing_root_does_not_restore_chrome_capture() {
    let mut ctx = context();
    let root = ctx.ui().create_window(Window::new("window", rect(30, 30, 140, 100), empty_content()));
    ctx.update_and_render_ui();
    let title = ctx.debug_root_chrome(root.id()).unwrap().0.unwrap();

    ctx.mousedown(title.x + 2, title.y + 2, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(true));
    assert_eq!(ctx.debug_root_moving(root.id()), Some(true));

    ctx.ui().set_window_visible(&root, false).unwrap();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));

    ctx.ui().set_window_visible(&root, true).unwrap();
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_root_has_pointer_capture(root.id()), Some(false));
    assert_eq!(ctx.debug_root_active(root.id()), Some(false));
}

/// Verifies that menu placement remains a live surface-slot relationship after owner movement.
#[test]
fn declarative_menu_popups_follow_heading_and_submenu_edges_when_the_window_moves() {
    // A nested File menu exercises both relationship variants. Edit supplies another top-level
    // heading, ensuring the helper's declaration order matches the compiled popup order.
    let (_, open_item) = MenuItem::create(MenuItemParameters::new("Open"));
    let (_, recent_item) = MenuItem::create(MenuItemParameters::new("Recent Project"));
    let (_, copy_item) = MenuItem::create(MenuItemParameters::new("Copy"));
    let menu_bar = MenuBar::new([
        Menu::new("File").item(open_item).submenu(Menu::new("Recent").item(recent_item)),
        Menu::new("Edit").item(copy_item),
    ]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu geometry", rect(30, 25, 190, 140), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // The first compiled relationship belongs to File. Opening it must use the current heading
    // slot's exact left and bottom edges, not a caller-computed or creation-time screen rectangle.
    let anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    assert_eq!(anchors.len(), 3);
    let file_heading = anchors[0].expect("the bar surface's File slot must have committed geometry");
    click_rect(&mut ctx, file_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu geometry File Menu"]);
    let file_popup = ctx.debug_active_popup_rects()[0];
    assert_eq!(file_popup.x, file_heading.x);
    assert_eq!(file_popup.y, file_heading.y + file_heading.height);

    // The parent popup surface commits the Recent row slot. Opening that row must place the child
    // at its exact right edge and preserve its y coordinate.
    let recent_row = ctx.debug_menu_anchor_rects(root.id()).unwrap()[1].expect("an active parent surface must lay out its submenu slot");
    click_rect(&mut ctx, recent_row);
    assert_eq!(ctx.debug_active_popup_names(), ["menu geometry File Menu", "menu geometry Recent Menu"]);
    let open_popups = ctx.debug_active_popup_rects();
    assert_eq!(open_popups[1].x, recent_row.x + recent_row.width);
    assert_eq!(open_popups[1].y, recent_row.y);

    // Both popup levels use the shared frame and place their compact rows directly inside it. Root
    // padding must not create a second inset around either a top-level menu or a recursive submenu.
    let popup_rows = ctx.debug_active_menu_row_rects();
    // Context already owns an atlas-bound Style; use its actual frame metric rather than creating
    // a disconnected resource-bearing value solely to read one scalar.
    let border = ctx.style().frame_insets().left;
    for (popup, rows) in open_popups.iter().zip(&popup_rows) {
        let first = rows.first().expect("each declared test menu must contain a row");
        let last = rows.last().unwrap();
        assert_eq!(first.x, popup.x + border);
        assert_eq!(first.y, popup.y + border);
        assert_eq!(first.width + border * 2, popup.width);
        assert_eq!(last.y + last.height + border, popup.y + popup.height);
    }

    // Move the owner programmatically while both levels remain open. A fresh layout must translate
    // headings, submenu rows, and both popup surfaces by the same delta while preserving the two
    // edge equations above.
    let delta = crate::vec2(37, 29);
    let original_window = ctx.debug_root_rect(root.id()).unwrap();
    ctx.ui()
        .set_window_rect(
            &root,
            Recti::new(
                original_window.x + delta.x,
                original_window.y + delta.y,
                original_window.width,
                original_window.height,
            ),
        )
        .unwrap();
    ctx.update_and_render_ui();

    let moved_anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    let moved_file_heading = moved_anchors[0].unwrap();
    let moved_recent_row = moved_anchors[1].unwrap();
    let moved_popups = ctx.debug_active_popup_rects();
    assert_eq!(
        (moved_file_heading.x, moved_file_heading.y),
        (file_heading.x + delta.x, file_heading.y + delta.y)
    );
    assert_eq!((moved_recent_row.x, moved_recent_row.y), (recent_row.x + delta.x, recent_row.y + delta.y));
    assert_eq!((moved_popups[0].x, moved_popups[0].y), (file_popup.x + delta.x, file_popup.y + delta.y));
    assert_eq!((moved_popups[1].x, moved_popups[1].y), (open_popups[1].x + delta.x, open_popups[1].y + delta.y));
    assert_eq!(moved_popups[0].x, moved_file_heading.x);
    assert_eq!(moved_popups[0].y, moved_file_heading.y + moved_file_heading.height);
    assert_eq!(moved_popups[1].x, moved_recent_row.x + moved_recent_row.width);
    assert_eq!(moved_popups[1].y, moved_recent_row.y);
}

/// Verifies that live marker-role changes remeasure only their owning menu surface.
#[test]
fn changing_item_marker_role_reflows_parent_and_reanchors_open_child() {
    // Keep the mutable item in the parent while the child owns an unrelated fixed-size item. The
    // application retains only the weak handle needed to change the parent's shared marker gutter.
    let (item, direct_item) = MenuItem::create(MenuItemParameters::new("Direct action"));
    let (_, child_item) = MenuItem::create(MenuItemParameters::new("Child action"));
    let menu_bar = MenuBar::new([Menu::new("File").item(direct_item).submenu(Menu::new("Child").item(child_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("live marker geometry", rect(24, 20, 220, 150), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Open both levels and retain their unmarked baseline. The child begins exactly at the parent
    // menu surface's right edge inside its one-pixel popup frame, which distinguishes resizing
    // from stale anchoring without conflating content geometry with window chrome.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let child_trigger = ctx.debug_active_menu_row_rects()[0][1];
    click_rect(&mut ctx, child_trigger);
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["live marker geometry File Menu", "live marker geometry Child Menu"]
    );
    let baseline_popups = ctx.debug_active_popup_rects();
    let baseline_rows = ctx.debug_active_menu_row_rects();
    // Recti intentionally has no equality implementation. Compare compact scalar snapshots when a
    // phase requires exact geometry rather than one relational edge assertion.
    let popup_geometry = |rects: &[Recti]| rects.iter().map(|rect| (rect.x, rect.y, rect.width, rect.height)).collect::<Vec<_>>();
    let row_geometry = |surfaces: &[Vec<Recti>]| {
        surfaces
            .iter()
            .map(|rows| rows.iter().map(|rect| (rect.x, rect.y, rect.width, rect.height)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    assert_eq!(baseline_popups[1].x, baseline_rows[0][1].x + baseline_rows[0][1].width);
    assert_eq!(baseline_popups[1].y, baseline_rows[0][1].y);

    // Introducing an unchecked role must still allocate the shared marker gutter. A normal update
    // remeasures the parent, widens every parent row, and repositions the already-open child from
    // the freshly committed submenu slot without altering the child's own intrinsic dimensions.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::Checked(false);
    ctx.update_and_render_ui();
    let unchecked_popups = ctx.debug_active_popup_rects();
    let unchecked_rows = ctx.debug_active_menu_row_rects();
    assert!(unchecked_popups[0].width > baseline_popups[0].width);
    assert_eq!(unchecked_popups[0].height, baseline_popups[0].height);
    assert!(unchecked_rows[0][0].width > baseline_rows[0][0].width);
    assert!(unchecked_rows[0][1].width > baseline_rows[0][1].width);
    assert_eq!(unchecked_popups[1].x, unchecked_rows[0][1].x + unchecked_rows[0][1].width);
    assert_eq!(unchecked_popups[1].y, unchecked_rows[0][1].y);
    assert_eq!(
        (unchecked_popups[1].width, unchecked_popups[1].height),
        (baseline_popups[1].width, baseline_popups[1].height)
    );

    // Toggling only the boolean paints a check but preserves its role, so no geometry on either
    // surface may move. This guards against measuring marker visibility instead of marker role.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::Checked(true);
    ctx.update_and_render_ui();
    assert_eq!(popup_geometry(&ctx.debug_active_popup_rects()), popup_geometry(&unchecked_popups));
    assert_eq!(row_geometry(&ctx.debug_active_menu_row_rects()), row_geometry(&unchecked_rows));

    // Removing the role releases the gutter. The parent and child must return to their exact
    // baseline rectangles while the open path remains intact and the child keeps its own size.
    ctx.ui().menu_item_mut(&item).unwrap().mark = MenuItemMark::None;
    ctx.update_and_render_ui();
    assert_eq!(popup_geometry(&ctx.debug_active_popup_rects()), popup_geometry(&baseline_popups));
    assert_eq!(row_geometry(&ctx.debug_active_menu_row_rects()), row_geometry(&baseline_rows));
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["live marker geometry File Menu", "live marker geometry Child Menu"]
    );
}

/// Verifies that a heading toggles its own menu and switches directly to a sibling heading.
#[test]
fn menu_heading_clicks_toggle_and_switch_the_single_active_popup_path() {
    // Each top-level menu owns one concrete item so both popup surfaces have nonzero geometry.
    let (_, new_item) = MenuItem::create(MenuItemParameters::new("New"));
    let (_, undo_item) = MenuItem::create(MenuItemParameters::new("Undo"));
    let menu_bar = MenuBar::new([Menu::new("File").item(new_item), Menu::new("Edit").item(undo_item)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu switching", rect(15, 20, 180, 120), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();
    let anchors = ctx.debug_menu_anchor_rects(root.id()).unwrap();
    let file_heading = anchors[0].unwrap();
    let edit_heading = anchors[1].unwrap();

    // Repeating the same heading press closes the existing top-level path instead of reopening it
    // after outside-dismissal policy runs for that press.
    click_rect(&mut ctx, file_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu switching File Menu"]);
    click_rect(&mut ctx, file_heading);
    assert!(ctx.debug_active_popup_names().is_empty());

    // A different heading replaces the path in one input transaction, and its popup uses that new
    // heading's exact below-edge relationship. It likewise toggles closed on repetition.
    click_rect(&mut ctx, file_heading);
    click_rect(&mut ctx, edit_heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu switching Edit Menu"]);
    let edit_popup = ctx.debug_active_popup_rects()[0];
    assert_eq!((edit_popup.x, edit_popup.y), (edit_heading.x, edit_heading.y + edit_heading.height));
    click_rect(&mut ctx, edit_heading);
    assert!(ctx.debug_active_popup_names().is_empty());
}

/// Verifies that choosing a sibling submenu removes every deeper popup from the old branch.
#[test]
fn opening_sibling_submenu_replaces_the_complete_descendant_suffix() {
    // Alpha owns a second descendant level so replacing it exercises more than the immediate child.
    // Concrete leaf items keep both terminal popup surfaces measurable without contributing row
    // nodes to the retained tree.
    let (_, alpha_leaf) = MenuItem::create(MenuItemParameters::new("Alpha leaf"));
    let (_, beta_leaf) = MenuItem::create(MenuItemParameters::new("Beta leaf"));
    let menu_bar = MenuBar::new([Menu::new("File")
        .submenu(Menu::new("Alpha").submenu(Menu::new("Deep").item(alpha_leaf)))
        .submenu(Menu::new("Beta").item(beta_leaf))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("submenu switching", rect(20, 20, 220, 150), empty_content()).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Open File, then follow Alpha into Deep. Row rectangles come from the active compact menu
    // surfaces in parent-first order, while each inner vector preserves declaration order.
    let file_heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, file_heading);
    let alpha_row = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, alpha_row);
    let deep_row = ctx.debug_active_menu_row_rects()[1][0];
    click_rect(&mut ctx, deep_row);
    assert_eq!(
        ctx.debug_active_popup_names(),
        ["submenu switching File Menu", "submenu switching Alpha Menu", "submenu switching Deep Menu",]
    );

    // Beta is a sibling of Alpha in the still-open File surface. Its press must replace both Alpha
    // and Deep atomically, leaving the common File prefix followed only by the new Beta branch.
    let beta_row = ctx.debug_active_menu_row_rects()[0][1];
    click_rect(&mut ctx, beta_row);
    assert_eq!(ctx.debug_active_popup_names(), ["submenu switching File Menu", "submenu switching Beta Menu"]);
    assert_eq!(ctx.debug_active_menu_row_rects().len(), 2);
}

/// Verifies that pointer menus suspend application key delivery and restore its retained focus.
#[test]
fn menu_pointer_scope_suspends_and_restores_preexisting_application_keyboard_focus() {
    // The application body is an ordinary persistent keyboard target. Menu surfaces are passive,
    // so their independent pointer capture must never replace this retained identity.
    let (probe, body) = OrderedProbe::create(WidgetOption::NONE);
    let body_id = body.id();
    let (_, invoke_item) = MenuItem::create(MenuItemParameters::new("Invoke"));
    let menu_bar = MenuBar::new([Menu::new("Actions").submenu(Menu::new("More").item(invoke_item))]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("menu focus", rect(20, 20, 220, 150), body).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Establish application focus before any menu becomes visible. Opening the bar starts a menu
    // keyboard scope, so text must not edit the retained application target behind that menu.
    let body_rect = ctx.debug_root_node_rect(root.id(), body_id).unwrap();
    click_rect(&mut ctx, body_rect);
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    ctx.text("after heading");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["menu focus Actions Menu"]);

    // Opening a nested popup changes the current menu surface without changing the suspended
    // application focus identity. Text remains manager-owned because type-ahead is outside the MVP.
    let submenu_row = ctx.debug_active_menu_row_rects()[0][0];
    click_rect(&mut ctx, submenu_row);
    ctx.text("after submenu");
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["menu focus Actions Menu", "menu focus More Menu"]);

    // Invoking the leaf dismisses the complete popup path during MouseDown. The following text event
    // proves that application focus resumes after the swallowed release tail completes.
    let item_row = ctx.debug_active_menu_row_rects()[1][0];
    click_rect(&mut ctx, item_row);
    ctx.text("after item");
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text"]));
}

/// Verifies the complete Windows-style keyboard path through a nested declarative menu.
#[test]
fn f10_arrows_escape_and_enter_navigate_nested_menus_and_restore_application_focus() {
    let (probe, body) = OrderedProbe::create(WidgetOption::NONE);
    let body_id = body.id();
    let (_, disabled_item) = MenuItem::create(MenuItemParameters::new("Disabled").disabled());
    let (recent, recent_item) = MenuItem::create(MenuItemParameters::new("Recent document"));
    let (_, open_item) = MenuItem::create(MenuItemParameters::new("Open"));
    let (_, undo_item) = MenuItem::create(MenuItemParameters::new("Undo"));
    let menu_bar = MenuBar::new([
        Menu::new("File")
            .item(disabled_item)
            .separator()
            .item(open_item)
            .submenu(Menu::new("Recent").item(recent_item)),
        Menu::new("Edit").item(undo_item),
    ]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("keyboard menus", rect(20, 20, 240, 160), body).menu_bar(menu_bar));
    ctx.update_and_render_ui();
    let mut recent_submissions = 0;
    let mut dispatcher = event_counter(recent.submitted());

    // Preserve an ordinary application target before F10 transfers key ownership to the bar.
    let body_rect = ctx.debug_root_node_rect(root.id(), body_id).unwrap();
    click_rect(&mut ctx, body_rect);
    ctx.key(KeyEvent::pressed(Key::Function(10), Modifiers::NONE));
    ctx.text("blocked");
    ctx.key(KeyEvent::pressed(Key::ArrowRight, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["keyboard menus Edit Menu"]);
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));

    // Window cycling is itself a manager command, but an open intrinsic menu is the narrower
    // keyboard scope. It consumes the complete chord without closing or leaving its active branch.
    ctx.key(KeyEvent::pressed(Key::Function(6), Modifiers::CTRL));
    ctx.key(KeyEvent::released(Key::Function(6), Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["keyboard menus Edit Menu"]);

    // Escape returns from the top-level popup to its heading. Move left to File, open it, skip the
    // disabled item and separator, then descend from Open to the Recent branch.
    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowLeft, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowRight, Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["keyboard menus File Menu", "keyboard menus Recent Menu"]);

    // Enter invokes the selected nested item, queues its existing typed event, closes the whole
    // path, and releases the menu scope. The application's earlier focus then receives text again.
    ctx.key(KeyEvent::pressed(Key::Enter, Modifiers::NONE));
    ctx.text("restored");
    ctx.update_and_render_ui();
    assert!(dispatcher.dispatch(&mut recent_submissions));
    assert_eq!(recent_submissions, 1);
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text"]));

    // An unchorded Alt tap enters the first heading. After opening and backing out to the bar, a
    // second tap exits the scope and again exposes the retained application focus.
    ctx.key(KeyEvent::pressed(Key::Alt, Modifiers::ALT));
    ctx.key(KeyEvent::released(Key::Alt, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::ArrowDown, Modifiers::NONE));
    ctx.update_and_render_ui();
    assert_eq!(ctx.debug_active_popup_names(), ["keyboard menus File Menu"]);

    ctx.key(KeyEvent::pressed(Key::Escape, Modifiers::NONE));
    ctx.key(KeyEvent::pressed(Key::Alt, Modifiers::ALT));
    ctx.key(KeyEvent::released(Key::Alt, Modifiers::NONE));
    ctx.text("restored-again");
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text", "text"]));
}

/// Verifies that removing a menu's owning window from traversal cannot leave a stale key scope.
#[test]
fn hiding_keyboard_menu_owner_releases_keys_to_the_next_eligible_focused_window() {
    let (probe, fallback_body) = OrderedProbe::create(WidgetOption::NONE);
    let fallback_id = fallback_body.id();
    let (_, action) = MenuItem::create(MenuItemParameters::new("Action"));
    let mut ctx = context();
    let fallback = ctx.ui().create_window(Window::new("fallback", rect(10, 10, 120, 90), fallback_body));
    let menu_owner = ctx
        .ui()
        .create_window(Window::new("menu owner", rect(150, 10, 140, 90), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(action)])));
    ctx.update_and_render_ui();

    // Retain focus in the lower window, then enter the other window's menu through its heading.
    let fallback_rect = ctx.debug_root_node_rect(fallback.id(), fallback_id).unwrap();
    click_rect(&mut ctx, fallback_rect);
    let heading = ctx.debug_menu_anchor_rects(menu_owner.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    assert_eq!(ctx.debug_active_popup_names(), ["menu owner File Menu"]);

    // Hiding the owner closes its path and clears manager selection. With no stale scope left to
    // consume text, the next eligible window's still-retained focus receives it immediately.
    ctx.ui().set_window_visible(&menu_owner, false).unwrap();
    ctx.text("fallback");
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(probe.try_read(|state| state.events.clone()), Some(vec!["down", "up", "text"]));
}

/// Verifies direct item events, dispatch ordering, and disabled-row menu policy together.
#[test]
fn menu_items_dispatch_directly_after_close_while_disabled_items_leave_the_menu_open() {
    /// Application state observed by the two concrete menu-item event subscriptions.
    #[derive(Default)]
    struct Model {
        /// Number of enabled-item submissions delivered by its native typed port.
        enabled_submissions: usize,
        /// Number of disabled-item submissions, expected to remain zero.
        disabled_submissions: usize,
        /// Whether enabled dispatch observed manager policy's already-closed popup path.
        enabled_observed_closed_menu: bool,
    }

    impl Model {
        /// Records the enabled item and inspects ordering through the safe event capability.
        fn enabled_submitted(&mut self, context: &mut Ui<'_>, _: &MenuItemSubmitted) {
            // The surface records an invocation and manager policy closes the path before the
            // item's typed event reaches application dispatch. The handler therefore sees no popup
            // without issuing a separate close command.
            self.enabled_submissions += 1;
            self.enabled_observed_closed_menu = context.debug_active_popup_names().is_empty();
        }

        /// Records any erroneous event emitted by the disabled concrete item.
        fn disabled_submitted(&mut self, _context: &mut Ui<'_>, _: &MenuItemSubmitted) {
            // The surface routes the row but resolves its disabled item to no invocation action.
            self.disabled_submissions += 1;
        }
    }

    // Each uniquely owned value moves into the compact menu data, while its handle remains with the
    // test so subscriptions can address the item's concrete event source after that move.
    let (enabled, enabled_item) = MenuItem::create(MenuItemParameters::new("Run"));
    let (disabled, disabled_item) = MenuItem::create(MenuItemParameters::new("Unavailable").disabled());
    let menu_bar = MenuBar::new([Menu::new("Actions").item(enabled_item).item(disabled_item)]);
    let dimensions = Dimensioni::new(320, 240);
    let mut ctx: Context<NoopRenderer, Model> = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions);
    let root = ctx
        .ui()
        .create_window(Window::new("menu events", rect(20, 25, 180, 120), empty_content()).menu_bar(menu_bar));
    ctx.subscribe_context(enabled.submitted(), Model::enabled_submitted).unwrap();
    ctx.subscribe_context(disabled.submitted(), Model::disabled_submitted).unwrap();
    let mut model = Model::default();
    ctx.update_ui_state(dimensions, &mut model);

    // Open Actions and press its enabled item. The item's own port fires once, and the handler sees
    // the menu closed in the same update transaction.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    let heading_point = crate::vec2(heading.x + heading.width / 2, heading.y + heading.height / 2);
    ctx.mousedown(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    // Rows no longer have RuntimeNodeIds. The test-only accessor exposes the same rectangles used
    // by the active popup surface for hit testing, indexed in declaration order.
    let enabled_rect = ctx.debug_active_menu_row_rects()[0][0];
    let enabled_point = crate::vec2(enabled_rect.x + enabled_rect.width / 2, enabled_rect.y + enabled_rect.height / 2);
    ctx.mousedown(enabled_point.x, enabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(enabled_point.x, enabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    assert_eq!(model.enabled_submissions, 1);
    assert!(model.enabled_observed_closed_menu);
    assert!(ctx.debug_active_popup_names().is_empty());

    // Reopen Actions and press the disabled row. The compact surface produces no invocation action,
    // so neither a typed event nor the manager's close policy runs and the popup remains active.
    ctx.mousedown(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(heading_point.x, heading_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    let disabled_rect = ctx.debug_active_menu_row_rects()[0][1];
    let disabled_point = crate::vec2(disabled_rect.x + disabled_rect.width / 2, disabled_rect.y + disabled_rect.height / 2);
    ctx.mousedown(disabled_point.x, disabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    ctx.mouseup(disabled_point.x, disabled_point.y, MouseButton::LEFT);
    ctx.update_ui_state(dimensions, &mut model);
    assert_eq!(model.enabled_submissions, 1);
    assert_eq!(model.disabled_submissions, 0);
    assert_eq!(ctx.debug_active_popup_names(), ["menu events Actions Menu"]);
}

/// Verifies that closing a menu cannot redirect the remainder of its pointer gesture.
#[test]
fn menu_item_dismissal_swallows_the_invalidated_popup_capture_tail() {
    // Place an event-recording application surface directly beneath the menu popup. This makes a
    // stale drag or release observable instead of relying on a Button, which would usually ignore
    // an unmatched release and conceal the cross-surface routing error.
    let (body_state, body) = OrderedProbe::create(WidgetOption::NONE);
    let (_, invoke_item) = MenuItem::create(MenuItemParameters::new("Invoke"));
    let menu_bar = MenuBar::new([Menu::new("Actions").item(invoke_item)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("capture tail", rect(20, 25, 220, 160), body).menu_bar(menu_bar));
    ctx.update_and_render_ui();

    // Opening the popup completes the heading's independent captured gesture. Pressing its sole
    // enabled row then closes the popup during MouseDown, while that now-inactive surface still
    // owns the physical gesture that began the invocation.
    let heading = ctx.debug_menu_anchor_rects(root.id()).unwrap()[0].unwrap();
    click_rect(&mut ctx, heading);
    let row = ctx.debug_active_menu_row_rects()[0][0];
    let point = crate::vec2(row.x + row.width / 2, row.y + row.height / 2);
    ctx.mousedown(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert!(ctx.debug_active_popup_names().is_empty());
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    // Neither a drag nor the final release may fall through to the newly revealed body. The
    // suppression ends with that release, so the following fresh click must route normally.
    ctx.mousemove(point.x + 1, point.y + 1);
    ctx.update_and_render_ui();
    ctx.mouseup(point.x + 1, point.y + 1, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(Vec::new()));

    ctx.mousedown(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    ctx.mouseup(point.x, point.y, MouseButton::LEFT);
    ctx.update_and_render_ui();
    assert_eq!(body_state.try_read(|state| state.events.clone()), Some(vec!["down", "up"]));
}

/// Verifies that ordinary programmatic popups retain explicit screen-space placement semantics.
#[test]
fn generic_screen_anchored_popup_does_not_follow_its_moved_owner() {
    // This popup deliberately uses the public generic API rather than a declarative menu relation.
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(20, 30, 120, 90), empty_content()));
    let popup = ctx.ui().create_popup(&source, "screen popup", desired_size_node(70, 45)).unwrap();
    let explicit_anchor = rect(210, 35, 1, 1);
    ctx.ui().show_popup_at(&popup, explicit_anchor).unwrap();
    ctx.update_and_render_ui();
    let before_move = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!((before_move.x, before_move.y), (explicit_anchor.x, explicit_anchor.y));

    // Moving the owner changes stacking ownership only; the exact screen anchor and auto-sized
    // popup extent remain unchanged across the following layout.
    ctx.ui().set_window_rect(&source, rect(85, 95, 120, 90)).unwrap();
    ctx.update_and_render_ui();
    let after_move = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!(
        (after_move.x, after_move.y, after_move.width, after_move.height),
        (before_move.x, before_move.y, before_move.width, before_move.height)
    );
    assert_eq!(ctx.debug_active_popup_names(), ["screen popup"]);
}

#[test]
fn popup_auto_size_tracks_content() {
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(200, 160, 100, 70), empty_content()));
    let (_, text) = crate::TextBlock::create(crate::TextBlockParameters::new("window content"));
    let popup = ctx.ui().create_popup(&source, "popup", text).unwrap();
    ctx.ui().show_popup(&popup).unwrap();
    ctx.update_and_render_ui();

    let content = ctx.debug_popup_content_size(&popup).unwrap();
    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(content.width > 0 && content.height > 0);
    assert!(outer.width >= content.width && outer.height >= content.height);
}

#[test]
fn auto_sized_window_includes_menu_bar_and_exposes_application_body_below_it() {
    let (_, action) = MenuItem::create(MenuItemParameters::new("Action"));
    let menu_bar = MenuBar::new([Menu::new("File").item(action)]);
    let mut ctx = context();
    let root = ctx
        .ui()
        .create_window(Window::new("auto menu", rect(20, 30, 1, 1), desired_size_node(90, 24)).menu_bar(menu_bar));
    ctx.ui()
        .set_window_options(
            &root,
            WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::AUTO_SIZE,
        )
        .unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_root_rect(root.id()).unwrap();
    let bar = ctx.debug_menu_bar_rect(root.id()).unwrap();
    let body = ctx.debug_root_body(root.id()).unwrap();
    assert!(bar.height > 0, "a non-empty menu must contribute intrinsic height");
    assert_eq!((bar.x, bar.width), (body.x, body.width), "the bar must fill the complete client width");
    assert_eq!(bar.y + bar.height, body.y, "application content must begin immediately below the bar");
    assert_eq!(body.height, 24, "auto height must retain the application's intrinsic extent below the bar");
    assert!(
        outer.height >= bar.height + body.height,
        "outer auto-size must include menu and application content"
    );
}

/// Verifies an unmounted item is never manager-addressable and its ID is not later reused.
#[test]
fn live_but_unmounted_menu_item_is_unknown_to_ui() {
    let (handle, item) = MenuItem::create(MenuItemParameters::new("Unmounted"));
    let destroyed_id = handle.id();
    let mut ctx = context();

    assert!(handle.submitted().is_alive(), "the declaration still owns its typed event source");
    assert!(matches!(ctx.ui().menu_item(&handle), Err(crate::MenuItemAccessError::UnknownItem)));
    assert!(matches!(ctx.ui().menu_item_mut(&handle), Err(crate::MenuItemAccessError::UnknownItem)));

    drop(item);
    assert!(!handle.submitted().is_alive(), "dropping the unmounted declaration expires its weak capability");

    // Release the final weak endpoint before creating an identical declaration. Its allocation may
    // be reused, but the stable item identity remains retired independently of that address.
    drop(handle);
    let (replacement, replacement_item) = MenuItem::create(MenuItemParameters::new("Unmounted"));
    assert_ne!(replacement.id(), destroyed_id);
    drop(replacement_item);
}

/// Verifies equal menu values in different Contexts remain isolated by stable identity.
#[test]
fn menu_item_handle_cannot_resolve_another_contexts_record() {
    let (first_handle, first_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let (second_handle, second_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let mut first = context();
    let mut second = context();
    first
        .ui()
        .create_window(Window::new("first", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(first_item)])));
    second
        .ui()
        .create_window(Window::new("second", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(second_item)])));

    // Process-wide stable IDs distinguish otherwise identical records across managers. Submission
    // endpoint allocations play no part in presentation access or cross-Context rejection.
    assert_eq!(first.ui().menu_item(&first_handle).unwrap().label, "Same");
    assert!(matches!(first.ui().menu_item(&second_handle), Err(crate::MenuItemAccessError::UnknownItem)));
    assert_eq!(second.ui().menu_item(&second_handle).unwrap().label, "Same");
    assert!(matches!(second.ui().menu_item(&first_handle), Err(crate::MenuItemAccessError::UnknownItem)));
}

/// Verifies equal menu values within one surface remain independently addressable.
#[test]
fn identical_menu_items_retain_independent_stable_identity() {
    let (first_handle, first_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let (second_handle, second_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let menu_bar = MenuBar::new([Menu::new("File").item(first_item).item(second_item)]);
    let mut ctx = context();
    ctx.ui()
        .create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()).menu_bar(menu_bar));

    // Mutating one ID-selected record must not depend on presentation equality, row position, or
    // either item's independently allocated submission endpoint.
    ctx.ui().menu_item_mut(&first_handle).unwrap().label = "First only".to_owned();
    assert_eq!(ctx.ui().menu_item(&first_handle).unwrap().label, "First only");
    assert_eq!(ctx.ui().menu_item(&second_handle).unwrap().label, "Same");
}

/// Verifies a destroyed mounted item's ID remains retired after releasing its weak endpoint.
#[test]
fn destroyed_mounted_menu_item_id_remains_retired_after_endpoint_release() {
    let (stale, stale_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let stale_id = stale.id();
    let mut ctx = context();
    let window = ctx
        .ui()
        .create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(stale_item)])));
    ctx.ui().destroy_window(&window).unwrap();

    // Destruction removes both authoritative state and the strong endpoint owner before any new
    // declaration exists, so stale access fails for identity rather than presentation mismatch.
    assert!(matches!(ctx.ui().menu_item(&stale), Err(crate::MenuItemAccessError::UnknownItem)));
    assert!(matches!(ctx.ui().menu_item_mut(&stale), Err(crate::MenuItemAccessError::UnknownItem)));
    assert!(!stale.submitted().is_alive());
    drop(window);
    drop(stale);

    let (replacement, replacement_item) = MenuItem::create(MenuItemParameters::new("Same"));
    assert_ne!(replacement.id(), stale_id);
    ctx.ui()
        .create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(replacement_item)])));
    assert_eq!(ctx.ui().menu_item(&replacement).unwrap().label, "Same");
}

/// Verifies a retained stale handle cannot address an identical replacement record.
#[test]
fn stale_menu_item_handle_cannot_select_an_identical_live_replacement() {
    let (stale, stale_item) = MenuItem::create(MenuItemParameters::new("Same"));
    let mut ctx = context();
    let old_window = ctx
        .ui()
        .create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(stale_item)])));
    ctx.ui().destroy_window(&old_window).unwrap();

    let (replacement, replacement_item) = MenuItem::create(MenuItemParameters::new("Same"));
    ctx.ui()
        .create_window(Window::new("window", rect(0, 0, 100, 80), empty_content()).menu_bar(MenuBar::new([Menu::new("File").item(replacement_item)])));

    // Keep the stale handle and its expired Weak endpoint alive while an identical record is
    // mounted. Stable ID membership must reject it and still select the replacement exactly.
    assert!(matches!(ctx.ui().menu_item(&stale), Err(crate::MenuItemAccessError::UnknownItem)));
    assert!(matches!(ctx.ui().menu_item_mut(&stale), Err(crate::MenuItemAccessError::UnknownItem)));
    assert_eq!(ctx.ui().menu_item(&replacement).unwrap().label, "Same");
}

#[test]
fn no_padding_option_makes_chromeless_root_content_edge_to_edge() {
    let mut ctx = context();
    let outer = rect(0, 0, 320, 240);
    let root = ctx.ui().create_window(Window::new("surface", outer, empty_content()));
    ctx.ui()
        .set_window_options(&root, WindowOption::NO_TITLE | WindowOption::NO_RESIZE | WindowOption::NO_PADDING)
        .unwrap();

    ctx.update_and_render_ui();

    let body = ctx.debug_root_body(root.id()).unwrap();
    assert_eq!((body.x, body.y, body.width, body.height), (outer.x, outer.y, outer.width, outer.height));
    let (title, close, resize) = ctx.debug_root_chrome(root.id()).unwrap();
    assert!(title.is_none() && close.is_none() && resize.is_none());
}

#[test]
fn auto_height_preserves_popup_width_and_stretches_column_items() {
    let mut item_ids = Vec::new();
    let items = ["Apple", "Banana", "Cherry", "Date"]
        .into_iter()
        .map(|label| {
            let (_, node) = ListItem::create(ListItemParameters::new(label));
            item_ids.push(node.id());
            node
        })
        .collect::<Vec<_>>();
    let (_, content) = Linear::create(LinearParameters::vertical(items));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "combo", content).unwrap();
    let anchor = rect(20, 30, 180, 1);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_HEIGHT | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, anchor).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert_eq!(outer.x, anchor.x);
    assert_eq!(outer.y, anchor.y);
    assert_eq!(outer.width, anchor.width, "AUTO_HEIGHT must retain the programmed width");
    assert!(outer.height > anchor.height, "popup height must still follow its items");
    let item_rects = item_ids
        .into_iter()
        .map(|item| ctx.debug_popup_node_rect(&popup, item).unwrap())
        .collect::<Vec<_>>();
    let first = item_rects[0];
    for item in item_rects {
        assert_eq!((item.x, item.width), (first.x, first.width));
    }
}

#[test]
fn auto_width_preserves_programmed_height() {
    let (_, item) = ListItem::create(ListItemParameters::new("intrinsic width"));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "horizontal", item).unwrap();
    let programmed = rect(20, 30, 1, 120);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, programmed).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(outer.width > programmed.width, "AUTO_WIDTH must derive width from content");
    assert_eq!(outer.height, programmed.height, "AUTO_WIDTH must retain the programmed height");
}

#[test]
fn auto_width_consumes_typed_measurement_invalidation_before_intrinsic_measurement() {
    let (text, content) = TextBlock::create(TextBlockParameters::new("x"));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "dynamic width", content).unwrap();
    let programmed = rect(20, 30, 1, 80);
    ctx.ui()
        .set_popup_options(
            &popup,
            WindowOption::FRAME | WindowOption::AUTO_WIDTH | WindowOption::NO_RESIZE | WindowOption::NO_TITLE,
        )
        .unwrap();
    ctx.ui().show_popup_at(&popup, programmed).unwrap();

    ctx.update_ui(Dimensioni::new(320, 240));
    let before = ctx.debug_popup_rect(&popup).unwrap();

    text.set_text("a substantially wider retained text block").unwrap();
    ctx.update_ui(Dimensioni::new(320, 240));
    let after = ctx.debug_popup_rect(&popup).unwrap();

    assert!(after.width > before.width, "auto-width measurement must observe the typed mutation");
    assert_eq!(after.height, programmed.height);
}

#[test]
fn auto_size_ignores_the_previous_rect_for_flexible_linear_and_grid_tracks() {
    let row_children = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("row {index}")))))
        .collect::<Vec<_>>();
    let (_, row) = Linear::create(
        LinearParameters::horizontal(row_children.into_iter().enumerate().map(|(index, child)| match index {
            0 => LinearItem::fixed(child, 18),
            1 => LinearItem::content(child),
            2 | 3 => LinearItem::flex(child, 1.0),
            _ => LinearItem::fixed(child, 4),
        }))
        .stretch_cross(),
    );
    let grid_items = (0..5)
        .map(|index| Node::widget(Custom::create(CustomParameters::new(format!("grid {index}")))))
        .collect::<Vec<_>>();
    let (_, grid) = Grid::create(GridParameters::new(
        [
            TrackSize::Fixed(18),
            TrackSize::Content,
            TrackSize::Flex(1.0),
            TrackSize::Flex(1.0),
            TrackSize::Fixed(4),
        ],
        [TrackSize::Flex(1.0)],
        grid_items,
    ));
    let (_, flexible_column) = Linear::create(LinearParameters::vertical([
        LinearItem::flex(Node::widget(Custom::create(CustomParameters::new("column first"))), 1.0),
        LinearItem::flex(Node::widget(Custom::create(CustomParameters::new("column second"))), 1.0),
    ]));
    let (_, content) = Linear::create(LinearParameters::vertical([row, grid, flexible_column]));
    let mut ctx = context();
    let source = ctx.ui().create_window(Window::new("source", rect(220, 170, 80, 50), empty_content()));
    let popup = ctx.ui().create_popup(&source, "intrinsic", content).unwrap();
    ctx.ui().show_popup_at(&popup, rect(20, 30, 2_000, 3_000)).unwrap();

    ctx.update_and_render_ui();

    let outer = ctx.debug_popup_rect(&popup).unwrap();
    assert!(
        outer.width < 1_000 && outer.height < 1_000,
        "AUTO_SIZE must derive both axes from content: {outer:?}"
    );
}

#[test]
fn body_input_falls_through_chrome_to_the_application_node() {
    let mut ctx = context();
    let (button, content) = button_content("button");
    let root = ctx.ui().create_window(Window::new("window", rect(20, 20, 140, 100), content));
    let mut button_dispatcher = event_counter(button);
    let mut root_dispatcher = event_counter(root.events());
    let mut button_submissions = 0;
    let mut root_submissions = 0;
    ctx.update_and_render_ui();
    let body = ctx.debug_root_body(root.id()).unwrap();

    ctx.mousedown(body.x + body.width / 2, body.y + body.height / 2, MouseButton::LEFT);
    ctx.update_and_render_ui();

    assert!(button_dispatcher.dispatch(&mut button_submissions));
    assert_eq!(button_submissions, 1);
    assert!(!root_dispatcher.dispatch(&mut root_submissions));
}

#[test]
fn post_tree_chrome_overlay_is_submitted_after_custom_descendant_rendering() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));
    let custom = ctx.register_custom_renderer(|frame, _args| frame.record_marker("application content")).unwrap();
    let content = Node::custom_render(Custom::create(CustomParameters::new("custom")), custom);
    let _root = ctx.ui().create_window(Window::new("window", rect(20, 20, 140, 100), content));

    ctx.update_and_render_ui();
    let events = log.snapshot();
    let marker = events
        .iter()
        .position(|event| matches!(event, RenderEvent::Marker(name) if name == "application content"))
        .expect("custom descendant marker missing");
    let final_atlas_quad = events
        .iter()
        .rposition(|event| matches!(event, RenderEvent::AtlasQuad(_)))
        .expect("root overlay quad missing");
    assert!(final_atlas_quad > marker);
}

#[test]
fn parent_overlay_is_recorded_after_custom_child_window_content() {
    let (backend, log) = recording_backend(test_atlas());
    let mut ctx = Context::new_test(backend, Dimensioni::new(320, 240));
    let parent_renderer = ctx.register_custom_renderer(|frame, _args| frame.record_marker("parent content")).unwrap();
    let child_renderer = ctx.register_custom_renderer(|frame, _args| frame.record_marker("child content")).unwrap();
    let parent_content = Node::custom_render(Custom::create(CustomParameters::new("parent custom")), parent_renderer);
    let child_content = Node::custom_render(Custom::create(CustomParameters::new("child custom")), child_renderer);
    let parent = ctx.ui().create_window(Window::new("parent", rect(20, 20, 180, 140), parent_content));
    ctx.ui()
        .create_child_window(&parent, Window::new("child", rect(40, 60, 100, 70), child_content))
        .unwrap();

    ctx.update_and_render_ui();
    let events = log.snapshot();
    let parent_content = events
        .iter()
        .position(|event| matches!(event, RenderEvent::Marker(name) if name == "parent content"))
        .expect("parent custom marker missing");
    let child_content = events
        .iter()
        .position(|event| matches!(event, RenderEvent::Marker(name) if name == "child content"))
        .expect("child custom marker missing");
    let final_parent_overlay = events
        .iter()
        .rposition(|event| matches!(event, RenderEvent::AtlasQuad(_)))
        .expect("parent chrome overlay quad missing");

    assert!(parent_content < child_content, "parent application content must record below child windows");
    assert!(child_content < final_parent_overlay, "parent chrome must record after child window content");
}
