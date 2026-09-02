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

use std::cell::Cell;

use microui_redux::render::{FrameError, FrameInfo, RendererBackend, RendererFrame, Vertex};
use microui_redux::retained::*;
use microui_redux::prelude::{
    Dimensioni, Ui, FileDialog, FileDialogRequest, FileDialogStatus, Menu, MenuBar, MenuItem, MenuItemHandle, MenuItemMark, MenuItemParameters,
    MenuItemSubmitted, Recti, TextBlock, TextBlockParameters, TypedWidgetHandle, Vec2i, Window,
};
use microui_redux::{
    color, rect, AtlasHandle, AtlasSource, AtlasUploadError, CaptionButtonSide, CharEntry, Constraints, Context, ControlRole, ControlState, Disclosure,
    DisclosureParameters, FontEntry, FontRef, FontRole, Grid, GridParameters, IconRef, IconRole, ImageError, Linear, LinearParameters, PointerState,
    ScrollArea, ScrollAreaOption, ScrollAreaParameters, Skin, SkinBundle, SourceFormat, SurfaceMutationError, TextureError, TextureId, WindowChromeSkin,
    WindowTitleAlignment,
};

struct TestBackend {
    atlas: AtlasHandle,
}

struct TestFrame;

impl RendererFrame for TestFrame {
    fn push_quad(&mut self, _vertices: [Vertex; 4]) {}
    fn push_triangle(&mut self, _vertices: [Vertex; 3]) {}
    fn flush(&mut self) {}
    fn draw_texture(&mut self, _id: TextureId, _vertices: [Vertex; 4]) {}
}

impl RendererBackend for TestBackend {
    type Frame<'a> = TestFrame;

    fn get_atlas(&self) -> AtlasHandle {
        self.atlas.clone()
    }

    fn replace_atlas(&mut self, atlas: AtlasHandle) -> Result<(), AtlasUploadError> {
        // This public-API fixture has no native renderer resource; the handle assignment is its
        // complete successful atlas transaction.
        self.atlas = atlas;
        Ok(())
    }

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(TestFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _pixels: &[u8]) -> Result<(), TextureError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

fn context_with_state<State: 'static>() -> Context<TestBackend, State> {
    // Build the smallest public atlas accepted by the downstream test renderer.
    let pixels = [255, 255, 255, 255];
    let icon_names = [
        "white",
        "close",
        "expand",
        "collapse",
        "check",
        "expand_down",
        "open_folder",
        "closed_folder",
        "file",
    ];
    let icons: Vec<_> = icon_names.iter().map(|name| (*name, Recti::new(0, 0, 1, 1))).collect();
    let entries = [(
        '_',
        CharEntry {
            offset: Vec2i::new(0, 0),
            advance: Vec2i::new(1, 0),
            rect: Recti::new(0, 0, 1, 1),
        },
    )];
    let font = FontEntry {
        line_size: 10,
        baseline: 8,
        font_size: 10,
        entries: &entries,
    };
    let fonts = [("body", font)];
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    };
    // Context infers the application state type from this helper's return value. Loading remains
    // explicit and fallible so this downstream fixture exercises the same validated public API an
    // application uses for embedded atlas metadata.
    let atlas = AtlasHandle::try_from(&source).expect("downstream retained-API atlas must satisfy the complete atlas contract");
    Context::new(TestBackend { atlas })
}

/// Verifies downstream applications receive the shared structured image error through Context.
#[test]
fn invalid_public_texture_upload_preserves_image_error_classification() {
    let mut context = context_with_state::<()>();

    let error = context.try_load_image_rgba(2, 2, &[0xFF; 4]).unwrap_err();

    assert!(matches!(
        error,
        TextureError::Image {
            source: ImageError::RawPixelLengthMismatch { expected: 16, actual: 4 },
        }
    ));
}

fn context() -> Context<TestBackend> {
    // Most downstream tests use the polling-only unit application state.
    context_with_state()
}

/// Verifies external code reads and replaces visuals through the typed control family.
#[test]
fn downstream_skin_visuals_are_concrete_and_exhaustive() {
    // A control state carries both focus ownership and pointer interaction. That concrete shape
    // makes impossible combinations such as a simultaneously disabled and pressed control
    // unrepresentable without requiring a universal, sparsely meaningful state table.
    let context = context();
    let mut skin = context.skin().clone();
    let focused = ControlState::Focused(PointerState::Normal);
    let pressed = ControlState::Focused(PointerState::Pressed);
    let original = skin.control(ControlRole::Button, focused);
    let mut changed = original;
    changed.content_color = color(17, 29, 43, 255);
    skin.set_control(ControlRole::Button, focused, changed);

    assert_eq!(ControlState::ALL.len(), ControlState::COUNT);
    assert_eq!(skin.control(ControlRole::Button, focused).content_color.r, 17);
    assert_eq!(skin.control(ControlRole::Button, pressed).content_color.r, original.content_color.r);
}

/// Verifies exact resource IDs and one atomic bundle remain the typed replacement path.
#[test]
fn downstream_resources_bundle_and_chrome_form_one_typed_runtime_value() {
    let mut context = context();
    let body = context.resource_catalog().font_ref("body").expect("body must be in the application catalog");
    let close = context.resource_catalog().icon_ref("close").expect("close must be in the application catalog");
    assert!(context.resource_catalog().font_ref("missing-font").is_none());
    assert!(context.resource_catalog().icon_ref("missing-icon").is_none());
    assert_eq!(body.resolve(context.skin(), &context.atlas()), context.atlas().font_id("body").unwrap());
    assert_eq!(close.resolve(context.skin(), &context.atlas()), context.atlas().icon_id("close").unwrap());

    // Build a separately allocated atlas/skin pair and describe manager-owned chrome with one
    // concrete data recipe. Installing the bundle uploads and publishes both halves together.
    let replacement_context = context_with_state::<()>();
    let replacement_atlas = replacement_context.atlas();
    let mut replacement_skin = Skin::from_atlas(&replacement_atlas);
    replacement_skin.metrics.padding = 19;
    replacement_skin.window_chrome = WindowChromeSkin::classic_mac(color(222, 222, 222, 255));
    let replacement = SkinBundle::new(replacement_atlas, replacement_skin);
    context
        .set_skin_bundle(&replacement)
        .expect("the downstream renderer accepts immutable atlas handles");

    assert_eq!(context.skin().metrics.padding, 19);
    assert_eq!(context.skin().window_chrome.title_alignment, WindowTitleAlignment::Centered);
    assert_eq!(context.skin().window_chrome.captions.close_side, CaptionButtonSide::Leading);
    // Semantic role references use IDs resolved once into the replacement skin and therefore share
    // the same direct typed lookup path without retaining resource names.
    let body_role = FontRef::role(FontRole::Body);
    let close_role = IconRef::role(IconRole::Close);
    assert_eq!(body_role.resolve(context.skin(), &context.atlas()), context.atlas().font_id("body").unwrap());
    assert_eq!(close_role.resolve(context.skin(), &context.atlas()), context.atlas().icon_id("close").unwrap());
}

struct FileDialogModel {
    dialog: FileDialog,
    /// Terminal result observed only through the component-owned typed source.
    completion: Option<FileDialogStatus>,
}

impl FileDialogModel {
    fn dialog_mut(state: &mut Self) -> &mut FileDialog {
        &mut state.dialog
    }

    fn file_dialog_completed(&mut self, event: &FileDialogCompleted) {
        self.completion = Some(event.status().clone());
    }
}

#[test]
fn downstream_file_dialog_completion_is_subscriber_driven_without_widget_access() {
    let mut context = context_with_state::<FileDialogModel>();
    let owner = context.ui().create_window(Window::new(
        "file-dialog owner",
        rect(0, 0, 1, 1),
        TextBlock::create(TextBlockParameters::new("")).1,
    ));
    let mut dialog = FileDialog::new(&mut context, &owner, FileDialogModel::dialog_mut);
    let completed = dialog.completed();
    context.subscribe(completed, FileDialogModel::file_dialog_completed).unwrap();
    dialog.open(&mut context.ui(), FileDialogRequest::default());
    let mut model = FileDialogModel { dialog, completion: None };

    // Explicit cancellation queues one event; the retained update delivers it without frame polling.
    assert!(model.dialog.cancel(&mut context.ui()));
    assert!(model.completion.is_none());
    context.update_ui_state(Dimensioni::new(320, 240), &mut model);
    assert_eq!(model.completion, Some(FileDialogStatus::Cancelled));
    assert!(!model.dialog.is_open());
}

/// Minimal downstream application state retaining only concrete menu-item state.
struct MenuModel {
    /// Live handle for an item whose enabled state changes.
    save: MenuItemHandle,
    /// Live handle for an item whose marker changes.
    word_wrap: MenuItemHandle,
    /// Item-specific application effects observed through concrete ports.
    invoked: Vec<&'static str>,
}

impl MenuModel {
    /// Handles the concrete Open item's event source.
    fn open_submitted(&mut self, _context: &mut Ui<'_>, _event: &MenuItemSubmitted) {
        self.invoked.push("Open");
    }
}

#[test]
fn downstream_window_owns_declarative_menu_and_live_concrete_items() {
    let mut context = context_with_state::<MenuModel>();
    let (open, open_item) = MenuItem::create(MenuItemParameters::new("Open").shortcut_hint("Ctrl+O"));
    // Menu commands project their typed submission ports; intrinsic menu policy closes the popup
    // before the application dispatcher invokes this handler.
    context.subscribe_context(open.submitted(), MenuModel::open_submitted).unwrap();
    let (save, save_item) = MenuItem::create(MenuItemParameters::new("Save").disabled());
    let (word_wrap, word_wrap_item) = MenuItem::create(MenuItemParameters::new("Word Wrap").checked(true));
    let body = TextBlock::create(TextBlockParameters::new("body")).1;
    let window = Window::new("document", rect(20, 20, 240, 160), body).menu_bar(MenuBar::new([
        Menu::new("File").item(open_item).item(save_item),
        Menu::new("View").item(word_wrap_item),
    ]));
    let window = context.ui().create_window(window);
    // The deliberately minimal downstream atlas contains no chrome icons, so this compile-contract
    // test removes title controls before committing the retained tree.
    context
        .ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    // Public state mutations address the concrete retained items directly. Text getters return
    // owned snapshots, while setters update the compact surface without exposing its private node.
    assert_eq!(context.ui().menu_item(&open).unwrap().label, "Open");
    assert_eq!(context.ui().menu_item(&open).unwrap().shortcut_hint.as_deref(), Some("Ctrl+O"));
    assert_eq!(context.ui().menu_item(&save).unwrap().shortcut_hint, None);
    {
        let mut ui = context.ui();
        let save = ui.menu_item_mut(&save).unwrap();
        save.label = "Save As".into();
        save.shortcut_hint = Some("Ctrl+Shift+S".into());
        save.enabled = true;
        ui.menu_item_mut(&word_wrap).unwrap().mark = MenuItemMark::Checked(false);
    }
    assert_eq!(context.ui().menu_item(&save).unwrap().label, "Save As");
    assert_eq!(context.ui().menu_item(&save).unwrap().shortcut_hint.as_deref(), Some("Ctrl+Shift+S"));
    assert!(context.ui().menu_item(&save).unwrap().enabled);
    assert_eq!(context.ui().menu_item(&word_wrap).unwrap().mark, MenuItemMark::Checked(false));

    let model = MenuModel { save, word_wrap, invoked: Vec::new() };
    // Context owns the complete window, including its compact menu data and private surfaces.
    // Concrete handles remain non-owning live views of item values moved into that declaration.
    assert!(window.events().is_alive());
    assert!(open.submitted().is_alive());
    assert!(model.save.submitted().is_alive());
    assert!(model.word_wrap.submitted().is_alive());
    assert!(model.invoked.is_empty());

    // Destroying the window releases the sole strong ownership chain for all menu items. The public
    // handles own only weak event endpoints, so they cannot keep a discarded window alive.
    context.ui().destroy_window(&window).unwrap();
    assert!(!window.events().is_alive());
    assert!(!open.submitted().is_alive());
    assert!(!model.save.submitted().is_alive());
    assert!(!model.word_wrap.submitted().is_alive());
}

/// Verifies downstream code can reuse one Menu declaration as an application-addressable popup.
#[test]
fn downstream_standalone_menu_uses_the_ordinary_popup_lifecycle() {
    let mut context = context_with_state::<()>();
    let owner = context
        .ui()
        .create_window(Window::new("owner", rect(20, 20, 180, 120), TextBlock::create(TextBlockParameters::new("body")).1));
    let (inspect, inspect_item) = MenuItem::create(MenuItemParameters::new("Inspect"));
    let (_, details_item) = MenuItem::create(MenuItemParameters::new("Details"));

    // The same recursive declaration accepted by MenuBar becomes a compact popup without exposing
    // row widgets or a second menu-specific lifecycle capability.
    let popup = context
        .ui()
        .create_menu_popup(
            &owner,
            Menu::new("Object actions")
                .item(inspect_item)
                .submenu(Menu::new("More").item(details_item)),
        )
        .unwrap();
    context.ui().show_popup_at(&popup, rect(40, 50, 1, 1)).unwrap();
    assert!(popup.events().is_alive());
    assert_eq!(context.ui().menu_item(&inspect).unwrap().label, "Inspect");

    // Popup and menu-item handles are both weak projections of the manager-owned declaration.
    // Destroying the owner releases the whole surface branch and expires both endpoints.
    context.ui().destroy_window(&owner).unwrap();
    assert!(!popup.events().is_alive());
    assert!(!inspect.submitted().is_alive());
}

#[test]
fn every_builtin_container_returns_a_typed_handle_and_completed_node() {
    let (row, row_node) = Linear::create(LinearParameters::horizontal(std::iter::empty::<Node>()));
    let (grid, grid_node) = Grid::create(GridParameters::new([], [], std::iter::empty::<Node>()));
    let (_, scroll_content) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
    let (scroll, scroll_node) = ScrollArea::create(ScrollAreaParameters::new(ScrollAreaOption::ENABLE_SCROLL, scroll_content));
    let (column, column_node) = Linear::create(LinearParameters::vertical([row_node, grid_node, scroll_node]));
    let (disclosure, root_node) = Disclosure::create(DisclosureParameters::header("group", true, [column_node]));

    assert_eq!(row.try_read(|state| state.len()), Some(Some(0)));
    assert_eq!(grid.try_read(|state| state.len()), Some(Some(0)));
    assert!(scroll.is_alive());
    assert_eq!(column.try_read(|state| state.len()), Some(Some(3)));
    assert_eq!(disclosure.try_read(|state| state.len()), Some(Some(1)));
    drop(root_node);
    assert!(!row.is_alive());
    assert!(!disclosure.is_alive());
}

/// Verifies that downstream code can construct and control a scrollbar without ScrollArea access.
#[test]
fn standalone_scrollbar_exposes_public_range_value_and_event_contracts() {
    // Construct one vertical bar with a non-zero range entirely through retained public exports.
    let (scrollbar, node) = Scrollbar::create(ScrollbarParameters::new(ScrollbarAxis::Vertical).range(40, 120, 15));
    assert!(scrollbar.is_alive());
    assert_eq!(scrollbar.offset(), Some(15));
    assert_eq!(scrollbar.try_read(Scrollbar::axis), Some(ScrollbarAxis::Vertical));
    let _changed = scrollbar.changed();

    // Programmatic range changes clamp the retained value but do not require a composite owner.
    scrollbar.set_lengths(40, 20).unwrap();
    assert_eq!(scrollbar.offset(), Some(0));

    // The returned node remains the sole strong runtime owner of the standalone widget.
    drop(node);
    assert!(!scrollbar.is_alive());
}

struct ExternalContainer {
    measure_calls: Cell<usize>,
    layout_calls: Cell<usize>,
    allocated_child: Cell<Option<Dimensioni>>,
    options: WidgetOption,
}

impl ContainerWidget for ExternalContainer {
    fn measure(&self, ctx: &mut MeasureCtx<'_>, constraints: Constraints) -> Dimensioni {
        self.measure_calls.set(self.measure_calls.get() + 1);
        ctx.measure_child(0, constraints).unwrap_or_else(|| Dimensioni::new(20, 20))
    }

    fn place(&mut self, ctx: &mut ContainerLayoutCtx<'_>, children: &mut Children, rect: Recti) {
        self.layout_calls.set(self.layout_calls.get() + 1);
        assert!(ctx.layout_child(children, usize::MAX, rect).is_none());
        if !children.is_empty() {
            self.allocated_child.set(ctx.layout_child(children, 0, rect));
        }
    }
}

impl Widget for ExternalContainer {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}

    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

fn external_container(children: impl IntoIterator<Item = Node>) -> (TypedWidgetHandle<ExternalContainer>, Container) {
    Container::new(
        ExternalContainer {
            measure_calls: Cell::new(0),
            layout_calls: Cell::new(0),
            allocated_child: Cell::new(None),
            options: WidgetOption::NO_INTERACT,
        },
        children,
    )
}

struct ExternalLeaf {
    options: WidgetOption,
}

impl ExternalLeaf {
    fn create() -> (TypedWidgetHandle<Self>, Node) {
        Node::typed_widget(Self { options: WidgetOption::NONE })
    }
}

impl Widget for ExternalLeaf {
    fn widget_opt(&self) -> &WidgetOption {
        &self.options
    }

    fn update(&mut self, _ctx: &mut WidgetUpdateCtx<'_>, _input: Option<&UiInputEvent>) {}
    fn paint(&mut self, _ctx: &mut WidgetPaintCtx<'_>) {}
}

impl LeafWidget for ExternalLeaf {
    fn measure(&self, _style: &Skin, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(12, 9)
    }
}

#[test]
fn downstream_custom_container_measures_and_lays_out_through_public_scoped_apis() {
    let (child_state, child) = ExternalLeaf::create();
    let (state, runtime) = external_container([child]);
    let node = Node::container(runtime);
    let mut ctx = context();
    let window = ctx.ui().create_window(Window::new("external", rect(10, 20, 100, 80), node));
    ctx.ui()
        .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let frame = FrameInfo::try_new(Dimensioni::new(320, 240), color(0, 0, 0, 255)).unwrap();

    ctx.update_ui(Dimensioni::new(320, 240));
    ctx.frame(frame).render_ui().unwrap();

    assert!(state.try_read(|state| state.measure_calls.get()).unwrap() > 0);
    assert!(state.try_read(|state| state.layout_calls.get()).unwrap() > 0);
    let allocated = state.try_read(|state| state.allocated_child.get()).flatten().unwrap();
    assert!(allocated.width > 12 && allocated.height > 9, "the parent rectangle must be authoritative");
    assert!(child_state.is_alive());
    ctx.ui().destroy_window(&window).unwrap();
    assert!(!state.is_alive());
    assert!(!child_state.is_alive());
}

#[test]
fn window_creation_and_lifecycle_need_no_numeric_application_identity() {
    let mut ctx = context();
    let content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let window = ctx.ui().create_window(Window::new("window", rect(10, 20, 100, 80), content));

    assert!(window.events().is_alive());
    ctx.ui().set_window_visible(&window, false).unwrap();
    assert!(window.events().is_alive(), "hiding retains the window and its application tree");
    ctx.ui().destroy_window(&window).unwrap();
    assert!(!window.events().is_alive());
    assert_eq!(ctx.ui().set_window_rect(&window, rect(0, 0, 1, 1)), Err(SurfaceMutationError::UnknownWindow));
}

#[test]
fn downstream_child_windows_use_their_parent_clip_and_layer_contract() {
    let mut ctx = context();
    let parent_content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let child_content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let parent = ctx
        .ui()
        .create_window(Window::new("desktop", rect(0, 0, 320, 240), parent_content).child_window_clip(ChildWindowClip::Content));
    ctx.ui().set_window_layer(&parent, MIN_LAYER).unwrap();
    let child = ctx
        .ui()
        .create_child_window(&parent, Window::new("tool", rect(20, 20, 100, 80), child_content))
        .unwrap();

    assert_eq!(ctx.ui().window_layer(&child), Ok(LayerBinding::Fixed(MIN_LAYER)));
    assert_eq!(ctx.ui().set_window_layer(&child, DEFAULT_LAYER), Err(SurfaceMutationError::ManagedLayer));
    ctx.ui().destroy_window(&parent).unwrap();
    assert_eq!(ctx.ui().window_layer(&child), Err(SurfaceMutationError::UnknownWindow));
}
