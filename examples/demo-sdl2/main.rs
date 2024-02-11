extern crate sdl2;
mod atlas;
mod renderer;

use std::rc::Rc;

use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use sdl2::video::GLProfile;
use crate::{renderer::Renderer, renderer::MyAtlas};
use microui_redux::*;
use rs_math3d::*;

struct State<'a> {
    label_colors: [LabelColor<'a>; 15],
    bg: [Real; 3],
    logbuf: String,
    logbuf_updated: bool,
    submit_buf: String,
    checks: [bool; 3],
    style: Style,

    window_header: NodeState,
    test_buttons_header: NodeState,
    background_header: NodeState,
    tree_and_text_header: NodeState,
    test1_tn: NodeState,
    test1a_tn: NodeState,
    test1b_tn: NodeState,
    test2_tn: NodeState,
    test3_tn: NodeState,
    open_popup: bool,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct LabelColor<'a> {
    pub label: &'a str,
    pub idx: ControlColor,
}

impl<'a> State<'a> {
    pub fn new() -> Self {
        Self {
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

            window_header: NodeState::Closed,
            test_buttons_header: NodeState::Expanded,
            tree_and_text_header: NodeState::Expanded,
            background_header: NodeState::Expanded,

            test1_tn: NodeState::Closed,
            test1a_tn: NodeState::Closed,
            test1b_tn: NodeState::Closed,
            test2_tn: NodeState::Closed,
            test3_tn: NodeState::Closed,
            open_popup: false,
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

    fn test_window(&mut self, ctx: &mut microui_redux::Context) {
        ctx.window("Demo Window", rect(40, 40, 300, 450), WidgetOption::NONE, |container| {
            let mut win = container.rect;
            win.width = if win.width > 240 { win.width } else { 240 };
            win.height = if win.height > 300 { win.height } else { 300 };

            container.rect = win;

            let mut buff = String::new();

            self.window_header = container.header("Window Info", self.window_header, |container| {
                let win_0 = container.rect;
                container.set_row_widths_height(&[54, -1], 0);
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
                container.set_row_widths_height(&[86, -110, -1], 0);
                container.label("Test buttons 1:");
                if !container.button_ex("Button 1", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                    self.write_log("Pressed button 1");
                }
                if !container.button_ex("Button 2", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                    self.write_log("Pressed button 2");
                }
                container.label("Test buttons 2:");
                if !container.button_ex("Button 3", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                    self.write_log("Pressed button 3");
                }
                if !container.button_ex("Popup", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                     self.open_popup = true;
                }
            });
            self.tree_and_text_header = container.header("Tree and Text", self.tree_and_text_header, |container| {
                container.set_row_widths_height(&[140, -1], 0);
                container.column(|container| {
                    self.test1_tn = container.treenode("Test 1", self.test1_tn, |container| {
                        self.test1a_tn = container.treenode("Test 1a", self.test1a_tn, |container| {
                            container.label("Hello");
                            container.label("world");
                        });
                        self.test1b_tn = container.treenode("Test 1b", self.test1b_tn, |container| {
                            if !container.button_ex("Button 1", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                                self.write_log("Pressed button 1");
                            }
                            if !container.button_ex("Button 2", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                                self.write_log("Pressed button 2");
                            }
                        });
                    });
                    self.test2_tn =container.treenode("Test 2", self.test2_tn, |container| {
                        container.set_row_widths_height(&[54, 54], 0);
                        if !container.button_ex("Button 3", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                            self.write_log("Pressed button 3");
                        }
                        if !container.button_ex("Button 4", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                            self.write_log("Pressed button 4");
                        }
                        if !container.button_ex("Button 5", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                            self.write_log("Pressed button 5");
                        }
                        if !container.button_ex("Button 6", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
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
                    container.set_row_widths_height(&[-1], 0);
                    container.text(
                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla."
                        ,
                    );
                });
            });
            self.background_header = container.header("Background Color", self.background_header, |container| {
                container.set_row_widths_height(&[-78, -1], 74);
                container.column(|container| {
                    container.set_row_widths_height(&[46, -1], 0);
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
        });

        if self.open_popup {
            ctx.open_popup("Test Popup");
            self.open_popup = !self.open_popup;
        }
        ctx.popup("Test Popup", |ctx| {
            if !ctx.button_ex("Hello", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                self.write_log("Hello")
            }
            if !ctx.button_ex("World", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                self.write_log("World")
            }
        });
    }

    fn log_window(&mut self, ctx: &mut microui_redux::Context) {
        ctx.window("Log Window", rect(350, 40, 300, 200), WidgetOption::NONE, |container| {
            container.set_row_widths_height(&[-1], -25);
            container.panel("Log Output", WidgetOption::NONE, |container| {
                let mut scroll = container.scroll;
                let content_size = container.content_size;
                container.set_row_widths_height(&[-1], -1);

                container.text(self.logbuf.as_str());

                if self.logbuf_updated {
                    scroll.y = content_size.y;
                    container.scroll = scroll;
                    self.logbuf_updated = false;
                }
            });
            let mut submitted = false;
            container.set_row_widths_height(&[-70, -1], 0);
            if container.textbox_ex(&mut self.submit_buf, WidgetOption::NONE).is_submitted() {
                container.set_focus(container.idmngr.last_id());
                submitted = true;
            }
            if !container.button_ex("Submit", Icon::None, WidgetOption::ALIGN_CENTER).is_none() {
                submitted = true;
            }
            if submitted {
                let mut buf = String::new();
                buf.push_str(self.submit_buf.as_str());
                self.write_log(buf.as_str());
                self.submit_buf.clear();
            }
        });
    }
    fn uint8_slider(&mut self, value: &mut u8, low: i32, high: i32, ctx: &mut microui_redux::Container) -> ResourceState {
        let mut tmp = *value as f32;
        ctx.idmngr.push_id_from_ptr(value);
        let res = ctx.slider_ex(&mut tmp, low as Real, high as Real, 0 as Real, 0, WidgetOption::ALIGN_CENTER);
        *value = tmp as u8;
        ctx.idmngr.pop_id();
        return res;
    }
    fn style_window(&mut self, ctx: &mut microui_redux::Context) {
        ctx.window("Style Editor", rect(350, 250, 300, 240), WidgetOption::NONE, |container| {
            let sw = (container.body.width as f64 * 0.14) as i32;
            container.set_row_widths_height(&[80, sw, sw, sw, sw, -1], 0);
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
            container.set_row_widths_height(&[80, sw], 0);
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
        ctx.propagate_style(&self.style);
    }

    fn process_frame(&mut self, ctx: &mut microui_redux::Context) {
        ctx.frame(|ctx| {
            self.style_window(ctx);
            self.log_window(ctx);
            self.test_window(ctx);
        })
    }
}

fn main() {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let gl_attr = video_subsystem.gl_attr();
    gl_attr.set_context_profile(GLProfile::GLES);
    gl_attr.set_context_version(3, 0);

    let window = video_subsystem.window("Window", 800, 600).opengl().build().unwrap();

    // Unlike the other example above, nobody created a context for your window, so you need to create one.

    // TODO: the rust compiler optimizes this out
    let _x_ = window.gl_create_context().unwrap();
    let gl = unsafe { glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _) };

    debug_assert_eq!(gl_attr.context_profile(), GLProfile::GLES);
    debug_assert_eq!(gl_attr.context_version(), (3, 0));

    let mut event_pump = sdl_context.event_pump().unwrap();
    let (width, height) = window.size();
    let rd = Renderer::new(gl, &atlas::ATLAS_TEXTURE, width, height);

    let mut state = State::new();
    let mut ctx = microui_redux::Context::new(Rc::new(MyAtlas {}), Box::new(rd));

    'running: loop {
        let (width, height) = window.size();

        ctx.clear(width as i32, height as i32, color(state.bg[0] as u8, state.bg[1] as u8, state.bg[2] as u8, 255));

        fn map_mouse_button(sdl_mb: sdl2::mouse::MouseButton) -> microui_redux::MouseButton {
            match sdl_mb {
                sdl2::mouse::MouseButton::Left => microui_redux::MouseButton::LEFT,
                sdl2::mouse::MouseButton::Right => microui_redux::MouseButton::RIGHT,
                sdl2::mouse::MouseButton::Middle => microui_redux::MouseButton::MIDDLE,
                _ => microui_redux::MouseButton::NONE,
            }
        }

        fn map_keymode(sdl_km: sdl2::keyboard::Mod, sdl_kc: Option<sdl2::keyboard::Keycode>) -> microui_redux::KeyMode {
            match (sdl_km, sdl_kc) {
                (sdl2::keyboard::Mod::LALTMOD, _) | (sdl2::keyboard::Mod::RALTMOD, _) => microui_redux::KeyMode::ALT,
                (sdl2::keyboard::Mod::LCTRLMOD, _) | (sdl2::keyboard::Mod::RCTRLMOD, _) => microui_redux::KeyMode::CTRL,
                (sdl2::keyboard::Mod::LSHIFTMOD, _) | (sdl2::keyboard::Mod::RSHIFTMOD, _) => microui_redux::KeyMode::SHIFT,
                (_, Some(sdl2::keyboard::Keycode::Backspace)) => microui_redux::KeyMode::BACKSPACE,
                (_, Some(sdl2::keyboard::Keycode::Return)) => microui_redux::KeyMode::RETURN,
                _ => microui_redux::KeyMode::NONE,
            }
        }

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                Event::Window { win_event: WindowEvent::Close, .. } => break 'running,
                Event::MouseMotion { x, y, .. } => ctx.input.borrow_mut().mousemove(x, y),
                Event::MouseWheel { y, .. } => ctx.input.borrow_mut().scroll(0, y * -30),
                Event::MouseButtonDown { x, y, mouse_btn, .. } => {
                    let mb = map_mouse_button(mouse_btn);
                    ctx.input.borrow_mut().mousedown(x, y, mb);
                }
                Event::MouseButtonUp { x, y, mouse_btn, .. } => {
                    let mb = map_mouse_button(mouse_btn);
                    ctx.input.borrow_mut().mouseup(x, y, mb);
                }
                Event::KeyDown { keymod, keycode, .. } => {
                    let km = map_keymode(keymod, keycode);
                    ctx.input.borrow_mut().keydown(km);
                }
                Event::KeyUp { keymod, keycode, .. } => {
                    let km = map_keymode(keymod, keycode);
                    ctx.input.borrow_mut().keyup(km);
                }
                Event::TextInput { text, .. } => {
                    ctx.input.borrow_mut().text(text.as_str());
                }

                _ => {}
            }
        }

        state.process_frame(&mut ctx);

        ctx.flush();
        window.gl_swap_window();

        ::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
    }
}
