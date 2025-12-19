#[path = "./common/mod.rs"]
mod common;

#[cfg(all(feature = "example-glow", feature = "example-vulkan"))]
compile_error!("Enable only one of `example-glow` or `example-vulkan` for demo-full.");
#[cfg(not(any(feature = "example-glow", feature = "example-vulkan")))]
compile_error!("Enable one of `example-glow` or `example-vulkan` to build demo-full.");

use common::{
    application::Application,
    application::BackendInitContext,
    atlas_assets,
    camera::Camera,
    obj_loader::Obj,
    polymesh::PolyMesh,
    view3d::View3D,
};
#[cfg(feature = "example-glow")]
use common::glow_renderer::{CustomRenderArea, GLRenderer as BackendRenderer, MeshBuffers, MeshSubmission, MeshVertex};
#[cfg(feature = "example-vulkan")]
use common::vulkan_renderer::{CustomRenderArea, MeshBuffers, MeshSubmission, MeshVertex, VulkanRenderer as BackendRenderer};
#[cfg(feature = "builder")]
use microui_redux::builder;
use microui_redux::*;
use rand::{rng, rngs::ThreadRng, Rng};
use std::{
    cell::RefCell,
    f32::consts::PI,
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
    rng: Rc<RefCell<ThreadRng>>,
    slots: Vec<SlotId>,
    image_texture: Option<TextureId>,
    label_colors: [LabelColor<'a>; 15],
    bg: [Real; 3],
    logbuf: String,
    logbuf_updated: bool,
    submit_buf: String,
    checks: [bool; 3],
    combo_state: Option<ComboState>,
    style: Style,

    demo_window: Option<WindowHandle>,
    style_window: Option<WindowHandle>,
    log_window: Option<WindowHandle>,
    popup_window: Option<WindowHandle>,
    log_output: Option<ContainerHandle>,
    triangle_window: Option<WindowHandle>,
    suzane_window: Option<WindowHandle>,
    dialog_window: Option<FileDialogState>,

    fps: f32,
    last_frame: Instant,

    window_header: NodeState,
    test_buttons_header: NodeState,
    background_header: NodeState,
    tree_and_text_header: NodeState,
    slot_header: NodeState,
    combo_header: NodeState,
    test1_tn: NodeState,
    test1a_tn: NodeState,
    test1b_tn: NodeState,
    test2_tn: NodeState,
    test3_tn: NodeState,
    open_popup: bool,
    open_dialog: bool,
    white_uv: Vec2f,
    triangle_data: Arc<RwLock<TriangleState>>,
    suzane_data: Arc<RwLock<SuzaneData>>,
}

impl<'a> State<'a> {
    pub fn new(_backend: BackendInitContext, renderer: RendererHandle<BackendRenderer>, slots: Vec<SlotId>, ctx: &mut Context<BackendRenderer>) -> Self {
        #[cfg(any(feature = "builder", feature = "png_source"))]
        let image_texture = ctx.load_image_from(ImageSource::Png { bytes: include_bytes!("./FACEPALM.png") }).ok();
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
        let pm_suzane = Obj::from_byte_stream(SUZANE).unwrap().to_polymesh();
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

        Self {
            renderer,
            rng: Rc::new(RefCell::new(rng())),
            slots,
            image_texture,
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
            logbuf: String::new(),
            logbuf_updated: false,
            submit_buf: String::new(),
            checks: [false, true, false],
            combo_state: None,
            style: Style::default(),
            demo_window: None,
            style_window: None,
            log_window: None,
            popup_window: None,
            log_output: None,
            triangle_window: None,
            suzane_window: None,
            dialog_window: None,
            fps: 0.0,
            last_frame: Instant::now(),
            window_header: NodeState::Closed,
            test_buttons_header: NodeState::Expanded,
            background_header: NodeState::Expanded,
            tree_and_text_header: NodeState::Expanded,
            slot_header: NodeState::Expanded,
            combo_header: NodeState::Expanded,
            test1_tn: NodeState::Closed,
            test1a_tn: NodeState::Closed,
            test1b_tn: NodeState::Closed,
            test2_tn: NodeState::Closed,
            test3_tn: NodeState::Closed,
            open_popup: false,
            open_dialog: false,
            white_uv,
            triangle_data,
            suzane_data,
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

    fn uint8_slider(&mut self, value: &mut u8, low: i32, high: i32, ctx: &mut Container) -> ResourceState {
        let mut tmp = *value as f32;
        ctx.idmngr.push_id_from_ptr(value);
        let res = ctx.slider_ex(&mut tmp, low as Real, high as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
        *value = tmp as u8;
        ctx.idmngr.pop_id();
        res
    }

    fn style_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        ctx.window(&mut self.style_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            let sw = (container.body.width as f64 * 0.14) as i32;
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
                        self.uint8_slider(&mut (*color).r, 0, 255, container);
                        self.uint8_slider(&mut (*color).g, 0, 255, container);
                        self.uint8_slider(&mut (*color).b, 0, 255, container);
                        self.uint8_slider(&mut (*color).a, 0, 255, container);
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
                let mut tmp = self.style.padding as u8;
                self.uint8_slider(&mut tmp, 0, 16, container);
                self.style.padding = tmp as i32;

                container.label("spacing");
                let mut tmp = self.style.spacing as u8;
                self.uint8_slider(&mut tmp, 0, 16, container);
                self.style.spacing = tmp as i32;

                container.label("title height");
                let mut tmp = self.style.title_height as u8;
                self.uint8_slider(&mut tmp, 0, 128, container);
                self.style.title_height = tmp as i32;

                container.label("thumb size");
                let mut tmp = self.style.thumb_size as u8;
                self.uint8_slider(&mut tmp, 0, 128, container);
                self.style.thumb_size = tmp as i32;

                container.label("scroll size");
                let mut tmp = self.style.scrollbar_size as u8;
                self.uint8_slider(&mut tmp, 0, 128, container);
                self.style.scrollbar_size = tmp as i32;
            });
            WindowState::Open
        });
        ctx.set_style(&self.style);
    }

    fn log_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        ctx.window(&mut self.log_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(24), |container| {
                container.panel(self.log_output.as_mut().unwrap(), ContainerOption::NONE, |container_handle| {
                    let container = &mut container_handle.inner_mut();
                    let mut scroll = container.scroll;
                    let content_size = container.content_size;
                    container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |container| {
                        container.text(self.logbuf.as_str());

                        if self.logbuf_updated {
                            scroll.y = content_size.y;
                            container.scroll = scroll;
                            self.logbuf_updated = false;
                        }
                    });
                });
            });
            let mut submitted = false;
            let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
            container.with_row(&submit_row, SizePolicy::Auto, |container| {
                if container.textbox_ex(&mut self.submit_buf, WidgetOption::NONE).is_submitted() {
                    container.set_focus(container.idmngr.last_id());
                    submitted = true;
                }
                if !container.button_ex("Submit", None, WidgetOption::ALIGN_CENTER).is_none() {
                    submitted = true;
                }
            });
            if submitted {
                let mut buf = String::new();
                buf.push_str(self.submit_buf.as_str());
                self.write_log(buf.as_str());
                self.submit_buf.clear();
            }
            WindowState::Open
        });
    }

    fn triangle_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.triangle_window.is_none() {
            return;
        }
        let mut renderer = self.renderer.clone();
        let tri_state = self.triangle_data.clone();
        let white_uv = self.white_uv;
        ctx.window(&mut self.triangle_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |container| {
                container.custom_render_widget("Triangle", WidgetOption::HOLD_FOCUS, move |_dim, cra| {
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
        });
    }

    fn suzane_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.suzane_window.is_none() {
            return;
        }
        let mut renderer = self.renderer.clone();
        let suzane_state = self.suzane_data.clone();
        ctx.window(&mut self.suzane_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0), |container| {
                container.custom_render_widget("Suzane", WidgetOption::HOLD_FOCUS, move |_dim, cra| {
                    if cra.content_area.width <= 0 || cra.content_area.height <= 0 {
                        return;
                    }
                    if let Ok(mut suzane) = suzane_state.write() {
                        suzane.view_3d.set_dimension(Dimensioni::new(cra.content_area.width, cra.content_area.height));
                        let _ = suzane.view_3d.update(cra.mouse_event);
                        if !matches!(cra.mouse_event, MouseEvent::Drag { .. } | MouseEvent::Scroll(_)) {
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
                                        suzane.view_3d.update(MouseEvent::Scroll(-0.5));
                                    }
                                    's' | 'S' => {
                                        suzane.view_3d.update(MouseEvent::Scroll(0.5));
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
        });
    }

    fn test_window(&mut self, ctx: &mut Context<BackendRenderer>) {
        let combo_items = ["Apple", "Banana", "Cherry", "Date"];
        let mut combo_anchor = None;
        let mut combo_changed = false;

        ctx.window(&mut self.demo_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            let mut win = container.rect;
            win.width = win.width.max(240);
            win.height = win.height.max(300);

            container.rect = win;

            let mut buff = String::new();

            self.window_header = container.header("Window Info", self.window_header, |container| {
                let win_0 = container.rect;
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
                    buff.push_str(format!("{:.1}", self.fps).as_str());
                    container.label(buff.as_str());
                });
            });
            self.test_buttons_header = container.header("Test Buttons", self.test_buttons_header, |container| {
                let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
                container.with_row(&button_widths, SizePolicy::Auto, |container| {
                    container.label("Test buttons 1:");
                    if !container.button_ex("Button 1", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.write_log("Pressed button 1");
                    }
                    if !container.button_ex("Button 2", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.write_log("Pressed button 2");
                    }
                    container.label("Test buttons 2:");
                    if !container.button_ex("Button 3", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.write_log("Pressed button 3");
                    }
                    if !container.button_ex("Popup", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.open_popup = true;
                    }

                    container.label("Test buttons 3:");
                    if !container.button_ex("Button 4", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.write_log("Pressed button 4");
                    }
                    if !container.button_ex("Dialog", None, WidgetOption::ALIGN_CENTER).is_none() {
                        self.open_dialog = true;
                    }
                });
            });
            self.combo_header = container.header("Combo Box", self.combo_header, |container| {
                let combo_state = self.combo_state.as_mut().unwrap();
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                    let (anchor, toggled, _) =
                        container.combo_box(combo_state, &combo_items, WidgetOption::NONE);
                    combo_anchor = Some(anchor);
                    if toggled {
                        combo_state.open = !combo_state.open;
                    }
                });
            });
            self.tree_and_text_header = container.header("Tree and Text", self.tree_and_text_header, |container| {
                let widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
                container.with_row(&widths, SizePolicy::Auto, |container| {
                    container.column(|container| {
                        self.test1_tn = container.treenode("Test 1", self.test1_tn, |container| {
                            self.test1a_tn = container.treenode("Test 1a", self.test1a_tn, |container| {
                                container.label("Hello");
                                container.label("world");
                            });
                            self.test1b_tn = container.treenode("Test 1b", self.test1b_tn, |container| {
                                if !container.button_ex("Button 1", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 1");
                                }
                                if !container.button_ex("Button 2", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 2");
                                }
                            });
                        });
                        self.test2_tn = container.treenode("Test 2", self.test2_tn, |container| {
                            let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
                            container.with_row(&tree_button_widths, SizePolicy::Auto, |container| {
                                if !container.button_ex("Button 3", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 3");
                                }
                                if !container.button_ex("Button 4", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 4");
                                }
                                if !container.button_ex("Button 5", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 5");
                                }
                                if !container.button_ex("Button 6", None, WidgetOption::ALIGN_CENTER).is_none() {
                                    self.write_log("Pressed button 6");
                                }
                            });
                        });
                        self.test3_tn = container.treenode("Test 3", self.test3_tn, |container| {
                            container.checkbox("Checkbox 1", &mut self.checks[0]);
                            container.checkbox("Checkbox 2", &mut self.checks[1]);
                            container.checkbox("Checkbox 3", &mut self.checks[2]);
                        });
                    });
                    container.column(|container| {
                        container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                            container.text_with_wrap(
                                "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.",
                                TextWrap::Word,
                            );
                        });
                    });
                });
            });
            self.background_header = container.header("Background Color", self.background_header, |container| {
                let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
                container.with_row(&background_widths, SizePolicy::Fixed(74), |container| {
                    let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
                    container.column(|container| {
                        container.with_row(&slider_row, SizePolicy::Auto, |container| {
                            container.label("Red:");
                            container.slider_ex(&mut self.bg[0], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                            container.label("Green:");
                            container.slider_ex(&mut self.bg[1], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                            container.label("Blue:");
                            container.slider_ex(&mut self.bg[2], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                        });
                    });
                    let r: Recti = container.next_cell();
                    container.draw_rect(r, color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255));
                    let mut buff = String::new();
                    buff.push_str(format!("#{:02X}{:02X}{:02X}", self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8).as_str());
                    container.draw_control_text(buff.as_str(), r, ControlColor::Text, WidgetOption::ALIGN_CENTER);
                });
            });

            self.slot_header = container.header("Slots", self.slot_header, |container| {
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(67), |container| {
                    container.button_ex2("Slot 1", Some(Image::Slot(self.slots[0].clone())), WidgetOption::NONE, WidgetFillOption::ALL);
                    container.button_ex3("Slot 2 - Green", Some(self.slots[1].clone()), WidgetOption::NONE, Rc::new(|_x, _y| {
                        color4b(0x00, 0xFF, 0x00, 0xFF)
                    }));
                    container.button_ex2("Slot 3", Some(Image::Slot(self.slots[2].clone())), WidgetOption::NONE, WidgetFillOption::ALL);
                    if let Some(texture) = self.image_texture {
                        container.with_row(&[SizePolicy::Fixed(256)], SizePolicy::Fixed(256), |ctx| {
                            ctx.button_ex2("External Image", Some(Image::Texture(texture)), WidgetOption::NONE, WidgetFillOption::ALL);
                        });
                    }
                });
                let rng = self.rng.clone();
                container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(67), |container| {
                    container.button_ex3(
                        "Slot 2 - Random",
                        Some(self.slots[1].clone()),
                        WidgetOption::NONE,
                        Rc::new(move |_x, _y| {
                            let mut rm = rng.borrow_mut();
                            color4b(rm.random(), rm.random(), rm.random(), rm.random())
                        }),
                    );
                });
            });
            WindowState::Open
        });

        if let Some(anchor) = combo_anchor {
            let combo_state = self.combo_state.as_mut().unwrap();
            let popup = &mut combo_state.popup;
            if combo_state.open {
                ctx.open_popup_at(popup, anchor);
            }

            ctx.popup(popup, |dropdown| {
                dropdown.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |dropdown| {
                    for (idx, item) in combo_items.iter().enumerate() {
                        if dropdown.list_item(item, WidgetOption::NONE).is_submitted() {
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
            if let Some(choice) = combo_items.get(self.combo_state.as_ref().unwrap().selected) {
                let msg = format!("Selected: {}", choice);
                self.write_log(msg.as_str());
            }
        }

        if self.open_popup {
            ctx.open_popup(self.popup_window.as_mut().unwrap());
            self.open_popup = false;
        }

        ctx.popup(&mut self.popup_window.as_mut().unwrap().clone(), |ctx| {
            if !ctx.button_ex("Hello", None, WidgetOption::ALIGN_CENTER).is_none() {
                self.write_log("Hello")
            }
            if !ctx.button_ex("World", None, WidgetOption::ALIGN_CENTER).is_none() {
                self.write_log("World")
            }
            WindowState::Open
        });

        self.dialog(ctx);
    }

    fn dialog(&mut self, ctx: &mut Context<BackendRenderer>) {
        if self.open_dialog {
            self.dialog_window.as_mut().unwrap().open(ctx);
            self.open_dialog = false;
            self.write_log("Open dialog!");
        }

        self.dialog_window.as_mut().unwrap().eval(ctx);
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
        state.combo_state = Some(ComboState::new(ctx.new_popup("Combo Box Popup")));
        state.log_output = Some(ctx.new_panel("Log Output"));
        state.dialog_window = Some(FileDialogState::new(ctx));
        state.triangle_window = Some(ctx.new_window("Triangle Window", rect(200, 100, 200, 200)));
        state.suzane_window = Some(ctx.new_window("Suzane Window", rect(220, 220, 300, 300)));
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

const SUZANE: &[u8; 63204] = include_bytes!("../assets/suzane.obj");
