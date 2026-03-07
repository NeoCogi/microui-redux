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

fn static_label(text: impl Into<String>) -> ListItem {
    ListItem::with_opt(text, WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)
}

struct State {
    renderer: RendererHandle<BackendRenderer>,
    bg: [Real; 3],
    bg_sliders: [Slider; 3],
    style_color_sliders: [Slider; 60],
    style_value_sliders: [Slider; 5],
    logbuf: String,
    logbuf_updated: bool,
    submit_buf: Textbox,
    text_area: TextArea,
    combo_state: Option<Combo>,
    combo_items: [ListItem; 4],
    style_color_labels: [ListItem; 14],
    style_metric_labels: [ListItem; 5],
    stack_direction_labels: [ListItem; 2],
    weight_labels: [ListItem; 2],
    window_info_labels: [ListItem; 3],
    window_info_values: [ListItem; 3],
    test_button_labels: [ListItem; 3],
    tree_labels: [ListItem; 2],
    background_labels: [ListItem; 3],
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

    window_header: Node,
    test_buttons_header: Node,
    background_header: Node,
    tree_and_text_header: Node,
    text_area_header: Node,
    slot_header: Node,
    combo_header: Node,
    test1_tn: Node,
    test1a_tn: Node,
    test1b_tn: Node,
    test2_tn: Node,
    test3_tn: Node,
    submit_button: Button,
    test_buttons: [Button; 6],
    tree_buttons: [Button; 6],
    popup_buttons: [Button; 2],
    slot_buttons: [Button; 4],
    stack_direction_buttons: [Button; 6],
    weight_buttons: [Button; 9],
    external_image_button: Option<Button>,
    checkboxes: [Checkbox; 3],
    open_popup: bool,
    open_dialog: bool,
    white_uv: Vec2f,
    triangle_data: Arc<RwLock<TriangleState>>,
    suzane_data: Arc<RwLock<SuzaneData>>,
    triangle_widget: Custom,
    suzane_widget: Custom,
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
            Button::with_image("Slot 1", Some(Image::Slot(slots[0])), WidgetOption::NONE, WidgetFillOption::ALL),
            Button::with_slot("Slot 2 - Green", slots[1], green_paint, WidgetOption::NONE, WidgetFillOption::ALL),
            Button::with_image("Slot 3", Some(Image::Slot(slots[2])), WidgetOption::NONE, WidgetFillOption::ALL),
            Button::with_slot("Slot 2 - Random", slots[1], random_paint, WidgetOption::NONE, WidgetFillOption::ALL),
        ];
        let external_image_button =
            image_texture.map(|texture| Button::with_image("External Image", Some(Image::Texture(texture)), WidgetOption::NONE, WidgetFillOption::ALL));
        let style_color_sliders = std::array::from_fn(|_| Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER));
        let style_value_sliders = [
            Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER),
            Slider::with_opt(0.0, 0.0, 16.0, 0.0, 0, WidgetOption::ALIGN_CENTER),
            Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER),
            Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER),
            Slider::with_opt(0.0, 0.0, 128.0, 0.0, 0, WidgetOption::ALIGN_CENTER),
        ];
        let bg_sliders = std::array::from_fn(|_| Slider::with_opt(0.0, 0.0, 255.0, 0.0, 0, WidgetOption::ALIGN_CENTER));
        let mut text_area =
            TextArea::new("This is a multi-line TextArea.\nYou can type, scroll, and resize the window.\n\nTry adding more lines to see the scrollbars.");
        text_area.wrap = TextWrap::Word;

        Self {
            renderer,
            bg: [90.0, 95.0, 100.0],
            bg_sliders,
            style_color_sliders,
            style_value_sliders,
            logbuf: String::new(),
            logbuf_updated: false,
            submit_buf: Textbox::new(""),
            text_area,
            combo_state: None,
            combo_items: [ListItem::new("Apple"), ListItem::new("Banana"), ListItem::new("Cherry"), ListItem::new("Date")],
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
            demo_window: None,
            style_window: None,
            log_window: None,
            popup_window: None,
            log_output: None,
            triangle_window: None,
            suzane_window: None,
            stack_direction_window: None,
            weight_window: None,
            dialog_window: None,
            fps: 0.0,
            last_frame: Instant::now(),
            window_header: Node::header("Window Info", NodeStateValue::Closed),
            test_buttons_header: Node::header("Test Buttons", NodeStateValue::Expanded),
            background_header: Node::header("Background Color", NodeStateValue::Expanded),
            tree_and_text_header: Node::header("Tree and Text", NodeStateValue::Expanded),
            text_area_header: Node::header("TextArea", NodeStateValue::Expanded),
            slot_header: Node::header("Slots", NodeStateValue::Expanded),
            combo_header: Node::header("Combo Box", NodeStateValue::Expanded),
            test1_tn: Node::tree("Test 1", NodeStateValue::Closed),
            test1a_tn: Node::tree("Test 1a", NodeStateValue::Closed),
            test1b_tn: Node::tree("Test 1b", NodeStateValue::Closed),
            test2_tn: Node::tree("Test 2", NodeStateValue::Closed),
            test3_tn: Node::tree("Test 3", NodeStateValue::Closed),
            submit_button: Button::with_opt("Submit", WidgetOption::ALIGN_CENTER),
            test_buttons: [
                Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Popup", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Dialog", WidgetOption::ALIGN_CENTER),
            ],
            tree_buttons: [
                Button::with_opt("Button 1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 3", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 4", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 5", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Button 6", WidgetOption::ALIGN_CENTER),
            ],
            popup_buttons: [
                Button::with_opt("Hello", WidgetOption::ALIGN_CENTER),
                Button::with_opt("World", WidgetOption::ALIGN_CENTER),
            ],
            slot_buttons,
            stack_direction_buttons: [
                Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Call 1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Call 2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("Call 3", WidgetOption::ALIGN_CENTER),
            ],
            weight_buttons: [
                Button::with_opt("w1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("w2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("w3", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g1", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g2", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g3", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g4", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g5", WidgetOption::ALIGN_CENTER),
                Button::with_opt("g6", WidgetOption::ALIGN_CENTER),
            ],
            external_image_button,
            checkboxes: [
                Checkbox::new("Checkbox 1", false),
                Checkbox::new("Checkbox 2", true),
                Checkbox::new("Checkbox 3", false),
            ],
            open_popup: false,
            open_dialog: false,
            white_uv,
            triangle_data,
            suzane_data,
            triangle_widget: Custom::with_opt("Triangle", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::NONE),
            suzane_widget: Custom::with_opt("Suzane", WidgetOption::HOLD_FOCUS, WidgetBehaviourOption::GRAB_SCROLL),
        }
    }

    fn write_log(&mut self, text: &str) {
        if self.logbuf.len() != 0 {
            self.logbuf.push('\n');
        }
        for c in text.chars() {
            self.logbuf.push(c);
        }
        self.logbuf_updated = true;
    }

    fn u8_slider(value: &mut u8, slider: &mut Slider, ctx: &mut Container, results: &mut FrameResults) -> ResourceState {
        slider.value = *value as Real;
        ctx.build_tree(results, |tree| {
            tree.widget(slider);
        });
        let res = results.state_of(&*slider);
        *value = slider.value as u8;
        slider.value = *value as Real;
        res
    }

    fn i32_slider(value: &mut i32, slider: &mut Slider, ctx: &mut Container, results: &mut FrameResults) -> ResourceState {
        slider.value = *value as Real;
        ctx.build_tree(results, |tree| {
            tree.widget(slider);
        });
        let res = results.state_of(&*slider);
        *value = slider.value as i32;
        slider.value = *value as Real;
        res
    }

    fn section<'a, F: FnOnce(&mut WidgetTreeBuilder<'a>)>(tree: &mut WidgetTreeBuilder<'a>, node: &'a mut Node, f: F) {
        tree.header(node, f);
    }

    fn legacy_tree_section<F: FnOnce(&mut Container, &mut FrameResults)>(container: &mut Container, results: &mut FrameResults, node: &mut Node, f: F) {
        container.set_row_flow(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
        let mut __runs = [widget_ref(node)];
        container.widgets(results, &mut __runs);
        if node.is_expanded() {
            let indent = container.get_style().indent;
            container.with_indent(indent, |container| f(container, results));
        }
    }

    fn style_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        let style_color_labels = &mut self.style_color_labels;
        let style_metric_labels = &mut self.style_metric_labels;
        let style_color_sliders = &mut self.style_color_sliders;
        let style_value_sliders = &mut self.style_value_sliders;
        let style = &mut self.style;
        ctx.window(
            &mut self.style_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
                let sw = (container.body().width as f64 * 0.14) as i32;
                let color_row = [
                    SizePolicy::Fixed(80),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Remainder(0),
                ];
                let metrics_row = [SizePolicy::Fixed(80), SizePolicy::Fixed(sw)];
                for (i, color) in style.colors.iter().enumerate() {
                    let slider_base = i * 4;
                    style_color_sliders[slider_base].value = color.r as Real;
                    style_color_sliders[slider_base + 1].value = color.g as Real;
                    style_color_sliders[slider_base + 2].value = color.b as Real;
                    style_color_sliders[slider_base + 3].value = color.a as Real;
                }
                style_value_sliders[0].value = style.padding as Real;
                style_value_sliders[1].value = style.spacing as Real;
                style_value_sliders[2].value = style.title_height as Real;
                style_value_sliders[3].value = style.thumb_size as Real;
                style_value_sliders[4].value = style.scrollbar_size as Real;

                container.build_tree(results, |tree| {
                    tree.run(|container, results| {
                        container.with_row(&color_row, SizePolicy::Auto, |container| {
                            for (i, label) in style_color_labels.iter_mut().enumerate() {
                                let mut label_runs = [widget_ref(label)];
                                container.widgets(results, &mut label_runs);
                                unsafe {
                                    let color = style.colors.as_mut_ptr().offset(i as isize);
                                    let slider_base = i * 4;
                                    Self::u8_slider(&mut (*color).r, &mut style_color_sliders[slider_base], container, results);
                                    Self::u8_slider(&mut (*color).g, &mut style_color_sliders[slider_base + 1], container, results);
                                    Self::u8_slider(&mut (*color).b, &mut style_color_sliders[slider_base + 2], container, results);
                                    Self::u8_slider(&mut (*color).a, &mut style_color_sliders[slider_base + 3], container, results);
                                }
                                let next_layout = container.next_cell();
                                let color = style.colors[i];
                                container.draw_rect(next_layout, color);
                            }
                        });
                        container.with_row(&metrics_row, SizePolicy::Auto, |container| {
                            for (idx, label) in style_metric_labels.iter_mut().enumerate() {
                                let mut label_runs = [widget_ref(label)];
                                container.widgets(results, &mut label_runs);
                                match idx {
                                    0 => Self::i32_slider(&mut style.padding, &mut style_value_sliders[0], container, results),
                                    1 => Self::i32_slider(&mut style.spacing, &mut style_value_sliders[1], container, results),
                                    2 => Self::i32_slider(&mut style.title_height, &mut style_value_sliders[2], container, results),
                                    3 => Self::i32_slider(&mut style.thumb_size, &mut style_value_sliders[3], container, results),
                                    _ => Self::i32_slider(&mut style.scrollbar_size, &mut style_value_sliders[4], container, results),
                                };
                            }
                        });
                    });
                });
                WindowState::Open
            },
        );
        ctx.set_style(style);
    }

    fn log_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        ctx.window(
            &mut self.log_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
                let mut submitted = false;
                let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
                let log_output = self.log_output.as_mut().unwrap().clone();
                let logbuf = &self.logbuf;
                let logbuf_updated = &mut self.logbuf_updated;

                container.build_tree(results, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(24), StackDirection::TopToBottom, |tree| {
                        tree.container(log_output, ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                            tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                                tree.run(move |container, _results| {
                                    let mut scroll = container.scroll();
                                    let content_size = container.content_size();
                                    container.text(logbuf.as_str());

                                    if *logbuf_updated {
                                        scroll.y = content_size.y;
                                        container.set_scroll(scroll);
                                        *logbuf_updated = false;
                                    }
                                });
                            });
                        });
                    });
                    tree.row(&submit_row, SizePolicy::Auto, |tree| {
                        tree.widget(&mut self.submit_buf);
                        tree.widget(&mut self.submit_button);
                    });
                });

                let submit_buf_out = results.state_of(&self.submit_buf);
                let submit_btn_out = results.state_of(&self.submit_button);
                if submit_buf_out.is_submitted() {
                    container.set_focus(Some(widget_id_of(&self.submit_buf)));
                    submitted = true;
                }
                if submit_btn_out.is_submitted() {
                    submitted = true;
                }
                if submitted {
                    let mut buf = String::new();
                    buf.push_str(self.submit_buf.buf.as_str());
                    self.write_log(buf.as_str());
                    self.submit_buf.buf.clear();
                }
                WindowState::Open
            },
        );
    }

    fn triangle_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.triangle_window.is_none() {
            return;
        }
        let renderer = self.renderer.clone();
        let tri_state = self.triangle_data.clone();
        let white_uv = self.white_uv;
        let triangle_widget = &mut self.triangle_widget;
        ctx.window(
            &mut self.triangle_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
                container.build_tree(results, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                        tree.run(move |container, results| {
                            let tri_state = tri_state.clone();
                            let mut renderer = renderer.clone();
                            container.widget_custom_render(results, triangle_widget, move |_dim, cra| {
                                if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                                    return;
                                }
                                let area = area_from_args(&cra);
                                if let Ok(mut tri) = tri_state.write() {
                                    tri.angle = (tri.angle + 0.02) % (std::f32::consts::PI * 2.0);
                                    let mut verts = build_triangle_vertices(area.rect, white_uv, tri.angle);
                                    renderer.scope_mut(move |vk| {
                                        let verts_local = std::mem::take(&mut verts);
                                        vk.enqueue_colored_vertices(area, verts_local);
                                    });
                                }
                            });
                        });
                    });
                });
                WindowState::Open
            },
        );
    }

    fn suzane_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.suzane_window.is_none() {
            return;
        }
        let renderer = self.renderer.clone();
        let suzane_state = self.suzane_data.clone();
        let suzane_widget = &mut self.suzane_widget;
        ctx.window(
            &mut self.suzane_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
                container.build_tree(results, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Remainder(0), StackDirection::TopToBottom, |tree| {
                        tree.run(move |container, results| {
                            let suzane_state = suzane_state.clone();
                            let mut renderer = renderer.clone();
                            container.widget_custom_render(results, suzane_widget, move |_dim, cra| {
                                if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                                    return;
                                }
                                if let Ok(mut suzane) = suzane_state.write() {
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
                                                    suzane.view_3d.apply_scroll(-0.5);
                                                }
                                                's' | 'S' => {
                                                    suzane.view_3d.apply_scroll(0.5);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }

                                    let area = area_from_args(&cra);
                                    let submission = MeshSubmission {
                                        mesh: suzane.mesh.clone(),
                                        pvm: suzane.view_3d.pvm(),
                                        view_model: suzane.view_3d.view_matrix(),
                                    };
                                    renderer.scope_mut(|r| {
                                        r.enqueue_mesh_draw(area, submission.clone());
                                    });
                                }
                            });
                        });
                    });
                });
                WindowState::Open
            },
        );
    }

    fn stack_direction_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.stack_direction_window.is_none() {
            return;
        }

        let buttons = &mut self.stack_direction_buttons;
        let stack_direction_labels = &mut self.stack_direction_labels;
        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.stack_direction_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
                let spacing = container.get_style().spacing.max(0);
                let body_width = container.body().width.max(1);
                let left_width = body_width.saturating_sub(spacing).max(2) / 2;
                let columns = [SizePolicy::Fixed(left_width), SizePolicy::Remainder(0)];

                let [button_top_0, button_top_1, button_top_2, button_bottom_0, button_bottom_1, button_bottom_2] = buttons;
                let [label_top, label_bottom] = stack_direction_labels;
                container.build_tree(results, |tree| {
                    tree.row(&columns, SizePolicy::Auto, |tree| {
                        tree.widget(label_top);
                        tree.widget(label_bottom);
                    });
                    tree.row(&columns, SizePolicy::Fixed(120), |tree| {
                        tree.column(|tree| {
                            tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::TopToBottom, |tree| {
                                tree.widget(button_top_0);
                                tree.widget(button_top_1);
                                tree.widget(button_top_2);
                            });
                        });
                        tree.column(|tree| {
                            tree.stack(SizePolicy::Remainder(0), SizePolicy::Fixed(28), StackDirection::BottomToTop, |tree| {
                                tree.widget(button_bottom_0);
                                tree.widget(button_bottom_1);
                                tree.widget(button_bottom_2);
                            });
                        });
                    });
                });

                if results.state_of(&*button_top_0).is_submitted() {
                    logs.push("Top->Bottom: call 1");
                }
                if results.state_of(&*button_top_1).is_submitted() {
                    logs.push("Top->Bottom: call 2");
                }
                if results.state_of(&*button_top_2).is_submitted() {
                    logs.push("Top->Bottom: call 3");
                }
                if results.state_of(&*button_bottom_0).is_submitted() {
                    logs.push("Bottom->Top: call 1");
                }
                if results.state_of(&*button_bottom_1).is_submitted() {
                    logs.push("Bottom->Top: call 2");
                }
                if results.state_of(&*button_bottom_2).is_submitted() {
                    logs.push("Bottom->Top: call 3");
                }
                WindowState::Open
            },
        );

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn weight_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.weight_window.is_none() {
            return;
        }

        let buttons = &mut self.weight_buttons;
        let weight_labels = &mut self.weight_labels;
        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.weight_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container, results| {
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
                ] = buttons;
                let [row_weight_label, grid_weight_label] = weight_labels;
                let row = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0), SizePolicy::Weight(3.0)];
                let cols = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
                let rows = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];

                container.build_tree(results, |tree| {
                    tree.row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |tree| {
                        tree.widget(row_weight_label);
                    });
                    tree.row(&row, SizePolicy::Fixed(28), |tree| {
                        tree.widget(button_row_0);
                        tree.widget(button_row_1);
                        tree.widget(button_row_2);
                    });
                    tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Auto, |tree| {
                        tree.widget(grid_weight_label);
                    });
                    tree.row(&[SizePolicy::Weight(1.0)], SizePolicy::Remainder(0), |tree| {
                        tree.column(|tree| {
                            tree.grid(&cols, &rows, |tree| {
                                tree.widget(button_grid_0);
                                tree.widget(button_grid_1);
                                tree.widget(button_grid_2);
                                tree.widget(button_grid_3);
                                tree.widget(button_grid_4);
                                tree.widget(button_grid_5);
                            });
                        });
                    });
                });

                if results.state_of(&*button_row_0).is_submitted() {
                    logs.push("Weight row: 1");
                }
                if results.state_of(&*button_row_1).is_submitted() {
                    logs.push("Weight row: 2");
                }
                if results.state_of(&*button_row_2).is_submitted() {
                    logs.push("Weight row: 3");
                }
                if results.state_of(&*button_grid_0).is_submitted() {
                    logs.push("Weight grid: 1");
                }
                if results.state_of(&*button_grid_1).is_submitted() {
                    logs.push("Weight grid: 2");
                }
                if results.state_of(&*button_grid_2).is_submitted() {
                    logs.push("Weight grid: 3");
                }
                if results.state_of(&*button_grid_3).is_submitted() {
                    logs.push("Weight grid: 4");
                }
                if results.state_of(&*button_grid_4).is_submitted() {
                    logs.push("Weight grid: 5");
                }
                if results.state_of(&*button_grid_5).is_submitted() {
                    logs.push("Weight grid: 6");
                }
                WindowState::Open
            },
        );

        for msg in logs {
            self.write_log(msg);
        }
    }

    fn test_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        let mut combo_anchor = None;
        let mut combo_changed = false;

        ctx.window(&mut self.demo_window.as_mut().unwrap().clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |container, results| {
            let mut win = container.rect();
            win.width = win.width.max(240);
            win.height = win.height.max(300);

            container.set_rect(win);

            let mut buff = String::new();
            let fps = self.fps;
            let mut button_logs: Vec<&'static str> = Vec::new();
            let mut tree_logs: Vec<&'static str> = Vec::new();
            {
                let window_header = &mut self.window_header;
                let window_info_labels = &mut self.window_info_labels;
                let window_info_values = &mut self.window_info_values;
                let test_buttons_header = &mut self.test_buttons_header;
                let test_buttons = &mut self.test_buttons;
                let test_button_labels = &mut self.test_button_labels;
                let open_popup = &mut self.open_popup;
                let open_dialog = &mut self.open_dialog;
                let combo_header = &mut self.combo_header;
                let combo_state = self.combo_state.as_mut().unwrap();
                let combo_labels = [
                    self.combo_items[0].label.as_str(),
                    self.combo_items[1].label.as_str(),
                    self.combo_items[2].label.as_str(),
                    self.combo_items[3].label.as_str(),
                ];
                let tree_and_text_header = &mut self.tree_and_text_header;
                let test1_tn = &mut self.test1_tn;
                let test1a_tn = &mut self.test1a_tn;
                let test1b_tn = &mut self.test1b_tn;
                let test2_tn = &mut self.test2_tn;
                let test3_tn = &mut self.test3_tn;
                let tree_buttons = &mut self.tree_buttons;
                let checkboxes = &mut self.checkboxes;
                let tree_labels = &mut self.tree_labels;
                let text_area_header = &mut self.text_area_header;
                let text_area = &mut self.text_area;
                let background_header = &mut self.background_header;
                let bg = &mut self.bg;
                let bg_sliders = &mut self.bg_sliders;
                let background_labels = &mut self.background_labels;
                let slot_header = &mut self.slot_header;
                let slot_buttons = &mut self.slot_buttons;
                let external_image_button = &mut self.external_image_button;
                container.build_tree(results, |tree| {
                    Self::section(tree, window_header, |tree| {
                        tree.run(|container, results| {
                            let [label_pos, label_size, label_fps] = window_info_labels;
                            let [value_pos, value_size, value_fps] = window_info_values;
                            let win_0 = container.rect();
                            let row_widths = [SizePolicy::Fixed(54), SizePolicy::Remainder(0)];
                            buff.clear();
                            buff.push_str(format!("{}, {}", win_0.x, win_0.y).as_str());
                            value_pos.label.clear();
                            value_pos.label.push_str(buff.as_str());
                            let mut runs = [widget_ref(label_pos), widget_ref(value_pos)];
                            container.row_widgets(results, &row_widths, SizePolicy::Auto, &mut runs);

                            buff.clear();
                            buff.push_str(format!("{}, {}", win_0.width, win_0.height).as_str());
                            value_size.label.clear();
                            value_size.label.push_str(buff.as_str());
                            let mut runs = [widget_ref(label_size), widget_ref(value_size)];
                            container.row_widgets(results, &row_widths, SizePolicy::Auto, &mut runs);

                            buff.clear();
                            buff.push_str(format!("{:.1}", fps).as_str());
                            value_fps.label.clear();
                            value_fps.label.push_str(buff.as_str());
                            let mut runs = [widget_ref(label_fps), widget_ref(value_fps)];
                            container.row_widgets(results, &row_widths, SizePolicy::Auto, &mut runs);
                        });
                    });

                    Self::section(tree, test_buttons_header, |tree| {
                        tree.run(|container, results| {
                            let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
                            let [button0, button1, button2, button3, button4, button5] = test_buttons;
                            let [label0, label1, label2] = test_button_labels;
                            let button0_id = widget_id_of(&*button0);
                            let button1_id = widget_id_of(&*button1);
                            let button2_id = widget_id_of(&*button2);
                            let button3_id = widget_id_of(&*button3);
                            let button4_id = widget_id_of(&*button4);
                            let button5_id = widget_id_of(&*button5);
                            let mut runs = [
                                widget_ref(label0),
                                widget_ref(button0),
                                widget_ref(button1),
                                widget_ref(label1),
                                widget_ref(button2),
                                widget_ref(button3),
                                widget_ref(label2),
                                widget_ref(button4),
                                widget_ref(button5),
                            ];
                            container.row_widgets(results, &button_widths, SizePolicy::Auto, &mut runs);

                            if results.state(button0_id).is_submitted() {
                                button_logs.push("Pressed button 1");
                            }
                            if results.state(button1_id).is_submitted() {
                                button_logs.push("Pressed button 2");
                            }
                            if results.state(button2_id).is_submitted() {
                                button_logs.push("Pressed button 3");
                            }
                            if results.state(button3_id).is_submitted() {
                                *open_popup = true;
                            }
                            if results.state(button4_id).is_submitted() {
                                button_logs.push("Pressed button 4");
                            }
                            if results.state(button5_id).is_submitted() {
                                *open_dialog = true;
                            }
                        });
                    });

                    Self::section(tree, combo_header, |tree| {
                        tree.run(|container, results| {
                            container.stack(SizePolicy::Auto, |container| {
                                combo_state.update_items(&combo_labels);
                                let combo_id = widget_id_of(&*combo_state);
                                let mut __runs = [widget_ref(combo_state)];
                                container.widgets(results, &mut __runs);
                                let res = results.state(combo_id);
                                combo_anchor = Some(combo_state.anchor());
                                if res.is_submitted() {
                                    combo_state.open = !combo_state.open;
                                }
                            });
                        });
                    });

                    Self::section(tree, tree_and_text_header, |tree| {
                        tree.run(|container, results| {
                            let widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
                            container.with_row(&widths, SizePolicy::Auto, |container| {
                                container.column(|container| {
                                    Self::legacy_tree_section(container, results, test1_tn, |container, results| {
                                        Self::legacy_tree_section(container, results, test1a_tn, |container, results| {
                                            let [label_hello, label_world] = tree_labels;
                                            let mut runs = [widget_ref(label_hello), widget_ref(label_world)];
                                            container.widgets(results, &mut runs);
                                        });
                                        Self::legacy_tree_section(container, results, test1b_tn, |container, results| {
                                            let (button0, button1) = {
                                                let (head, tail) = tree_buttons.split_at_mut(1);
                                                (&mut head[0], &mut tail[0])
                                            };
                                            let button0_id = widget_id_of(&*button0);
                                            let button1_id = widget_id_of(&*button1);
                                            let mut runs = [widget_ref(button0), widget_ref(button1)];
                                            container.widgets(results, &mut runs);
                                            if results.state(button0_id).is_submitted() {
                                                tree_logs.push("Pressed button 1");
                                            }
                                            if results.state(button1_id).is_submitted() {
                                                tree_logs.push("Pressed button 2");
                                            }
                                        });
                                    });
                                    Self::legacy_tree_section(container, results, test2_tn, |container, results| {
                                        let (button2, button3, button4, button5) = {
                                            let (_, tail) = tree_buttons.split_at_mut(2);
                                            let (b2_slice, tail) = tail.split_at_mut(1);
                                            let (b3_slice, tail) = tail.split_at_mut(1);
                                            let (b4_slice, b5_slice) = tail.split_at_mut(1);
                                            (&mut b2_slice[0], &mut b3_slice[0], &mut b4_slice[0], &mut b5_slice[0])
                                        };
                                        let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
                                        let button2_id = widget_id_of(&*button2);
                                        let button3_id = widget_id_of(&*button3);
                                        let button4_id = widget_id_of(&*button4);
                                        let button5_id = widget_id_of(&*button5);
                                        let mut runs = [
                                            widget_ref(button2),
                                            widget_ref(button3),
                                            widget_ref(button4),
                                            widget_ref(button5),
                                        ];
                                        container.row_widgets(results, &tree_button_widths, SizePolicy::Auto, &mut runs);
                                        if results.state(button2_id).is_submitted() {
                                            tree_logs.push("Pressed button 3");
                                        }
                                        if results.state(button3_id).is_submitted() {
                                            tree_logs.push("Pressed button 4");
                                        }
                                        if results.state(button4_id).is_submitted() {
                                            tree_logs.push("Pressed button 5");
                                        }
                                        if results.state(button5_id).is_submitted() {
                                            tree_logs.push("Pressed button 6");
                                        }
                                    });
                                    Self::legacy_tree_section(container, results, test3_tn, |container, results| {
                                        let (checkbox0, checkbox1, checkbox2) = {
                                            let (head, tail) = checkboxes.split_at_mut(1);
                                            let (mid, tail) = tail.split_at_mut(1);
                                            (&mut head[0], &mut mid[0], &mut tail[0])
                                        };
                                        let mut runs = [
                                            widget_ref(checkbox0),
                                            widget_ref(checkbox1),
                                            widget_ref(checkbox2),
                                        ];
                                        container.widgets(results, &mut runs);
                                    });
                                });
                                container.column(|container| {
                                    container.stack(SizePolicy::Auto, |container| {
                                        container.text_with_wrap(
                                            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.",
                                            TextWrap::Word,
                                        );
                                    });
                                });
                            });
                        });
                    });

                    Self::section(tree, text_area_header, |tree| {
                        tree.run(|container, results| {
                            container.stack(SizePolicy::Fixed(120), |container| {
                                let mut __runs = [widget_ref(text_area)];
                                container.widgets(results, &mut __runs);
                            });
                        });
                    });

                    Self::section(tree, background_header, |tree| {
                        tree.run(|container, results| {
                            let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
                            container.with_row(&background_widths, SizePolicy::Fixed(74), |container| {
                                let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
                                container.column(|container| {
                                    let (slider_red, slider_green, slider_blue) = {
                                        let (head, tail) = bg_sliders.split_at_mut(1);
                                        let (mid, tail) = tail.split_at_mut(1);
                                        (&mut head[0], &mut mid[0], &mut tail[0])
                                    };
                                    let [label_red, label_green, label_blue] = background_labels;

                                    slider_red.value = bg[0];
                                    slider_green.value = bg[1];
                                    slider_blue.value = bg[2];

                                    let mut runs = [
                                        widget_ref(label_red),
                                        widget_ref(slider_red),
                                        widget_ref(label_green),
                                        widget_ref(slider_green),
                                        widget_ref(label_blue),
                                        widget_ref(slider_blue),
                                    ];
                                    container.row_widgets(results, &slider_row, SizePolicy::Auto, &mut runs);
                                    bg[0] = slider_red.value;
                                    bg[1] = slider_green.value;
                                    bg[2] = slider_blue.value;
                                });
                                let r: Recti = container.next_cell();
                                container.draw_rect(r, color(bg[0] as u8, bg[1] as u8, bg[2] as u8, 255));
                                let mut buff = String::new();
                                buff.push_str(format!("#{:02X}{:02X}{:02X}", bg[0] as u8, bg[1] as u8, bg[2] as u8).as_str());
                                container.draw_control_text(buff.as_str(), r, ControlColor::Text, WidgetOption::ALIGN_CENTER);
                            });
                        });
                    });

                    Self::section(tree, slot_header, |tree| {
                        tree.run(|container, results| {
                            let (slot0, slot1, slot2, slot3) = {
                                let (s0, rest) = slot_buttons.split_at_mut(1);
                                let (s1, rest) = rest.split_at_mut(1);
                                let (s2, s3) = rest.split_at_mut(1);
                                (&mut s0[0], &mut s1[0], &mut s2[0], &mut s3[0])
                            };
                            container.stack(SizePolicy::Fixed(67), |container| {
                                let mut runs = [widget_ref(slot0), widget_ref(slot1), widget_ref(slot2)];
                                container.widgets(results, &mut runs);
                                if let Some(button) = external_image_button.as_mut() {
                                    container.stack_with_width(SizePolicy::Fixed(256), SizePolicy::Fixed(256), |ctx| {
                                        let mut runs = [widget_ref(button)];
                                        ctx.widgets(results, &mut runs);
                                    });
                                }
                            });
                            container.stack(SizePolicy::Fixed(67), |container| {
                                let mut runs = [widget_ref(slot3)];
                                container.widgets(results, &mut runs);
                            });
                        });
                    });
                });
            }
            for msg in button_logs {
                self.write_log(msg);
            }
            for msg in tree_logs {
                self.write_log(msg);
            }
            WindowState::Open
        });

        if let Some(anchor) = combo_anchor {
            let combo_state = self.combo_state.as_mut().unwrap();
            let combo_items = &mut self.combo_items;
            let popup = &mut combo_state.popup;
            if combo_state.open {
                ctx.open_popup_at(popup, anchor);
            }

            ctx.popup(popup, WidgetBehaviourOption::NO_SCROLL, |dropdown, results| {
                dropdown.build_tree(results, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                        for item in combo_items.iter_mut() {
                            tree.widget(item);
                        }
                    });
                });
                for (idx, item) in combo_items.iter().enumerate() {
                    if results.state_of(item).is_submitted() {
                        combo_state.selected = idx;
                        combo_changed = true;
                        dropdown.set_focus(None);
                    }
                }
                if combo_changed { WindowState::Closed } else { WindowState::Open }
            });

            if !popup.is_open() {
                combo_state.open = false;
            }
        }

        if combo_changed {
            let selected = self.combo_state.as_ref().unwrap().selected;
            if let Some(choice) = self.combo_items.get(selected) {
                let msg = format!("Selected: {}", choice.label);
                self.write_log(msg.as_str());
            }
        }

        if self.open_popup {
            let popup_width = (self.style.default_cell_width + self.style.padding.max(0) * 2).max(80);
            let popup = self.popup_window.as_mut().unwrap();
            ctx.open_popup(popup);
            popup.set_size(&Dimensioni::new(popup_width, 1));
            self.open_popup = false;
        }

        let mut popup_logs: Vec<&'static str> = Vec::new();
        {
            let popup_buttons = &mut self.popup_buttons;
            ctx.popup(
                &mut self.popup_window.as_mut().unwrap().clone(),
                WidgetBehaviourOption::NO_SCROLL,
                |ctx, results| {
                    ctx.build_tree(results, |tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                            let (button0, button1) = {
                                let (head, tail) = popup_buttons.split_at_mut(1);
                                (&mut head[0], &mut tail[0])
                            };
                            tree.widget(button0);
                            tree.widget(button1);
                        });
                    });
                    if results.state_of(&popup_buttons[0]).is_submitted() {
                        popup_logs.push("Hello")
                    }
                    if results.state_of(&popup_buttons[1]).is_submitted() {
                        popup_logs.push("World")
                    }
                    WindowState::Open
                },
            );
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
        let mut state = State::new(backend, renderer, slots, ctx);
        state.demo_window = Some(ctx.new_window("Demo Window", rect(40, 40, 300, 450)));
        state.log_window = Some(ctx.new_window("Log Window", rect(350, 40, 300, 200)));
        state.style_window = Some(ctx.new_window("Style Editor", rect(350, 250, 300, 240)));
        state.popup_window = Some(ctx.new_popup("Test Popup"));
        state.combo_state = Some(Combo::new(ctx.new_popup("Combo Box Popup")));
        state.log_output = Some(ctx.new_panel("Log Output"));
        state.dialog_window = Some(FileDialogState::new(ctx));
        state.triangle_window = Some(ctx.new_window("Triangle Window", rect(200, 100, 200, 200)));
        state.suzane_window = Some(ctx.new_window("Suzane Window", rect(220, 220, 300, 300)));
        state.stack_direction_window = Some(ctx.new_window("Stack Direction Demo", rect(530, 40, 280, 220)));
        state.weight_window = Some(ctx.new_window("Weight Demo", rect(530, 270, 280, 260)));
        state
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
