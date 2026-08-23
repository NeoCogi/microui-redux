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
    Dimensioni, EventContext, FileDialog, FileDialogRequest, FileDialogStatus, Menu, MenuGroup, MenuItem, MenuItemMark, MenuItemParameters, MenuItemSubmitted,
    MenuPanel, Recti, TextBlock, TextBlockParameters, TypedWidgetHandle, WindowMenu,
};
use microui_redux::{
    color, rect, AtlasHandle, AtlasSource, Constraints, Context, Disclosure, DisclosureParameters, FontEntry, Grid, GridParameters, Linear, LinearParameters,
    RootMutationError, ScrollArea, ScrollAreaOption, ScrollAreaParameters, SourceFormat, Style, TextureId,
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

    fn frame(&mut self, _info: FrameInfo) -> Result<Self::Frame<'_>, FrameError> {
        Ok(TestFrame)
    }

    fn create_texture(&mut self, _id: TextureId, _width: i32, _height: i32, _pixels: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn destroy_texture(&mut self, _id: TextureId) {}
}

fn context_with_state<State: 'static>() -> Context<TestBackend, State> {
    // Build the smallest public atlas accepted by the downstream test renderer.
    let pixels = [255, 255, 255, 255];
    let icons = [("white", Recti::new(0, 0, 1, 1))];
    let font = FontEntry {
        line_size: 10,
        baseline: 8,
        font_size: 10,
        entries: &[],
    };
    let fonts = [("default", font)];
    let source = AtlasSource {
        width: 1,
        height: 1,
        pixels: &pixels,
        icons: &icons,
        fonts: &fonts,
        format: SourceFormat::Raw,
    };
    // Context infers the application state type from this helper's return value.
    Context::new(TestBackend { atlas: AtlasHandle::from(&source) })
}

fn context() -> Context<TestBackend> {
    // Most downstream tests use the polling-only unit application state.
    context_with_state()
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
    let mut dialog = FileDialog::new(&mut context, FileDialogModel::dialog_mut);
    let completed = dialog.completed();
    context.subscribe(completed, FileDialogModel::file_dialog_completed).unwrap();
    dialog.open(&mut context, FileDialogRequest::default());
    let mut model = FileDialogModel { dialog, completion: None };

    // Explicit cancellation queues one event; the retained update delivers it without frame polling.
    assert!(model.dialog.cancel(&mut context));
    assert!(model.completion.is_none());
    context.update_ui_state(Dimensioni::new(320, 240), &mut model);
    assert_eq!(model.completion, Some(FileDialogStatus::Cancelled));
    assert!(!model.dialog.is_open());
}

/// Minimal downstream application state that owns one complete menu component.
struct MenuModel {
    /// Menu component found by its internal subscription accessor.
    menu: WindowMenu,
    /// Live handle for an item whose enabled state changes.
    save: TypedWidgetHandle<MenuItem>,
    /// Live handle for an item whose marker changes.
    word_wrap: TypedWidgetHandle<MenuItem>,
    /// Item-specific application effects observed through concrete ports.
    invoked: Vec<&'static str>,
}

impl MenuModel {
    /// Returns the stable component location required by `WindowMenu::create`.
    fn menu_mut(state: &mut Self) -> &mut WindowMenu {
        &mut state.menu
    }

    /// Handles the concrete Open item's event source.
    fn open_submitted(&mut self, _context: &mut EventContext<'_>, _event: &MenuItemSubmitted) {
        self.invoked.push("Open");
    }
}

#[test]
fn downstream_window_menu_exposes_concrete_items_roots_and_live_state() {
    let mut context = context_with_state::<MenuModel>();
    let (open, open_node) = MenuItem::create(MenuItemParameters::new("Open").shortcut_hint("Ctrl+O"));
    WindowMenu::register_item(&mut context, MenuModel::menu_mut, &open, MenuModel::open_submitted).unwrap();
    let (save, save_node) = MenuItem::create(MenuItemParameters::new("Save").disabled());
    let (word_wrap, word_wrap_node) = MenuItem::create(MenuItemParameters::new("Word Wrap").checked(true));
    let panel = MenuPanel::new([
        Menu::new("File", [MenuGroup::new([open_node, save_node])]),
        Menu::new("View", [MenuGroup::new([word_wrap_node])]),
    ]);
    let body = TextBlock::create(TextBlockParameters::new("body")).1;
    let menu = WindowMenu::create(&mut context, MenuModel::menu_mut, "document", rect(20, 20, 240, 160), panel, body);
    // The deliberately minimal downstream atlas contains no chrome icons, so this compile-contract
    // test removes title controls before committing the retained tree.
    context
        .set_root_options(menu.window().id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();

    // Public state mutations address the concrete retained items directly.
    assert_eq!(save.set_enabled(true), Some(()));
    assert_eq!(word_wrap.set_mark(MenuItemMark::Checked(false)), Some(()));
    assert_eq!(save.is_enabled(), Some(true));
    assert_eq!(word_wrap.mark(), Some(MenuItemMark::Checked(false)));

    // Both roots are ordinary Context-owned retained roots. Anchored placement accepts only the
    // component's popup, leaving the semantic component closed until its bar requests activation.
    let window_id = menu.window().id();
    let popup_id = menu.popup(0).unwrap().id();
    assert_eq!(context.show_popup_at(window_id, rect(40, 40, 100, 1)), Err(RootMutationError::NotPopup));
    context.show_popup_at(popup_id, rect(40, 40, 100, 1)).unwrap();
    context.set_root_visible(popup_id, false).unwrap();

    let model = MenuModel {
        menu,
        save,
        word_wrap,
        invoked: Vec::new(),
    };
    assert!(model.menu.window().widget().is_alive());
    assert!(model.menu.popup(0).unwrap().widget().is_alive());
    assert!(model.save.is_alive());
    assert!(model.word_wrap.is_alive());
    assert!(model.invoked.is_empty());
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
    fn measure(&self, _style: &Style, _atlas: &AtlasHandle, _constraints: Constraints) -> Dimensioni {
        Dimensioni::new(12, 9)
    }
}

#[test]
fn downstream_custom_container_measures_and_lays_out_through_public_scoped_apis() {
    let (child_state, child) = ExternalLeaf::create();
    let (state, runtime) = external_container([child]);
    let node = Node::container(runtime);
    let mut ctx = context();
    let root = ctx.create_window("external", rect(10, 20, 100, 80), node);
    ctx.set_root_options(root.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
        .unwrap();
    let frame = FrameInfo::try_new(Dimensioni::new(320, 240), color(0, 0, 0, 255)).unwrap();

    ctx.update_ui(Dimensioni::new(320, 240));
    ctx.frame(frame).render_ui().unwrap();

    assert!(state.try_read(|state| state.measure_calls.get()).unwrap() > 0);
    assert!(state.try_read(|state| state.layout_calls.get()).unwrap() > 0);
    let allocated = state.try_read(|state| state.allocated_child.get()).flatten().unwrap();
    assert!(allocated.width > 12 && allocated.height > 9, "the parent rectangle must be authoritative");
    assert!(child_state.is_alive());
    assert!(ctx.destroy_root(root.id()));
    assert!(!state.is_alive());
    assert!(!child_state.is_alive());
}

#[test]
fn root_creation_and_lifecycle_need_no_projection_or_generated_node_identity() {
    let mut ctx = context();
    let content = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>())).1;
    let root = ctx.create_window("root", rect(10, 20, 100, 80), content);
    let id = root.id();

    assert_eq!(root.widget().try_read(|widget| widget.is_visible()), Some(true));
    ctx.set_root_visible(id, false).unwrap();
    assert_eq!(root.widget().try_read(|widget| widget.is_visible()), Some(false));
    assert!(ctx.destroy_root(id));
    assert!(!root.widget().is_alive());
    assert_eq!(ctx.set_root_rect(id, rect(0, 0, 1, 1)), Err(RootMutationError::UnknownRoot));
}
