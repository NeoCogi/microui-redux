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
#[path = "./common/mod.rs"]
mod common;

#[cfg(any(
    all(feature = "example-glow", feature = "example-vulkan"),
    all(feature = "example-glow", feature = "example-wgpu"),
    all(feature = "example-vulkan", feature = "example-wgpu"),
))]
compile_error!("Enable only one of `example-glow`, `example-vulkan`, or `example-wgpu` for demo-full.");
#[cfg(not(any(feature = "example-glow", feature = "example-vulkan", feature = "example-wgpu")))]
compile_error!("Enable one of `example-glow`, `example-vulkan`, or `example-wgpu` to build demo-full.");

use common::{application::Application, application::BackendInitContext, atlas_assets, camera::Camera, obj_loader::Obj, polymesh::PolyMesh, view3d::View3D};
#[cfg(feature = "example-glow")]
use common::glow_renderer::{CustomRenderArea, GLRenderer as BackendRenderer, MeshBuffers, MeshSubmission, MeshVertex};
#[cfg(feature = "example-vulkan")]
use common::vulkan_renderer::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex, VulkanRenderer as BackendRenderer};
#[cfg(feature = "example-wgpu")]
use common::wgpu_renderer::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex, WgpuRenderer as BackendRenderer};
#[cfg(feature = "builder")]
use microui_redux::builder;
use microui_redux::*;
use rand::{rng, Rng};
use std::{
    cell::RefCell,
    f32::consts::PI,
    fs,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, RwLock},
    time::Instant,
};

#[repr(C)]
pub struct TriVertex {
    pub pos: Vec2f,
    pub color: Color4b,
}

const TRI_VERTS: [TriVertex; 3] = [
    TriVertex {
        pos: Vec2f { x: 0.0, y: -1.0 },
        color: Color4b { x: 0xff, y: 0x00, z: 0x00, w: 0xff },
    },
    TriVertex {
        pos: Vec2f { x: -1.0, y: 1.0 },
        color: Color4b { x: 0x00, y: 0xff, z: 0x00, w: 0xff },
    },
    TriVertex {
        pos: Vec2f { x: 1.0, y: 1.0 },
        color: Color4b { x: 0x00, y: 0x00, z: 0xff, w: 0xff },
    },
];

struct TriangleState {
    angle: f32,
}

struct SuzaneData {
    view_3d: View3D,
    mesh: MeshBuffers,
}

fn static_label(text: impl Into<String>) -> WidgetHandle<ListItem> {
    widget_handle(ListItem::with_opt(text, WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME))
}

struct State {
    renderer: RendererHandle<BackendRenderer>,
    bg: [Real; 3],
    bg_sliders: [WidgetHandle<Slider>; 3],
    style_color_sliders: [WidgetHandle<Slider>; 60],
    style_value_sliders: [WidgetHandle<Slider>; 5],
    logbuf: Rc<RefCell<String>>,
    logbuf_updated: bool,
    submit_buf: WidgetHandle<Textbox>,
    text_area: WidgetHandle<TextArea>,
    combo_state: WidgetHandle<Combo>,
    combo_items: [WidgetHandle<ListItem>; 4],
    style_color_labels: [WidgetHandle<ListItem>; 14],
    style_color_swatches: [WidgetHandle<ColorSwatch>; 14],
    style_metric_labels: [WidgetHandle<ListItem>; 5],
    stack_direction_labels: [WidgetHandle<ListItem>; 2],
    weight_labels: [WidgetHandle<ListItem>; 2],
    window_info_labels: [WidgetHandle<ListItem>; 3],
    window_info_values: [WidgetHandle<ListItem>; 3],
    test_button_labels: [WidgetHandle<ListItem>; 3],
    tree_labels: [WidgetHandle<ListItem>; 2],
    background_labels: [WidgetHandle<ListItem>; 3],
    style: Style,

    demo_window: Option<WindowHandle>,
    style_window: Option<WindowHandle>,
    log_window: Option<WindowHandle>,
    popup_window: Option<WindowHandle>,
    log_output: Option<ContainerHandle>,
    triangle_window: Option<WindowHandle>,
    suzane_window: Option<WindowHandle>,
    stack_direction_window: Option<WindowHandle>,
    weight_window: Option<WindowHandle>,
    dialog_window: Option<FileDialogState>,

    fps: f32,
    last_frame: Instant,

    window_header: WidgetHandle<Node>,
    test_buttons_header: WidgetHandle<Node>,
    background_header: WidgetHandle<Node>,
    tree_and_text_header: WidgetHandle<Node>,
    text_area_header: WidgetHandle<Node>,
    slot_header: WidgetHandle<Node>,
    combo_header: WidgetHandle<Node>,
    test1_tn: WidgetHandle<Node>,
    test1a_tn: WidgetHandle<Node>,
    test1b_tn: WidgetHandle<Node>,
    test2_tn: WidgetHandle<Node>,
    test3_tn: WidgetHandle<Node>,
    submit_button: WidgetHandle<Button>,
    log_text: WidgetHandle<TextBlock>,
    test_buttons: [WidgetHandle<Button>; 6],
    tree_buttons: [WidgetHandle<Button>; 6],
    popup_buttons: [WidgetHandle<Button>; 2],
    slot_buttons: [WidgetHandle<Button>; 4],
    stack_direction_buttons: [WidgetHandle<Button>; 6],
    weight_buttons: [WidgetHandle<Button>; 9],
    external_image_button: Option<WidgetHandle<Button>>,
    checkboxes: [WidgetHandle<Checkbox>; 3],
    open_popup: bool,
    open_dialog: bool,
    white_uv: Vec2f,
    triangle_data: Arc<RwLock<TriangleState>>,
    suzane_data: Arc<RwLock<SuzaneData>>,
    triangle_widget: WidgetHandle<Custom>,
    suzane_widget: WidgetHandle<Custom>,
    background_swatch: WidgetHandle<ColorSwatch>,
    style_tree: WidgetTree,
    log_tree: WidgetTree,
    triangle_tree: WidgetTree,
    suzane_tree: WidgetTree,
    stack_direction_tree: WidgetTree,
    weight_tree: WidgetTree,
    demo_tree: WidgetTree,
    combo_tree: WidgetTree,
    popup_tree: WidgetTree,
}

impl State {
    pub fn new(_backend: BackendInitContext, renderer: RendererHandle<BackendRenderer>, slots: Vec<SlotId>, ctx: &mut Context<BackendRenderer>) -> Self {
        #[cfg(any(feature = "builder", feature = "png_source"))]
        let image_texture = load_external_image_texture(ctx);
        #[cfg(not(any(feature = "builder", feature = "png_source")))]
        let image_texture = None;
        let white_uv = renderer.scope(|r| {
            let atlas = r.get_atlas();
            let rect = atlas.get_icon_rect(WHITE_ICON);
            let dim = atlas.get_texture_dimension();
            Vec2f::new(
                (rect.x as f32 + rect.width as f32 * 0.5) / dim.width as f32,
                (rect.y as f32 + rect.height as f32 * 0.5) / dim.height as f32,
            )
        });

        let triangle_data = Arc::new(RwLock::new(TriangleState { angle: 0.0 }));
        let suzane_path = demo_asset_path("assets/suzane.obj");
        let suzane_bytes = fs::read(&suzane_path).unwrap_or_else(|err| panic!("Failed to read {}: {err}", suzane_path.display()));
        let pm_suzane = Obj::from_byte_stream(suzane_bytes.as_slice())
            .unwrap_or_else(|err| panic!("Failed to parse {}: {err}", suzane_path.display()))
            .to_polymesh();
        let bounds = pm_suzane.calculate_bounding_box();
        let mesh_buffers = build_mesh_buffers(&pm_suzane);
        let view_3d = View3D::new(
            Camera::new(
                bounds.center(),
                bounds.max.length() * 3.0,
                Quat::identity(),
                PI / 4.0,
                1.0,
                0.1,
                bounds.max.length() * 10.0,
            ),
            Dimension::new(600, 600),
            bounds,
        );
        let suzane_data = Arc::new(RwLock::new(SuzaneData { view_3d, mesh: mesh_buffers }));

        let rng = Rc::new(RefCell::new(rng()));
        let green_paint: Rc<dyn Fn(usize, usize) -> Color4b> = Rc::new(|_x, _y| color4b(0x00, 0xFF, 0x00, 0xFF));
        let random_paint: Rc<dyn Fn(usize, usize) -> Color4b> = {
            let rng = rng.clone();
            Rc::new(move |_x, _y| {
                let mut rm = rng.borrow_mut();
                color4b(rm.random(), rm.random(), rm.random(), rm.random())
            })
        };
        let slot_buttons = [
            widget_handle(Button::with_image(
                "Slot 1",
                Some(Image::Slot(slots[0])),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_slot(
                "Slot 2 - Green",
                slots[1],
                green_paint,
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_image(
                "Slot 3",
                Some(Image::Slot(slots[2])),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
            widget_handle(Button::with_slot(
                "Slot 2 - Random",
                slots[1],
                random_paint,
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            )),
        ];
        let external_image_button = image_texture.map(|texture| {
            widget_handle(Button::with_image(
                "External Image",
                Some(Image::Texture(texture)),
                WidgetOption::NONE,
                WidgetFillOption::ALL,
            ))
        });
        let style_color_sliders = std::array::from_fn(|_| widget_handle(Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER)));
        let style_color_swatches = std::array::from_fn(|_| widget_handle(ColorSwatch::new(color(0, 0, 0, 0xFF))));
        let style_value_sliders = [
            widget_handle(Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
            widget_handle(Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER)),
        ];
        let bg_sliders = std::array::from_fn(|_| widget_handle(Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER)));
        let mut text_area =
            TextArea::new("This is a multi-line TextArea.\nYou can type, scroll, and resize the window.\n\nTry adding more lines to see the scrollbars.");
        text_area.wrap = TextWrap::Word;
        let mut state = Self {
            renderer,
            bg: [90.0, 95.0, 100.0],
            bg_sliders,
            style_color_sliders,
            style_value_sliders,
            logbuf: Rc::new(RefCell::new(String::new())),
            logbuf_updated: false,
            submit_buf: widget_handle(Textbox::new("")),
            text_area: widget_handle(text_area),
            combo_state: widget_handle(Combo::new(ctx.new_popup("Combo Box Popup"))),
            combo_items: [
                widget_handle(ListItem::new("Apple")),
                widget_handle(ListItem::new("Banana")),
                widget_handle(ListItem::new("Cherry")),
                widget_handle(ListItem::new("Date")),
            ],
            style_color_labels: [
                static_label("text"),
                static_label("border:"),
                static_label("windowbg:"),
                static_label("titlebg:"),
                static_label("titletext:"),
                static_label("panelbg:"),
                static_label("button:"),
                static_label("buttonhover:"),
                static_label("buttonfocus:"),
                static_label("base:"),
                static_label("basehover:"),
                static_label("basefocus:"),
                static_label("scrollbase:"),
                static_label("scrollthumb:"),
            ],
            style_color_swatches,
            style_metric_labels: [
                static_label("padding"),
                static_label("spacing"),
                static_label("title height"),
                static_label("thumb size"),
                static_label("scroll size"),
            ],
            stack_direction_labels: [static_label("Top -> Bottom"), static_label("Bottom -> Top")],
            weight_labels: [static_label("Row weights 1 : 2 : 3"), static_label("Grid weights rows 1 : 2")],
            window_info_labels: [static_label("Position:"), static_label("Size:"), static_label("FPS:")],
            window_info_values: [static_label(""), static_label(""), static_label("")],
            test_button_labels: [
                static_label("Test buttons 1:"),
                static_label("Test buttons 2:"),
                static_label("Test buttons 3:"),
            ],
            tree_labels: [static_label("Hello"), static_label("world")],
            background_labels: [static_label("Red:"), static_label("Green:"), static_label("Blue:")],
            style: Style::default(),
            demo_window: Some(ctx.new_window("Demo Window", rect(40, 40, 300, 450))),
            style_window: Some(ctx.new_window("Style Editor", rect(350, 250, 300, 240))),
            log_window: Some(ctx.new_window("Log Window", rect(350, 40, 300, 200))),
            popup_window: Some(ctx.new_popup("Test Popup")),
            log_output: Some(ctx.new_panel("Log Output")),
            triangle_window: Some(ctx.new_window("Triangle Window", rect(200, 100, 200, 200))),
            suzane_window: Some(ctx.new_window("Suzane Window", rect(220, 220, 300, 300))),
            stack_direction_window: Some(ctx.new_window("Stack Direction Demo", rect(530, 40, 280, 220))),
            weight_window: Some(ctx.new_window("Weight Demo", rect(530, 270, 280, 260))),
            dialog_window: Some(FileDialogState::new(ctx)),
            fps: 0.0,
            last_frame: Instant::now(),
            window_header: widget_handle(Node::header("Window Info", NodeStateValue::Closed)),
            test_buttons_header: widget_handle(Node::header("Test Buttons", NodeStateValue::Expanded)),
            background_header: widget_handle(Node::header("Background Color", NodeStateValue::Expanded)),
            tree_and_text_header: widget_handle(Node::header("Tree and Text", NodeStateValue::Expanded)),
            text_area_header: widget_handle(Node::header("TextArea", NodeStateValue::Expanded)),
            slot_header: widget_handle(Node::header("Slots", NodeStateValue::Expanded)),
            combo_header: widget_handle(Node::header("Combo Box", NodeStateValue::Expanded)),
            test1_tn: widget_handle(Node::tree("Test 1", NodeStateValue::Closed)),
            test1a_tn: widget_handle(Node::tree("Test 1a", NodeStateValue::Closed)),
            test1b_tn: widget_handle(Node::tree("Test 1b", NodeStateValue::Closed)),
            test2_tn: widget_handle(Node::tree("Test 2", NodeStateValue::Closed)),
            test3_tn: widget_handle(Node::tree("Test 3", NodeStateValue::Closed)),
            submit_button: widget_handle(Button::with_opt("Submit", WidgetOption::ALIGN_CENTER)),
            log_text: widget_handle(TextBlock::new("")),
            test_buttons: [
                widget_handle(Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Popup", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Dialog", WidgetOption::ALIGN_CENTER)),
            ],
            tree_buttons: [
                widget_handle(Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 5", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Button 6", WidgetOption::ALIGN_CENTER)),
            ],
            popup_buttons: [
                widget_handle(Button::with_opt("Hello", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("World", WidgetOption::ALIGN_CENTER)),
            ],
            slot_buttons,
            stack_direction_buttons: [
                widget_handle(Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER)),
            ],
            weight_buttons: [
                widget_handle(Button::with_opt("w1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("w2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("w3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g1", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g2", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g3", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g4", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g5", WidgetOption::ALIGN_CENTER)),
                widget_handle(Button::with_opt("g6", WidgetOption::ALIGN_CENTER)),
            ],
            external_image_button,
            checkboxes: [
                widget_handle(Checkbox::new("Checkbox 1", false)),
                widget_handle(Checkbox::new("Checkbox 2", true)),
                widget_handle(Checkbox::new("Checkbox 3", false)),
            ],
            open_popup: false,
            open_dialog: false,
            white_uv,
            triangle_data,
            suzane_data,
            triangle_widget: widget_handle(Custom::with_opt("Triangle", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::NONE)),
            suzane_widget: widget_handle(Custom::with_opt("Suzane", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::GRAB_SCROLL)),
            background_swatch: widget_handle(ColorSwatch::new(color(90, 95, 100, 0xFF))),
            style_tree: WidgetTree::default(),
            log_tree: WidgetTree::default(),
            triangle_tree: WidgetTree::default(),
            suzane_tree: WidgetTree::default(),
            stack_direction_tree: WidgetTree::default(),
            weight_tree: WidgetTree::default(),
            demo_tree: WidgetTree::default(),
            combo_tree: WidgetTree::default(),
            popup_tree: WidgetTree::default(),
        };
        state.rebuild_trees();
        state
    }

    fn write_log(&mut self, text: &str) {
        let mut logbuf = self.logbuf.borrow_mut();
        if !logbuf.is_empty() {
            logbuf.push('\n');
        }
        for c in text.chars() {
            logbuf.push(c);
        }
        self.logbuf_updated = true;
    }

    fn section(tree: &mut WidgetTreeBuilder, node: &WidgetHandle<Node>, f: impl FnOnce(&mut WidgetTreeBuilder)) {
        tree.header(node.clone(), f);
    }

    fn rebuild_trees(&mut self) {
        let style_color_labels = self.style_color_labels.clone();
        let style_color_sliders = self.style_color_sliders.clone();
        let style_color_swatches = self.style_color_swatches.clone();
        let style_metric_labels = self.style_metric_labels.clone();
        let style_value_sliders = self.style_value_sliders.clone();
        self.style_tree = WidgetTreeBuilder::build(move |tree| {
            let color_row = [
                SizePolicy::Fixed(80),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
                SizePolicy::Weight(1.0),
            ];
            let metrics_row = [SizePolicy::Fixed(80), SizePolicy::Remainder(0)];

            for ((label, sliders), swatch) in style_color_labels
                .iter()
                .zip(style_color_sliders.chunks_exact(4))
                .zip(style_color_swatches.iter())
            {
                tree.row(&color_row, SizePolicy::Auto, |tree| {
                    tree.widget(label.clone());
                    tree.widget(sliders[0].clone());
                    tree.widget(sliders[1].clone());
                    tree.widget(sliders[2].clone());
                    tree.widget(sliders[3].clone());
                    tree.widget(swatch.clone());
                });
            }

            for (label, slider) in style_metric_labels.iter().zip(style_value_sliders.iter()) {
                tree.row(&metrics_row, SizePolicy::Auto, |tree| {
                    tree.widget(label.clone());
                    tree.widget(slider.clone());
                });
            }
        });

        let log_output = self.log_output.clone().expect("log output panel missing");
        let log_text = self.log_text.clone();
        let submit_buf = self.submit_buf.clone();
        let submit_button = self.submit_button.clone();
        self.log_tree = WidgetTreeBuilder::build(move |tree| {
            let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(24), StackDirection::TopToBottom, |tree| {
                tree.container(log_output.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.widget(log_text.clone());
                });
            });
            tree.row(&submit_row, SizePolicy::Auto, |tree| {
                tree.widget(submit_buf.clone());
                tree.widget(submit_button.clone());
            });
        });

        let triangle_widget = self.triangle_widget.clone();
        let triangle_data = self.triangle_data.clone();
        let renderer = self.renderer.clone();
        let white_uv = self.white_uv;
        self.triangle_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                let triangle_data = triangle_data.clone();
                let renderer = renderer.clone();
                tree.custom_render(triangle_widget.clone(), move |_dim, cra| {
                    if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                        return;
                    }
                    let area = area_from_args(cra);
                    if let Ok(mut tri) = triangle_data.write() {
                        tri.angle = (tri.angle + 0.02) % (std::f32::consts::PI * 2.0);
                        let mut verts = build_triangle_vertices(area.rect, white_uv, tri.angle);
                        let mut renderer = renderer.clone();
                        renderer.scope_mut(move |vk| {
                            let verts_local = std::mem::take(&mut verts);
                            vk.enqueue_colored_vertices(area, verts_local);
                        });
                    }
                });
            });
        });

        let suzane_widget = self.suzane_widget.clone();
        let suzane_data = self.suzane_data.clone();
        let renderer = self.renderer.clone();
        self.suzane_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                let suzane_data = suzane_data.clone();
                let renderer = renderer.clone();
                tree.custom_render(suzane_widget.clone(), move |_dim, cra| {
                    if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                        return;
                    }
                    if let Ok(mut suzane) = suzane_data.write() {
                        suzane.view_3d.set_dimension(Dimensioni::new(cra.content_area.width, cra.content_area.height));
                        let _ = suzane.view_3d.update(cra.mouse_event);
                        if let Some(delta) = cra.scroll_delta {
                            let axis = if delta.y != 0 { delta.y } else { delta.x };
                            if axis != 0 {
                                suzane.view_3d.apply_scroll(axis as f32);
                            }
                        }
                        if !matches!(cra.mouse_event, MouseEvent::Drag { .. }) && cra.scroll_delta.is_none() {
                            let step = 20;
                            let mut delta = Vec2i::new(0, 0);
                            if cra.key_codes.is_left() {
                                delta.x -= step;
                            }
                            if cra.key_codes.is_right() {
                                delta.x += step;
                            }
                            if cra.key_codes.is_up() {
                                delta.y -= step;
                            }
                            if cra.key_codes.is_down() {
                                delta.y += step;
                            }
                            if delta.x != 0 || delta.y != 0 {
                                let center = Vec2i::new(cra.content_area.width / 2, cra.content_area.height / 2);
                                let curr = Vec2i::new(center.x + delta.x, center.y + delta.y);
                                suzane.view_3d.update(MouseEvent::Drag { prev_pos: center, curr_pos: curr });
                            }
                            for ch in cra.text_input.chars() {
                                match ch {
                                    'w' | 'W' => {
                                        let _ = suzane.view_3d.apply_scroll(-0.5);
                                    }
                                    's' | 'S' => {
                                        let _ = suzane.view_3d.apply_scroll(0.5);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let area = area_from_args(cra);
                        let submission = MeshSubmission {
                            mesh: suzane.mesh.clone(),
                            pvm: suzane.view_3d.pvm(),
                            view_model: suzane.view_3d.view_matrix(),
                        };
                        let mut renderer = renderer.clone();
                        renderer.scope_mut(|r| {
                            r.enqueue_mesh_draw(area, submission.clone());
                        });
                    }
                });
            });
        });

        let stack_direction_labels = self.stack_direction_labels.clone();
        let stack_direction_buttons = self.stack_direction_buttons.clone();
        self.stack_direction_tree = WidgetTreeBuilder::build(move |tree| {
            let columns = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
            let [label_top, label_bottom] = stack_direction_labels.clone();
            let [button_top_0, button_top_1, button_top_2, button_bottom_0, button_bottom_1, button_bottom_2] = stack_direction_buttons.clone();
            tree.row(&columns, SizePolicy::Auto, |tree| {
                tree.widget(label_top.clone());
                tree.widget(label_bottom.clone());
            });
            tree.row(&columns, SizePolicy::Fixed(120), |tree| {
                tree.column(|tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::TopToBottom, |tree| {
                        tree.widget(button_top_0.clone());
                        tree.widget(button_top_1.clone());
                        tree.widget(button_top_2.clone());
                    });
                });
                tree.column(|tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::BottomToTop, |tree| {
                        tree.widget(button_bottom_0.clone());
                        tree.widget(button_bottom_1.clone());
                        tree.widget(button_bottom_2.clone());
                    });
                });
            });
        });

        let weight_labels = self.weight_labels.clone();
        let weight_buttons = self.weight_buttons.clone();
        self.weight_tree = WidgetTreeBuilder::build(move |tree| {
            let [row_weight_label, grid_weight_label] = weight_labels.clone();
            let [
                button_row_0,
                button_row_1,
                button_row_2,
                button_grid_0,
                button_grid_1,
                button_grid_2,
                button_grid_3,
                button_grid_4,
                button_grid_5,
            ] = weight_buttons.clone();
            let row = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0), SizePolicy::Weight(3.0)];
            let cols = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
            let rows = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];
            tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                tree.widget(row_weight_label.clone());
            });
            tree.row(&row, SizePolicy::Fixed(28), |tree| {
                tree.widget(button_row_0.clone());
                tree.widget(button_row_1.clone());
                tree.widget(button_row_2.clone());
            });
            tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Auto, |tree| {
                tree.widget(grid_weight_label.clone());
            });
            tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Remainder(0), |tree| {
                tree.column(|tree| {
                    tree.grid(&cols, &rows, |tree| {
                        tree.widget(button_grid_0.clone());
                        tree.widget(button_grid_1.clone());
                        tree.widget(button_grid_2.clone());
                        tree.widget(button_grid_3.clone());
                        tree.widget(button_grid_4.clone());
                        tree.widget(button_grid_5.clone());
                    });
                });
            });
        });

        let combo_items = self.combo_items.clone();
        self.combo_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                for item in &combo_items {
                    tree.widget(item.clone());
                }
            });
        });

        let popup_buttons = self.popup_buttons.clone();
        self.popup_tree = WidgetTreeBuilder::build(move |tree| {
            tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                for button in &popup_buttons {
                    tree.widget(button.clone());
                }
            });
        });

        let window_header = self.window_header.clone();
        let test_buttons_header = self.test_buttons_header.clone();
        let background_header = self.background_header.clone();
        let tree_and_text_header = self.tree_and_text_header.clone();
        let text_area_header = self.text_area_header.clone();
        let slot_header = self.slot_header.clone();
        let combo_header = self.combo_header.clone();
        let test1_tn = self.test1_tn.clone();
        let test1a_tn = self.test1a_tn.clone();
        let test1b_tn = self.test1b_tn.clone();
        let test2_tn = self.test2_tn.clone();
        let test3_tn = self.test3_tn.clone();
        let window_info_labels = self.window_info_labels.clone();
        let window_info_values = self.window_info_values.clone();
        let test_buttons = self.test_buttons.clone();
        let test_button_labels = self.test_button_labels.clone();
        let combo_state = self.combo_state.clone();
        let tree_buttons = self.tree_buttons.clone();
        let checkboxes = self.checkboxes.clone();
        let tree_labels = self.tree_labels.clone();
        let text_area = self.text_area.clone();
        let bg_sliders = self.bg_sliders.clone();
        let background_labels = self.background_labels.clone();
        let background_swatch = self.background_swatch.clone();
        let slot_buttons = self.slot_buttons.clone();
        let external_image_button = self.external_image_button.clone();
        self.demo_tree = WidgetTreeBuilder::build(move |tree| {
            let window_info_row = [SizePolicy::Fixed(54), SizePolicy::Remainder(0)];
            let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
            let tree_widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
            let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
            let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
            let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
            let [label_pos, label_size, label_fps] = window_info_labels.clone();
            let [value_pos, value_size, value_fps] = window_info_values.clone();
            let [button0, button1, button2, button3, button4, button5] = test_buttons.clone();
            let [test_label0, test_label1, test_label2] = test_button_labels.clone();
            let [tree_button0, tree_button1, tree_button2, tree_button3, tree_button4, tree_button5] = tree_buttons.clone();
            let [checkbox0, checkbox1, checkbox2] = checkboxes.clone();
            let [tree_label_hello, tree_label_world] = tree_labels.clone();
            let [slider_red, slider_green, slider_blue] = bg_sliders.clone();
            let [label_red, label_green, label_blue] = background_labels.clone();
            let [slot0, slot1, slot2, slot3] = slot_buttons.clone();

            Self::section(tree, &window_header, |tree| {
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_pos.clone());
                    tree.widget(value_pos.clone());
                });
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_size.clone());
                    tree.widget(value_size.clone());
                });
                tree.row(&window_info_row, SizePolicy::Auto, |tree| {
                    tree.widget(label_fps.clone());
                    tree.widget(value_fps.clone());
                });
            });

            Self::section(tree, &test_buttons_header, |tree| {
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label0.clone());
                    tree.widget(button0.clone());
                    tree.widget(button1.clone());
                });
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label1.clone());
                    tree.widget(button2.clone());
                    tree.widget(button3.clone());
                });
                tree.row(&button_widths, SizePolicy::Auto, |tree| {
                    tree.widget(test_label2.clone());
                    tree.widget(button4.clone());
                    tree.widget(button5.clone());
                });
            });

            Self::section(tree, &combo_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                    tree.widget(combo_state.clone());
                });
            });

            Self::section(tree, &tree_and_text_header, |tree| {
                tree.row(&tree_widths, SizePolicy::Auto, |tree| {
                    tree.column(|tree| {
                        tree.tree_node(test1_tn.clone(), |tree| {
                            tree.tree_node(test1a_tn.clone(), |tree| {
                                tree.widget(tree_label_hello.clone());
                                tree.widget(tree_label_world.clone());
                            });
                            tree.tree_node(test1b_tn.clone(), |tree| {
                                tree.widget(tree_button0.clone());
                                tree.widget(tree_button1.clone());
                            });
                        });
                        tree.tree_node(test2_tn.clone(), |tree| {
                            tree.row(&tree_button_widths, SizePolicy::Auto, |tree| {
                                tree.widget(tree_button2.clone());
                                tree.widget(tree_button3.clone());
                            });
                            tree.row(&tree_button_widths, SizePolicy::Auto, |tree| {
                                tree.widget(tree_button4.clone());
                                tree.widget(tree_button5.clone());
                            });
                        });
                        tree.tree_node(test3_tn.clone(), |tree| {
                            tree.widget(checkbox0.clone());
                            tree.widget(checkbox1.clone());
                            tree.widget(checkbox2.clone());
                        });
                    });
                    tree.column(|tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                            tree.text_with_wrap(
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.",
                                TextWrap::Word,
                            );
                        });
                    });
                });
            });

            Self::section(tree, &text_area_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(120), StackDirection::TopToBottom, |tree| {
                    tree.widget(text_area.clone());
                });
            });

            Self::section(tree, &background_header, |tree| {
                tree.row(&background_widths, SizePolicy::Fixed(74), |tree| {
                    tree.column(|tree| {
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_red.clone());
                            tree.widget(slider_red.clone());
                        });
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_green.clone());
                            tree.widget(slider_green.clone());
                        });
                        tree.row(&slider_row, SizePolicy::Auto, |tree| {
                            tree.widget(label_blue.clone());
                            tree.widget(slider_blue.clone());
                        });
                    });
                    tree.widget(background_swatch.clone());
                });
            });

            Self::section(tree, &slot_header, |tree| {
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                    tree.widget(slot0.clone());
                    tree.widget(slot1.clone());
                    tree.widget(slot2.clone());
                    if let Some(button) = external_image_button.clone() {
                        tree.stack(SizePolicy::Fixed(256), SizePolicy::Fixed(256), StackDirection::TopToBottom, |tree| {
                            tree.widget(button.clone());
                        });
                    }
                });
                tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(67), StackDirection::TopToBottom, |tree| {
                    tree.widget(slot3.clone());
                });
            });
        });
    }

    fn style_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        for (i, color) in self.style.colors.iter().enumerate() {
            let slider_base = i * 4;
            self.style_color_sliders[slider_base].borrow_mut().value = color.r as Real;
            self.style_color_sliders[slider_base + 1].borrow_mut().value = color.g as Real;
            self.style_color_sliders[slider_base + 2].borrow_mut().value = color.b as Real;
            self.style_color_sliders[slider_base + 3].borrow_mut().value = color.a as Real;
            self.style_color_swatches[i].borrow_mut().fill = *color;
        }
        self.style_value_sliders[0].borrow_mut().value = self.style.padding as Real;
        self.style_value_sliders[1].borrow_mut().value = self.style.spacing as Real;
        self.style_value_sliders[2].borrow_mut().value = self.style.title_height as Real;
        self.style_value_sliders[3].borrow_mut().value = self.style.thumb_size as Real;
        self.style_value_sliders[4].borrow_mut().value = self.style.scrollbar_size as Real;

        ctx.window(
            &mut self.style_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.style_tree,
        );

        for (color, sliders) in self.style.colors.iter_mut().zip(self.style_color_sliders.chunks_exact(4)) {
            color.r = sliders[0].borrow().value as u8;
            color.g = sliders[1].borrow().value as u8;
            color.b = sliders[2].borrow().value as u8;
            color.a = sliders[3].borrow().value as u8;
        }
        for (swatch, color) in self.style_color_swatches.iter().zip(self.style.colors.iter()) {
            swatch.borrow_mut().fill = *color;
        }
        self.style.padding = self.style_value_sliders[0].borrow().value as i32;
        self.style.spacing = self.style_value_sliders[1].borrow().value as i32;
        self.style.title_height = self.style_value_sliders[2].borrow().value as i32;
        self.style.thumb_size = self.style_value_sliders[3].borrow().value as i32;
        self.style.scrollbar_size = self.style_value_sliders[4].borrow().value as i32;
        ctx.set_style(&self.style);
    }

    fn log_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        self.log_text.borrow_mut().text = self.logbuf.borrow().clone();
        ctx.window(
            &mut self.log_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.log_tree,
        );

        if self.logbuf_updated {
            let mut log_output = self.log_output.as_mut().unwrap().clone();
            log_output.with_mut(|panel| {
                let mut scroll = panel.scroll();
                scroll.y = panel.content_size().height;
                panel.set_scroll(scroll);
            });
            self.logbuf_updated = false;
        }

        let mut submitted = false;
        {
            let results = ctx.committed_results();
            let submit_buf_out = results.state_of_handle(&self.submit_buf);
            let submit_btn_out = results.state_of_handle(&self.submit_button);
            if submit_buf_out.is_submitted() {
                self.log_window.as_mut().unwrap().set_focus(Some(widget_id_of_handle(&self.submit_buf)));
                submitted = true;
            }
            if submit_btn_out.is_submitted() {
                submitted = true;
            }
        }
        if submitted {
            let mut buf = String::new();
            buf.push_str(self.submit_buf.borrow().buf.as_str());
            self.write_log(buf.as_str());
            self.submit_buf.borrow_mut().buf.clear();
        }
    }

    fn triangle_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.triangle_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.triangle_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.triangle_tree,
        );
    }

    fn suzane_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.suzane_window.is_none() {
            return;
        }
        ctx.window(
            &mut self.suzane_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.suzane_tree,
        );
    }

    fn stack_direction_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.stack_direction_window.is_none() {
            return;
        }

        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.stack_direction_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.stack_direction_tree,
        );

        let results = ctx.committed_results();
        if results.state_of_handle(&self.stack_direction_buttons[0]).is_submitted() {
            logs.push("Top->Bottom: call 1");
        }
        if results.state_of_handle(&self.stack_direction_buttons[1]).is_submitted() {
            logs.push("Top->Bottom: call 2");
        }
        if results.state_of_handle(&self.stack_direction_buttons[2]).is_submitted() {
            logs.push("Top->Bottom: call 3");
        }
        if results.state_of_handle(&self.stack_direction_buttons[3]).is_submitted() {
            logs.push("Bottom->Top: call 1");
        }
        if results.state_of_handle(&self.stack_direction_buttons[4]).is_submitted() {
            logs.push("Bottom->Top: call 2");
        }
        if results.state_of_handle(&self.stack_direction_buttons[5]).is_submitted() {
            logs.push("Bottom->Top: call 3");
        }

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn weight_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.weight_window.is_none() {
            return;
        }

        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.weight_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.weight_tree,
        );

        let results = ctx.committed_results();
        if results.state_of_handle(&self.weight_buttons[0]).is_submitted() {
            logs.push("Weight row: 1");
        }
        if results.state_of_handle(&self.weight_buttons[1]).is_submitted() {
            logs.push("Weight row: 2");
        }
        if results.state_of_handle(&self.weight_buttons[2]).is_submitted() {
            logs.push("Weight row: 3");
        }
        if results.state_of_handle(&self.weight_buttons[3]).is_submitted() {
            logs.push("Weight grid: 1");
        }
        if results.state_of_handle(&self.weight_buttons[4]).is_submitted() {
            logs.push("Weight grid: 2");
        }
        if results.state_of_handle(&self.weight_buttons[5]).is_submitted() {
            logs.push("Weight grid: 3");
        }
        if results.state_of_handle(&self.weight_buttons[6]).is_submitted() {
            logs.push("Weight grid: 4");
        }
        if results.state_of_handle(&self.weight_buttons[7]).is_submitted() {
            logs.push("Weight grid: 5");
        }
        if results.state_of_handle(&self.weight_buttons[8]).is_submitted() {
            logs.push("Weight grid: 6");
        }

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn test_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        {
            let window = self.demo_window.as_mut().unwrap();
            let mut win = window.rect();
            win.width = win.width.max(240);
            win.height = win.height.max(300);
            window.set_rect(win);

            let [value_pos, value_size, value_fps] = self.window_info_values.clone();
            value_pos.borrow_mut().label = format!("{}, {}", win.x, win.y);
            value_size.borrow_mut().label = format!("{}, {}", win.width, win.height);
            value_fps.borrow_mut().label = format!("{:.1}", self.fps);
        }

        let combo_labels: Vec<String> = self.combo_items.iter().map(|item| item.borrow().label.clone()).collect();
        self.combo_state.borrow_mut().update_items(&combo_labels);

        self.bg_sliders[0].borrow_mut().value = self.bg[0];
        self.bg_sliders[1].borrow_mut().value = self.bg[1];
        self.bg_sliders[2].borrow_mut().value = self.bg[2];
        {
            let mut swatch = self.background_swatch.borrow_mut();
            swatch.fill = color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255);
            swatch.label = format!("#{:02X}{:02X}{:02X}", swatch.fill.r, swatch.fill.g, swatch.fill.b);
        }

        ctx.window(
            &mut self.demo_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            &self.demo_tree,
        );

        let mut button_logs: Vec<&'static str> = Vec::new();
        let mut tree_logs: Vec<&'static str> = Vec::new();
        let combo_anchor = self.combo_state.borrow().anchor();
        {
            let results = ctx.committed_results();
            if results.state_of_handle(&self.test_buttons[0]).is_submitted() {
                button_logs.push("Pressed button 1");
            }
            if results.state_of_handle(&self.test_buttons[1]).is_submitted() {
                button_logs.push("Pressed button 2");
            }
            if results.state_of_handle(&self.test_buttons[2]).is_submitted() {
                button_logs.push("Pressed button 3");
            }
            if results.state_of_handle(&self.test_buttons[3]).is_submitted() {
                self.open_popup = true;
            }
            if results.state_of_handle(&self.test_buttons[4]).is_submitted() {
                button_logs.push("Pressed button 4");
            }
            if results.state_of_handle(&self.test_buttons[5]).is_submitted() {
                self.open_dialog = true;
            }
            if results.state_of_handle(&self.tree_buttons[0]).is_submitted() {
                tree_logs.push("Pressed button 1");
            }
            if results.state_of_handle(&self.tree_buttons[1]).is_submitted() {
                tree_logs.push("Pressed button 2");
            }
            if results.state_of_handle(&self.tree_buttons[2]).is_submitted() {
                tree_logs.push("Pressed button 3");
            }
            if results.state_of_handle(&self.tree_buttons[3]).is_submitted() {
                tree_logs.push("Pressed button 4");
            }
            if results.state_of_handle(&self.tree_buttons[4]).is_submitted() {
                tree_logs.push("Pressed button 5");
            }
            if results.state_of_handle(&self.tree_buttons[5]).is_submitted() {
                tree_logs.push("Pressed button 6");
            }
        }
        self.bg[0] = self.bg_sliders[0].borrow().value;
        self.bg[1] = self.bg_sliders[1].borrow().value;
        self.bg[2] = self.bg_sliders[2].borrow().value;
        {
            let mut swatch = self.background_swatch.borrow_mut();
            swatch.fill = color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255);
            swatch.label = format!("#{:02X}{:02X}{:02X}", swatch.fill.r, swatch.fill.g, swatch.fill.b);
        }
        for msg in button_logs {
            self.write_log(msg);
        }
        for msg in tree_logs {
            self.write_log(msg);
        }

        let mut popup = self.combo_state.borrow().popup.clone();
        if self.combo_state.borrow().is_open() {
            ctx.open_popup_at(&mut popup, combo_anchor);
        }

        ctx.popup(&mut popup, WidgetBehaviourOption::NO_SCROLL, &self.combo_tree);
        let combo_log = {
            let results = ctx.committed_results();
            let mut selected_label = None;
            for (idx, item) in self.combo_items.iter().enumerate() {
                if results.state_of_handle(item).is_submitted() {
                    selected_label = self.combo_state.borrow_mut().select(idx, &combo_labels);
                    break;
                }
            }
            selected_label
        };
        if let Some(label) = combo_log {
            let msg = format!("Selected: {label}");
            self.write_log(msg.as_str());
        }

        if self.open_popup {
            let popup_width = (self.style.default_cell_width + self.style.padding.max(0) * 2).max(80);
            let popup = self.popup_window.as_mut().unwrap();
            ctx.open_popup(popup);
            popup.set_size(&Dimensioni::new(popup_width, 1));
            self.open_popup = false;
        }

        let mut popup_logs: Vec<&'static str> = Vec::new();
        ctx.popup(
            &mut self.popup_window.as_mut().unwrap().clone(),
            WidgetBehaviourOption::NO_SCROLL,
            &self.popup_tree,
        );
        {
            let results = ctx.committed_results();
            if results.state_of_handle(&self.popup_buttons[0]).is_submitted() {
                popup_logs.push("Hello")
            }
            if results.state_of_handle(&self.popup_buttons[1]).is_submitted() {
                popup_logs.push("World")
            }
        }
        for msg in popup_logs {
            self.write_log(msg);
        }

        self.dialog(ctx);
    }

    fn dialog(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.open_dialog {
            self.dialog_window.as_mut().unwrap().open(ctx);
            self.open_dialog = false;
            self.write_log("Open dialog!");
        }

        let dialog_result = {
            let dialog = self.dialog_window.as_mut().unwrap();
            let was_open = dialog.is_open();
            dialog.eval(ctx);
            if was_open && !dialog.is_open() {
                Some(dialog.file_name().clone())
            } else {
                None
            }
        };
        if let Some(result) = dialog_result {
            match result {
                Some(name) => {
                    let mut msg = String::new();
                    msg.push_str("Selected file: ");
                    msg.push_str(name.as_str());
                    self.write_log(msg.as_str());
                }
                None => self.write_log("File dialog canceled"),
            }
        }
    }

    fn process_frame(&mut self, ctx: &mut Context<BackendRenderer>) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        if dt > 0.0 {
            let inst_fps = 1.0 / dt;
            self.fps = if self.fps == 0.0 { inst_fps } else { self.fps * 0.9 + inst_fps * 0.1 };
        }

        ctx.frame(|ctx| {
            self.style_window(ctx);
            self.log_window(ctx);
            self.test_window(ctx);
            self.triangle_window(ctx);
            self.suzane_window(ctx);
            self.stack_direction_window(ctx);
            self.weight_window(ctx);
        })
    }
}

fn main() {
    let slots_orig = atlas_assets::default_slots();
    let mut atlas = atlas_assets::load_atlas(&slots_orig);
    let slots = atlas.clone_slot_table();
    atlas.render_slot(slots[0], Rc::new(|_x, _y| color4b(0xFF, 0, 0, 0xFF)));
    atlas.render_slot(slots[1], Rc::new(|_x, _y| color4b(0, 0xFF, 0, 0xFF)));
    atlas.render_slot(slots[2], Rc::new(|_x, _y| color4b(0, 0, 0xFF, 0xFF)));
    #[cfg(feature = "builder")]
    {
        builder::Builder::save_png_image(atlas.clone(), "atlas.png").unwrap();
    }

    let mut app = Application::new(atlas.clone(), move |backend: BackendInitContext, ctx| {
        let slots = atlas.clone_slot_table();
        let renderer = ctx.renderer_handle();
        State::new(backend, renderer, slots, ctx)
    })
    .unwrap();

    app.event_loop(|ctx, state| {
        state.process_frame(ctx);
    });
}

fn area_from_args(args: &CustomRenderArgs) -> CustomRenderArea {
    let clip = args
        .content_area
        .intersect(&args.view)
        .unwrap_or_else(|| rect(args.content_area.x, args.content_area.y, 0, 0));
    CustomRenderArea { rect: args.content_area, clip }
}

fn build_triangle_vertices(area: Recti, white_uv: Vec2f, angle: f32) -> Vec<Vertex> {
    let (sin_theta, cos_theta) = angle.sin_cos();
    let half_w = (area.width.max(1) as f32) * 0.5;
    let half_h = (area.height.max(1) as f32) * 0.5;
    let cx = area.x as f32 + half_w;
    let cy = area.y as f32 + half_h;

    let order = [0usize, 2, 1]; // convert to clockwise winding for Vulkan UI pipeline
    order
        .iter()
        .map(|tv| {
            let tv = &TRI_VERTS[*tv];
            let rx = tv.pos.x * cos_theta - tv.pos.y * sin_theta;
            let ry = tv.pos.x * sin_theta + tv.pos.y * cos_theta;
            let pos = Vec2f::new(cx + rx * half_w, cy + ry * half_h);
            Vertex::new(pos, white_uv, tv.color)
        })
        .collect()
}

fn build_mesh_buffers(mesh: &PolyMesh) -> MeshBuffers {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for poly in mesh.polys() {
        let start = vertices.len() as u32;
        let mut count = 0;
        for v in poly {
            let position = mesh.vertex_position(v.pos);
            let normal = mesh.vertex_normal(v.normal);
            let uv = mesh.vertex_uv(v.tex);
            vertices.push(MeshVertex {
                position: [position.x, position.y, position.z],
                normal: [normal.x, normal.y, normal.z],
                uv: [uv.x, uv.y],
            });
            count += 1;
        }
        for i in 2..count {
            indices.push(start);
            indices.push(start + i as u32 - 1);
            indices.push(start + i as u32);
        }
    }
    MeshBuffers::from_vecs(vertices, indices)
}

fn demo_asset_path(relative: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[cfg(any(feature = "builder", feature = "png_source"))]
fn load_external_image_texture(ctx: &mut Context<BackendRenderer>) -> Option<TextureId> {
    let image_path = demo_asset_path("examples/FACEPALM.png");
    let png_bytes = match fs::read(&image_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("Failed to read {}: {err}", image_path.display());
            return None;
        }
    };
    match ctx.load_image_from(ImageSource::Png { bytes: png_bytes.as_slice() }) {
        Ok(texture) => Some(texture),
        Err(err) => {
            eprintln!("Failed to decode {}: {err}", image_path.display());
            None
        }
    }
}
