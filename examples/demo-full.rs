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

#[derive(Copy, Clone)]
#[repr(C)]
pub struct LabelColor<'a> {
    pub label: &'a str,
    pub idx: ControlColor,
}

struct State<'a> {
    renderer: RendererHandle<BackendRenderer>,
    label_colors: [LabelColor<'a>; 15],
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

impl<'a> State<'a> {
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
            label_colors: [
                LabelColor { label: "text", idx: ControlColor::Text },
                LabelColor {
                    label: "border:",
                    idx: ControlColor::Border,
                },
                LabelColor {
                    label: "windowbg:",
                    idx: ControlColor::WindowBG,
                },
                LabelColor {
                    label: "titlebg:",
                    idx: ControlColor::TitleBG,
                },
                LabelColor {
                    label: "titletext:",
                    idx: ControlColor::TitleText,
                },
                LabelColor {
                    label: "panelbg:",
                    idx: ControlColor::PanelBG,
                },
                LabelColor {
                    label: "button:",
                    idx: ControlColor::Button,
                },
                LabelColor {
                    label: "buttonhover:",
                    idx: ControlColor::ButtonHover,
                },
                LabelColor {
                    label: "buttonfocus:",
                    idx: ControlColor::ButtonFocus,
                },
                LabelColor { label: "base:", idx: ControlColor::Base },
                LabelColor {
                    label: "basehover:",
                    idx: ControlColor::BaseHover,
                },
                LabelColor {
                    label: "basefocus:",
                    idx: ControlColor::BaseFocus,
                },
                LabelColor {
                    label: "scrollbase:",
                    idx: ControlColor::ScrollBase,
                },
                LabelColor {
                    label: "scrollthumb:",
                    idx: ControlColor::ScrollThumb,
                },
                LabelColor { label: "", idx: ControlColor::Text },
            ],
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

    fn u8_slider(value: &mut u8, slider: &mut Slider, ctx: &mut Container) -> ResourceState {
        slider.value = *value as Real;
        let res = ctx.slider(slider);
        *value = slider.value as u8;
        slider.value = *value as Real;
        res
    }

    fn i32_slider(value: &mut i32, slider: &mut Slider, ctx: &mut Container) -> ResourceState {
        slider.value = *value as Real;
        let res = ctx.slider(slider);
        *value = slider.value as i32;
        slider.value = *value as Real;
        res
    }

    fn real_slider(value: &mut Real, slider: &mut Slider, ctx: &mut Container) -> ResourceState {
        slider.value = *value;
        let res = ctx.slider(slider);
        *value = slider.value;
        res
    }

    fn style_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        ctx.window(
            &mut self.style_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                let sw = (container.body().width as f64 * 0.14) as i32;
                let color_row = [
                    SizePolicy::Fixed(80),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Fixed(sw),
                    SizePolicy::Remainder(0),
                ];
                container.with_row(&color_row, SizePolicy::Auto, |container| {
                    let mut i = 0;
                    while self.label_colors[i].label.len() > 0 {
                        container.label(self.label_colors[i].label);
                        unsafe {
                            let color = self.style.colors.as_mut_ptr().offset(i as isize);
                            let slider_base = i * 4;
                            Self::u8_slider(&mut (*color).r, &mut self.style_color_sliders[slider_base], container);
                            Self::u8_slider(&mut (*color).g, &mut self.style_color_sliders[slider_base + 1], container);
                            Self::u8_slider(&mut (*color).b, &mut self.style_color_sliders[slider_base + 2], container);
                            Self::u8_slider(&mut (*color).a, &mut self.style_color_sliders[slider_base + 3], container);
                        }
                        let next_layout = container.next_cell();
                        let color = self.style.colors[i];
                        container.draw_rect(next_layout, color);
                        i += 1;
                    }
                });
                let metrics_row = [SizePolicy::Fixed(80), SizePolicy::Fixed(sw)];
                container.with_row(&metrics_row, SizePolicy::Auto, |container| {
                    container.label("padding");
                    Self::i32_slider(&mut self.style.padding, &mut self.style_value_sliders[0], container);

                    container.label("spacing");
                    Self::i32_slider(&mut self.style.spacing, &mut self.style_value_sliders[1], container);

                    container.label("title height");
                    Self::i32_slider(&mut self.style.title_height, &mut self.style_value_sliders[2], container);

                    container.label("thumb size");
                    Self::i32_slider(&mut self.style.thumb_size, &mut self.style_value_sliders[3], container);

                    container.label("scroll size");
                    Self::i32_slider(&mut self.style.scrollbar_size, &mut self.style_value_sliders[4], container);
                });
                WindowState::Open
            },
        );
        ctx.set_style(&self.style);
    }

    fn log_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        ctx.window(
            &mut self.log_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                container.stack(SizePolicy::Remainder(24), |container| {
                    container.panel(
                        self.log_output.as_mut().unwrap(),
                        ContainerOption::NONE,
                        WidgetBehaviourOption::NONE,
                        |container_handle| {
                            container_handle.with_mut(|container| {
                                let mut scroll = container.scroll();
                                let content_size = container.content_size();
                                container.stack(SizePolicy::Remainder(0), |container| {
                                    container.text(self.logbuf.as_str());

                                    if self.logbuf_updated {
                                        scroll.y = content_size.y;
                                        container.set_scroll(scroll);
                                        self.logbuf_updated = false;
                                    }
                                });
                            });
                        },
                    );
                });
                let mut submitted = false;
                let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
                container.with_row(&submit_row, SizePolicy::Auto, |container| {
                    if container.textbox(&mut self.submit_buf).is_submitted() {
                        container.set_focus(Some(widget_id_of(&self.submit_buf)));
                        submitted = true;
                    }
                    if container.button(&mut self.submit_button).is_submitted() {
                        submitted = true;
                    }
                });
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
        let mut renderer = self.renderer.clone();
        let tri_state = self.triangle_data.clone();
        let white_uv = self.white_uv;
        let triangle_widget = &mut self.triangle_widget;
        ctx.window(
            &mut self.triangle_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                container.stack(SizePolicy::Remainder(0), |container| {
                    container.custom_render_widget(triangle_widget, move |_dim, cra| {
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
                WindowState::Open
            },
        );
    }

    fn suzane_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.suzane_window.is_none() {
            return;
        }
        let mut renderer = self.renderer.clone();
        let suzane_state = self.suzane_data.clone();
        let suzane_widget = &mut self.suzane_widget;
        ctx.window(
            &mut self.suzane_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                container.stack(SizePolicy::Remainder(0), |container| {
                    container.custom_render_widget(suzane_widget, move |_dim, cra| {
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
                WindowState::Open
            },
        );
    }

    fn stack_direction_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.stack_direction_window.is_none() {
            return;
        }

        let buttons = &mut self.stack_direction_buttons;
        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.stack_direction_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                let spacing = container.get_style().spacing.max(0);
                let body_width = container.body().width.max(1);
                let left_width = body_width.saturating_sub(spacing).max(2) / 2;
                let columns = [SizePolicy::Fixed(left_width), SizePolicy::Remainder(0)];

                container.with_row(&columns, SizePolicy::Auto, |container| {
                    container.label("Top -> Bottom");
                    container.label("Bottom -> Top");
                });

                container.with_row(&columns, SizePolicy::Fixed(120), |container| {
                    container.column(|container| {
                        container.stack_direction(SizePolicy::Fixed(28), StackDirection::TopToBottom, |container| {
                            if container.button(&mut buttons[0]).is_submitted() {
                                logs.push("Top->Bottom: call 1");
                            }
                            if container.button(&mut buttons[1]).is_submitted() {
                                logs.push("Top->Bottom: call 2");
                            }
                            if container.button(&mut buttons[2]).is_submitted() {
                                logs.push("Top->Bottom: call 3");
                            }
                        });
                    });
                    container.column(|container| {
                        container.stack_direction(SizePolicy::Fixed(28), StackDirection::BottomToTop, |container| {
                            if container.button(&mut buttons[3]).is_submitted() {
                                logs.push("Bottom->Top: call 1");
                            }
                            if container.button(&mut buttons[4]).is_submitted() {
                                logs.push("Bottom->Top: call 2");
                            }
                            if container.button(&mut buttons[5]).is_submitted() {
                                logs.push("Bottom->Top: call 3");
                            }
                        });
                    });
                });
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
        let mut logs: Vec<&'static str> = Vec::new();
        ctx.window(
            &mut self.weight_window.as_mut().unwrap().clone(),
            ContainerOption::NONE,
            WidgetBehaviourOption::NONE,
            |container| {
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                    container.label("Row weights 1 : 2 : 3");
                });
                let row = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0), SizePolicy::Weight(3.0)];
                container.with_row(&row, SizePolicy::Fixed(28), |container| {
                    if container.button(&mut buttons[0]).is_submitted() {
                        logs.push("Weight row: 1");
                    }
                    if container.button(&mut buttons[1]).is_submitted() {
                        logs.push("Weight row: 2");
                    }
                    if container.button(&mut buttons[2]).is_submitted() {
                        logs.push("Weight row: 3");
                    }
                });

                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                    container.label("Grid weights rows 1 : 2");
                });
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |container| {
                    container.column(|container| {
                        let cols = [SizePolicy::Weight(1.0), SizePolicy::Weight(1.0), SizePolicy::Weight(1.0)];
                        let rows = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];
                        container.with_grid(&cols, &rows, |container| {
                            if container.button(&mut buttons[3]).is_submitted() {
                                logs.push("Weight grid: 1");
                            }
                            if container.button(&mut buttons[4]).is_submitted() {
                                logs.push("Weight grid: 2");
                            }
                            if container.button(&mut buttons[5]).is_submitted() {
                                logs.push("Weight grid: 3");
                            }
                            if container.button(&mut buttons[6]).is_submitted() {
                                logs.push("Weight grid: 4");
                            }
                            if container.button(&mut buttons[7]).is_submitted() {
                                logs.push("Weight grid: 5");
                            }
                            if container.button(&mut buttons[8]).is_submitted() {
                                logs.push("Weight grid: 6");
                            }
                        });
                    });
                });
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

        ctx.window(&mut self.demo_window.as_mut().unwrap().clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |container| {
            let mut win = container.rect();
            win.width = win.width.max(240);
            win.height = win.height.max(300);

            container.set_rect(win);

            let mut buff = String::new();
            let fps = self.fps;

            {
                let window_header = &mut self.window_header;
                container.header(window_header, |container| {
                    let win_0 = container.rect();
                    let row_widths = [SizePolicy::Fixed(54), SizePolicy::Remainder(0)];
                    container.with_row(&row_widths, SizePolicy::Auto, |container| {
                        container.label("Position:");

                        buff.clear();
                        buff.push_str(format!("{}, {}", win_0.x, win_0.y).as_str());

                        container.label(buff.as_str());
                        buff.clear();
                        container.label("Size:");

                        buff.push_str(format!("{}, {}", win_0.width, win_0.height).as_str());

                        container.label(buff.as_str());
                    });
                    container.with_row(&row_widths, SizePolicy::Auto, |container| {
                        container.label("FPS:");
                        buff.clear();
                        buff.push_str(format!("{:.1}", fps).as_str());
                        container.label(buff.as_str());
                    });
                });
            }
            let mut button_logs: Vec<&'static str> = Vec::new();
            {
                let test_buttons_header = &mut self.test_buttons_header;
                let test_buttons = &mut self.test_buttons;
                let open_popup = &mut self.open_popup;
                let open_dialog = &mut self.open_dialog;
                container.header(test_buttons_header, |container| {
                    let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
                    container.with_row(&button_widths, SizePolicy::Auto, |container| {
                        container.label("Test buttons 1:");
                        if container.button(&mut test_buttons[0]).is_submitted() {
                            button_logs.push("Pressed button 1");
                        }
                        if container.button(&mut test_buttons[1]).is_submitted() {
                            button_logs.push("Pressed button 2");
                        }
                        container.label("Test buttons 2:");
                        if container.button(&mut test_buttons[2]).is_submitted() {
                            button_logs.push("Pressed button 3");
                        }
                        if container.button(&mut test_buttons[3]).is_submitted() {
                            *open_popup = true;
                        }

                        container.label("Test buttons 3:");
                        if container.button(&mut test_buttons[4]).is_submitted() {
                            button_logs.push("Pressed button 4");
                        }
                        if container.button(&mut test_buttons[5]).is_submitted() {
                            *open_dialog = true;
                        }
                    });
                });
            }
            for msg in button_logs {
                self.write_log(msg);
            }

            {
                let combo_header = &mut self.combo_header;
                let combo_state = self.combo_state.as_mut().unwrap();
                let combo_labels = [
                    self.combo_items[0].label.as_str(),
                    self.combo_items[1].label.as_str(),
                    self.combo_items[2].label.as_str(),
                    self.combo_items[3].label.as_str(),
                ];
                container.header(combo_header, |container| {
                    container.stack(SizePolicy::Auto, |container| {
                        let (anchor, toggled, _) = container.combo_box(combo_state, &combo_labels);
                        combo_anchor = Some(anchor);
                        if toggled {
                            combo_state.open = !combo_state.open;
                        }
                    });
                });
            }

            let mut tree_logs: Vec<&'static str> = Vec::new();
            {
                let tree_and_text_header = &mut self.tree_and_text_header;
                let test1_tn = &mut self.test1_tn;
                let test1a_tn = &mut self.test1a_tn;
                let test1b_tn = &mut self.test1b_tn;
                let test2_tn = &mut self.test2_tn;
                let test3_tn = &mut self.test3_tn;
                let tree_buttons = &mut self.tree_buttons;
                let checkboxes = &mut self.checkboxes;
                container.header(tree_and_text_header, |container| {
                    let widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
                    container.with_row(&widths, SizePolicy::Auto, |container| {
                        container.column(|container| {
                            container.treenode(test1_tn, |container| {
                                container.treenode(test1a_tn, |container| {
                                    container.label("Hello");
                                    container.label("world");
                                });
                                container.treenode(test1b_tn, |container| {
                                    if container.button(&mut tree_buttons[0]).is_submitted() {
                                        tree_logs.push("Pressed button 1");
                                    }
                                    if container.button(&mut tree_buttons[1]).is_submitted() {
                                        tree_logs.push("Pressed button 2");
                                    }
                                });
                            });
                            container.treenode(test2_tn, |container| {
                                let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
                                container.with_row(&tree_button_widths, SizePolicy::Auto, |container| {
                                    if container.button(&mut tree_buttons[2]).is_submitted() {
                                        tree_logs.push("Pressed button 3");
                                    }
                                    if container.button(&mut tree_buttons[3]).is_submitted() {
                                        tree_logs.push("Pressed button 4");
                                    }
                                    if container.button(&mut tree_buttons[4]).is_submitted() {
                                        tree_logs.push("Pressed button 5");
                                    }
                                    if container.button(&mut tree_buttons[5]).is_submitted() {
                                        tree_logs.push("Pressed button 6");
                                    }
                                });
                            });
                            container.treenode(test3_tn, |container| {
                                container.checkbox(&mut checkboxes[0]);
                                container.checkbox(&mut checkboxes[1]);
                                container.checkbox(&mut checkboxes[2]);
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
            }
            for msg in tree_logs {
                self.write_log(msg);
            }

            {
                let text_area_header = &mut self.text_area_header;
                let text_area = &mut self.text_area;
                container.header(text_area_header, |container| {
                    container.stack(SizePolicy::Fixed(120), |container| {
                        container.textarea(text_area);
                    });
                });
            }

            {
                let background_header = &mut self.background_header;
                let bg = &mut self.bg;
                let bg_sliders = &mut self.bg_sliders;
                container.header(background_header, |container| {
                    let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
                    container.with_row(&background_widths, SizePolicy::Fixed(74), |container| {
                        let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
                        container.column(|container| {
                            container.with_row(&slider_row, SizePolicy::Auto, |container| {
                                container.label("Red:");
                                Self::real_slider(&mut bg[0], &mut bg_sliders[0], container);
                                container.label("Green:");
                                Self::real_slider(&mut bg[1], &mut bg_sliders[1], container);
                                container.label("Blue:");
                                Self::real_slider(&mut bg[2], &mut bg_sliders[2], container);
                            });
                        });
                        let r: Recti = container.next_cell();
                        container.draw_rect(r, color(bg[0] as u8, bg[1] as u8, bg[2] as u8, 255));
                        let mut buff = String::new();
                        buff.push_str(format!("#{:02X}{:02X}{:02X}", bg[0] as u8, bg[1] as u8, bg[2] as u8).as_str());
                        container.draw_control_text(buff.as_str(), r, ControlColor::Text, WidgetOption::ALIGN_CENTER);
                    });
                });
            }

            {
                let slot_header = &mut self.slot_header;
                let slot_buttons = &mut self.slot_buttons;
                let external_image_button = &mut self.external_image_button;
                container.header(slot_header, |container| {
                    container.stack(SizePolicy::Fixed(67), |container| {
                        container.button(&mut slot_buttons[0]);
                        container.button(&mut slot_buttons[1]);
                        container.button(&mut slot_buttons[2]);
                        if let Some(button) = external_image_button.as_mut() {
                            container.stack_with_width(SizePolicy::Fixed(256), SizePolicy::Fixed(256), |ctx| {
                                ctx.button(button);
                            });
                        }
                    });
                    container.stack(SizePolicy::Fixed(67), |container| {
                        container.button(&mut slot_buttons[3]);
                    });
                });
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

            ctx.popup(popup, WidgetBehaviourOption::NO_SCROLL, |dropdown| {
                dropdown.stack(SizePolicy::Auto, |dropdown| {
                    for (idx, item) in combo_items.iter_mut().enumerate() {
                        if dropdown.list_item(item).is_submitted() {
                            combo_state.selected = idx;
                            combo_changed = true;
                            dropdown.set_focus(None);
                        }
                    }
                });
                if combo_changed {
                    WindowState::Closed
                } else {
                    WindowState::Open
                }
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
            ctx.popup(&mut self.popup_window.as_mut().unwrap().clone(), WidgetBehaviourOption::NO_SCROLL, |ctx| {
                ctx.stack(SizePolicy::Auto, |ctx| {
                    if ctx.button(&mut popup_buttons[0]).is_submitted() {
                        popup_logs.push("Hello")
                    }
                    if ctx.button(&mut popup_buttons[1]).is_submitted() {
                        popup_logs.push("World")
                    }
                });
                WindowState::Open
            });
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
