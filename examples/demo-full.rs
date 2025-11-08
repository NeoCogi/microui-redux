#[path = "./common/mod.rs"]
mod common;

use common::*;
//

use application::*;
use camera::Camera;
use glow::{HasContext, NativeBuffer, NativeProgram, UniformLocation, COLOR_BUFFER_BIT, DEPTH_BUFFER_BIT};
use microui_redux::*;
use obj_loader::Obj;
use polymesh::PolyMesh;
use view3d::View3D;

use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use microui_redux::WindowHandle;
use rand::rngs::ThreadRng;
use rand::*;

pub use glow_renderer::{create_program, get_active_program_attributes, get_active_program_uniforms};

pub use glow_renderer::*;

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

const VERTEX_SHADER: &str = "#version 100
uniform highp mat4 uTransform;
attribute highp vec2 vertexPosition;
attribute lowp vec4 vertexColor;
varying lowp vec4 vVertexColor;
void main()
{
    vVertexColor = vertexColor;
    highp vec4 pos = vec4(vertexPosition.x, vertexPosition.y, 0.0, 1.0);
    gl_Position = uTransform * pos;
}";

const FRAGMENT_SHADER: &str = "#version 100
varying lowp vec4 vVertexColor;
void main()
{
    gl_FragColor = vVertexColor;
}";

pub struct TriangleRenderData {
    program: NativeProgram,
    vb: NativeBuffer,

    pos_attr: u32,
    color_attr: u32,
    tm_uni: UniformLocation,
    tm: Mat4f, // unneeded really, but it's just to illustrate how to be done (the matrix is function of the angle)
    angle: f32,
}

pub struct SuzaneData {
    view_3d: View3D,
    mesh: PolyMesh,
}

struct State<'a> {
    gl: Arc<glow::Context>,
    rng: Rc<RefCell<ThreadRng>>,
    slots: Vec<SlotId>,
    label_colors: [LabelColor<'a>; 15],
    bg: [Real; 3],
    logbuf: String,
    logbuf_updated: bool,
    submit_buf: String,
    checks: [bool; 3],
    style: Style,

    demo_window: Option<WindowHandle>,
    style_window: Option<WindowHandle>,
    log_window: Option<WindowHandle>,
    popup_window: Option<WindowHandle>,
    log_output: Option<ContainerHandle>,
    triangle_window: Option<WindowHandle>,
    suzane_window: Option<WindowHandle>,
    dialog_window: Option<FileDialogState>,

    window_header: NodeState,
    test_buttons_header: NodeState,
    background_header: NodeState,
    tree_and_text_header: NodeState,
    slot_header: NodeState,
    test1_tn: NodeState,
    test1a_tn: NodeState,
    test1b_tn: NodeState,
    test2_tn: NodeState,
    test3_tn: NodeState,
    open_popup: bool,
    open_dialog: bool,

    pm_renderer: Arc<RwLock<PolyMeshRenderer>>,
    suzane_data: Arc<RwLock<SuzaneData>>,
    triangle_data: Arc<RwLock<TriangleRenderData>>,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct LabelColor<'a> {
    pub label: &'a str,
    pub idx: ControlColor,
}

const MAX_POLYMESH_TRIS: usize = 65536;

impl<'a> State<'a> {
    pub fn new(gl: Arc<glow::Context>, slots: Vec<SlotId>) -> Self {
        let pm_renderer = Arc::new(RwLock::new(PolyMeshRenderer::create(&gl, MAX_POLYMESH_TRIS)));

        let program = create_program(&gl, VERTEX_SHADER, FRAGMENT_SHADER).unwrap();
        let attrs = get_active_program_attributes(&gl, program)
            .iter()
            .enumerate()
            .map(|(i, a)| (a.name.clone(), i as u32))
            .collect::<HashMap<_, _>>();

        let unis = get_active_program_uniforms(&gl, program)
            .iter()
            .map(|u| (u.name.clone(), unsafe { gl.get_uniform_location(program, &u.name).unwrap() }))
            .collect::<HashMap<_, _>>();
        let td = TriangleRenderData {
            program,
            vb: unsafe { gl.create_buffer().unwrap() },
            pos_attr: attrs["vertexPosition"],
            color_attr: attrs["vertexColor"],
            tm_uni: unis["uTransform"],
            tm: Mat4f::identity(),
            angle: 0.0,
        };

        let pm_suzane = Obj::from_byte_stream(SUZANE).unwrap().to_polymesh();
        let bounds = pm_suzane.calculate_bounding_box();
        Self {
            gl,
            rng: Rc::new(RefCell::new(thread_rng())),
            slots,
            style: Style::default(),
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

            demo_window: None,
            style_window: None,
            log_window: None,
            popup_window: None,
            log_output: None,
            triangle_window: None,
            suzane_window: None,
            dialog_window: None,

            window_header: NodeState::Closed,
            test_buttons_header: NodeState::Expanded,
            tree_and_text_header: NodeState::Expanded,
            background_header: NodeState::Expanded,
            slot_header: NodeState::Expanded,

            test1_tn: NodeState::Closed,
            test1a_tn: NodeState::Closed,
            test1b_tn: NodeState::Closed,
            test2_tn: NodeState::Closed,
            test3_tn: NodeState::Closed,
            open_popup: false,
            open_dialog: false,
            triangle_data: Arc::new(RwLock::new(td)),
            pm_renderer,
            suzane_data: Arc::new(RwLock::new(SuzaneData {
                mesh: pm_suzane,
                view_3d: View3D::new(
                    Camera::new(
                        bounds.center(),
                        bounds.max.length() * 3.0,
                        Quat::identity(), //Quat::of_axis_angle(&Vec3f::new(0.0, 1.0, 0.0), 0.0),
                        PI / 4.0,
                        1.0,
                        0.1,
                        bounds.max.length() * 10.0,
                    ),
                    Dimension::new(600, 600),
                    bounds,
                ),
            })),
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

    fn test_window(&mut self, ctx: &mut Context<GLRenderer>) {
        ctx.window(&mut self.demo_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            let mut win = container.rect;
            win.width = if win.width > 240 { win.width } else { 240 };
            win.height = if win.height > 300 { win.height } else { 300 };

            container.rect = win;

            let mut buff = String::new();

            self.window_header = container.header("Window Info", self.window_header, |container| {
                let win_0 = container.rect;
                let row_widths = [SizePolicy::Fixed(54), SizePolicy::Remainder(0)];
                container.set_row_widths_height(&row_widths, SizePolicy::Auto);
                container.label("Position:");

                buff.clear();
                buff.push_str(format!("{}, {}", win_0.x, win_0.y).as_str());

                container.label(buff.as_str());
                buff.clear();
                container.label("Size:");

                buff.push_str(format!("{}, {}", win_0.width, win_0.height).as_str());

                container.label(buff.as_str());
            });
            self.test_buttons_header = container.header("Test Buttons", self.test_buttons_header, |container| {
                let button_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(109), SizePolicy::Remainder(0)];
                container.set_row_widths_height(&button_widths, SizePolicy::Auto);
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
            self.tree_and_text_header = container.header("Tree and Text", self.tree_and_text_header, |container| {
                let widths = [SizePolicy::Fixed(140), SizePolicy::Remainder(0)];
                container.set_row_widths_height(&widths, SizePolicy::Auto);
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
                    self.test2_tn =container.treenode("Test 2", self.test2_tn, |container| {
                        let tree_button_widths = [SizePolicy::Fixed(54), SizePolicy::Fixed(54)];
                        container.set_row_widths_height(&tree_button_widths, SizePolicy::Auto);
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
                    self.test3_tn = container.treenode("Test 3", self.test3_tn, |container| {
                        container.checkbox("Checkbox 1", &mut self.checks[0]);
                        container.checkbox("Checkbox 2", &mut self.checks[1]);
                        container.checkbox("Checkbox 3", &mut self.checks[2]);
                    });
                });
                container.column(|container| {
                    container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Auto);
                    container.text(
                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla."
                        ,
                    );
                });
            });
            self.background_header = container.header("Background Color", self.background_header, |container| {
                let background_widths = [SizePolicy::Remainder(77), SizePolicy::Remainder(0)];
                container.set_row_widths_height(&background_widths, SizePolicy::Fixed(74));
                container.column(|container| {
                    let slider_row = [SizePolicy::Fixed(46), SizePolicy::Remainder(0)];
                    container.set_row_widths_height(&slider_row, SizePolicy::Auto);
                    container.label("Red:");
                    container.slider_ex(&mut self.bg[0], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                    container.label("Green:");
                    container.slider_ex(&mut self.bg[1], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                    container.label("Blue:");
                    container.slider_ex(&mut self.bg[2], 0 as Real, 255 as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
                });
                let r: Recti = container.next_cell();
                container.draw_rect(r, color(self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8, 255));
                let mut buff = String::new();
                buff.push_str(format!("#{:02X}{:02X}{:02X}", self.bg[0] as u8, self.bg[1] as u8, self.bg[2] as u8).as_str());
                container.draw_control_text(buff.as_str(), r, ControlColor::Text, WidgetOption::ALIGN_CENTER);
            });

            self.slot_header = container.header("Slots", self.slot_header, |container| {
                container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Fixed(67));
                container.button_ex2("Slot 1", Some(self.slots[0].clone()), WidgetOption::NONE);
                container.button_ex3("Slot 2 - Green", Some(self.slots[1].clone()), WidgetOption::NONE, Rc::new(|_x, _y| {
                    color4b(0x00, 0xFF, 0x00, 0xFF)
                }));
                container.button_ex2("Slot 3", Some(self.slots[2].clone()), WidgetOption::NONE);
                let rng = self.rng.clone();
                container.button_ex3("Slot 2 - Random", Some(self.slots[1].clone()), WidgetOption::NONE, Rc::new(move |_x, _y| {
                    let mut rm = rng.borrow_mut();
                    color4b(rm.gen(), rm.gen(), rm.gen(), rm.gen())
                }));


            });
            WindowState::Open
        });

        if self.open_popup {
            ctx.open_popup(self.popup_window.as_mut().unwrap());
            self.open_popup = !self.open_popup;
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

    fn log_window(&mut self, ctx: &mut Context<GLRenderer>) {
        ctx.window(&mut self.log_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(24));
            container.panel(self.log_output.as_mut().unwrap(), ContainerOption::NONE, |container_handle| {
                let container = &mut container_handle.inner_mut();
                let mut scroll = container.scroll;
                let content_size = container.content_size;
                container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0));

                container.text(self.logbuf.as_str());

                if self.logbuf_updated {
                    scroll.y = content_size.y;
                    container.scroll = scroll;
                    self.logbuf_updated = false;
                }
            });
            let mut submitted = false;
            let submit_row = [SizePolicy::Remainder(69), SizePolicy::Remainder(0)];
            container.set_row_widths_height(&submit_row, SizePolicy::Auto);
            if container.textbox_ex(&mut self.submit_buf, WidgetOption::NONE).is_submitted() {
                container.set_focus(container.idmngr.last_id());
                submitted = true;
            }
            if !container.button_ex("Submit", None, WidgetOption::ALIGN_CENTER).is_none() {
                submitted = true;
            }
            if submitted {
                let mut buf = String::new();
                buf.push_str(self.submit_buf.as_str());
                self.write_log(buf.as_str());
                self.submit_buf.clear();
            }
            WindowState::Open
        });
    }

    fn triangle_window(&mut self, ctx: &mut Context<GLRenderer>) {
        let gl = self.gl.clone();
        let tdi = self.triangle_data.clone();

        ctx.window(&mut self.triangle_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0));
            container.custom_render_widget("Triangle", WidgetOption::NONE, move |dim, cra| {
                let gl = &gl;

                match tdi.try_read() {
                    Ok(td) => unsafe {
                        gl.viewport(
                            cra.content_area.x,
                            dim.height - cra.content_area.y - cra.content_area.height,
                            cra.content_area.width,
                            cra.content_area.height,
                        );
                        gl.scissor(
                            cra.content_area.x,
                            dim.height - cra.content_area.y - cra.content_area.height,
                            cra.content_area.width,
                            cra.content_area.height,
                        );
                        gl.clear_color(0.5, 0.5, 0.5, 1.0);
                        gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);

                        gl.disable(glow::BLEND);
                        debug_assert!(gl.get_error() == 0);
                        gl.disable(glow::CULL_FACE);
                        debug_assert!(gl.get_error() == 0);
                        gl.enable(glow::DEPTH_TEST);
                        debug_assert!(gl.get_error() == 0);
                        gl.enable(glow::SCISSOR_TEST);
                        debug_assert!(gl.get_error() == 0);

                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(td.vb));
                        debug_assert!(gl.get_error() == 0);

                        // update the vertex buffer
                        let vertices_u8: &[u8] =
                            core::slice::from_raw_parts(TRI_VERTS.as_ptr() as *const u8, TRI_VERTS.len() * core::mem::size_of::<TriVertex>());
                        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertices_u8, glow::DYNAMIC_DRAW);
                        debug_assert!(gl.get_error() == 0);

                        gl.use_program(Some(td.program));

                        gl.enable_vertex_attrib_array(td.pos_attr);
                        gl.enable_vertex_attrib_array(td.color_attr);
                        debug_assert!(gl.get_error() == 0);

                        gl.vertex_attrib_pointer_f32(td.pos_attr, 2, glow::FLOAT, false, core::mem::size_of::<TriVertex>() as i32, 0);
                        gl.vertex_attrib_pointer_f32(td.color_attr, 4, glow::UNSIGNED_BYTE, true, core::mem::size_of::<TriVertex>() as i32, 8);
                        debug_assert!(gl.get_error() == 0);

                        let tm_ptr = td.tm.col.as_ptr() as *const _ as *const f32;
                        let slice = std::slice::from_raw_parts(tm_ptr, 16);
                        gl.uniform_matrix_4_f32_slice(Some(&td.tm_uni), false, &slice);
                        debug_assert_eq!(gl.get_error(), 0);

                        gl.draw_arrays(glow::TRIANGLES, 0, 3);
                        debug_assert!(gl.get_error() == 0);

                        gl.disable_vertex_attrib_array(td.pos_attr);
                        gl.disable_vertex_attrib_array(td.color_attr);
                        debug_assert!(gl.get_error() == 0);
                        gl.use_program(None);
                        debug_assert!(gl.get_error() == 0);
                    },
                    _ => println!("unable to read"),
                }

                match tdi.try_write() {
                    Ok(mut td) => {
                        td.angle += 0.01;
                        td.tm = rotation_from_axis_angle(&Vec3f::new(0.0, 0.0, 1.0), td.angle);
                    }
                    _ => {
                        if tdi.is_poisoned() {
                            println!("poisoned!");
                            tdi.clear_poison();
                        }
                        println!("failed to get lock")
                    }
                }
            });
            WindowState::Open
        });
    }

    fn uint8_slider(&mut self, value: &mut u8, low: i32, high: i32, ctx: &mut Container) -> ResourceState {
        let mut tmp = *value as f32;
        ctx.idmngr.push_id_from_ptr(value);
        let res = ctx.slider_ex(&mut tmp, low as Real, high as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
        *value = tmp as u8;
        ctx.idmngr.pop_id();
        return res;
    }

    fn style_window(&mut self, ctx: &mut Context<GLRenderer>) {
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
            container.set_row_widths_height(&color_row, SizePolicy::Auto);
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
            let metrics_row = [SizePolicy::Fixed(80), SizePolicy::Fixed(sw)];
            container.set_row_widths_height(&metrics_row, SizePolicy::Auto);
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
            WindowState::Open
        });
        ctx.set_style(&self.style);
    }

    fn suzane_window(&mut self, ctx: &mut Context<GLRenderer>) {
        let gl = self.gl.clone();
        let renderer = self.pm_renderer.clone();
        let suzane = self.suzane_data.clone();

        ctx.window(&mut self.suzane_window.as_mut().unwrap().clone(), ContainerOption::NONE, |container| {
            container.set_row_widths_height(&[SizePolicy::Remainder(0)], SizePolicy::Remainder(0));
            container.custom_render_widget("Suzane", WidgetOption::HOLD_FOCUS, move |dim, cra| {
                let gl = &gl;
                let mut suzane = suzane.write().unwrap();
                suzane.view_3d.set_dimension(Dimensioni::new(cra.content_area.width, cra.content_area.height));

                // if !cra.input.get_mouse_buttons().is_none() {
                //     println!("Mouse Pressed: {:?}", cra.input.rel_mouse_pos());
                // }
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
                }

                match renderer.try_write() {
                    Ok(mut renderer) => unsafe {
                        gl.viewport(
                            cra.content_area.x,
                            dim.height - cra.content_area.y - cra.content_area.height,
                            cra.content_area.width,
                            cra.content_area.height,
                        );
                        gl.scissor(
                            cra.content_area.x,
                            dim.height - cra.content_area.y - cra.content_area.height,
                            cra.content_area.width,
                            cra.content_area.height,
                        );
                        gl.clear_color(0.5, 0.5, 0.5, 1.0);
                        gl.clear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);

                        let pvm = suzane.view_3d.pvm();
                        let view = suzane.view_3d.view_matrix();
                        renderer.render(gl, &pvm, &view, &(&suzane.mesh));
                    },
                    _ => {
                        println!("unable to hold the lock on the polymesh")
                    }
                }
            });
            WindowState::Open
        });
    }

    fn dialog(&mut self, ctx: &mut Context<GLRenderer>) {
        if self.open_dialog {
            self.dialog_window.as_mut().unwrap().open(ctx);
            self.open_dialog = !self.open_dialog;
            self.write_log("Open dialog!");
        }

        self.dialog_window.as_mut().unwrap().eval(ctx);
    }

    fn process_frame(&mut self, ctx: &mut Context<GLRenderer>) {
        ctx.frame(|ctx| {
            self.style_window(ctx);
            self.log_window(ctx);
            self.test_window(ctx);
            self.triangle_window(ctx);
            self.suzane_window(ctx);
        })
    }
}

const SUZANE: &[u8; 63204] = include_bytes!("../assets/suzane.obj");
fn main() {
    let slots_orig = vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)];
    let mut atlas = builder::Builder::from_config(&atlas_config(&slots_orig)).unwrap().to_atlas();
    let slots = atlas.clone_slot_table();
    atlas.render_slot(slots[0], Rc::new(|_x, _y| color4b(0xFF, 0, 0, 0xFF)));
    atlas.render_slot(slots[1], Rc::new(|_x, _y| color4b(0, 0xFF, 0, 0xFF)));
    atlas.render_slot(slots[2], Rc::new(|_x, _y| color4b(0, 0, 0xFF, 0xFF)));
    builder::Builder::save_png_image(atlas.clone(), "atlas.png").unwrap();

    let mut fw = Application::new(atlas.clone(), move |gl, ctx| {
        let slots = atlas.clone_slot_table();
        let mut state = State::new(gl.clone(), slots);

        state.demo_window = Some(ctx.new_window("Demo Window", rect(40, 40, 300, 450)));
        state.log_window = Some(ctx.new_window("Log Window", rect(350, 40, 300, 200)));
        state.style_window = Some(ctx.new_window("Style Editor", rect(350, 250, 300, 240)));
        state.popup_window = Some(ctx.new_popup("Test Popup"));
        state.log_output = Some(ctx.new_panel("Log Outputman, "));
        state.triangle_window = Some(ctx.new_window("Triangle Window", rect(200, 100, 200, 200)));
        state.suzane_window = Some(ctx.new_window("Suzane Window", rect(220, 220, 300, 300)));
        state.dialog_window = Some(FileDialogState::new(ctx));

        state
    })
    .unwrap();

    fw.event_loop(|ctx, state| {
        state.process_frame(ctx);
    });
}
