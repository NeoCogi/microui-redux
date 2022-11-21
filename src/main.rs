#![allow(dead_code)]
#![allow(mutable_transmutes)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unused_assignments)]
#![allow(unused_mut)]

//#![no_std]

use core::alloc::*;

extern crate libc;
#[path = "./microui.rs"]
pub mod microui;
#[path = "./renderer.rs"]
pub mod renderer;
pub type SDL_SysWMmsg = libc::c_int;

//use ::libc;
extern "C" {
    fn strlen(_: *const libc::c_char) -> libc::c_ulong;
    fn strcat(_: *mut libc::c_char, _: *const libc::c_char) -> *mut libc::c_char;
    fn exit(_: libc::c_int) -> !;
    fn malloc(_: libc::c_ulong) -> *mut libc::c_void;
    fn sprintf(_: *mut libc::c_char, _: *const libc::c_char, _: ...) -> libc::c_int;
    fn SDL_Init(flags: Uint32) -> libc::c_int;
    fn SDL_PollEvent(event: *mut SDL_Event) -> libc::c_int;
    fn mu_rect(
        x: libc::c_int,
        y: libc::c_int,
        w: libc::c_int,
        h: libc::c_int,
    ) -> mu_Rect;
    fn mu_color(
        r: libc::c_int,
        g: libc::c_int,
        b: libc::c_int,
        a: libc::c_int,
    ) -> mu_Color;
    fn mu_init(ctx: *mut mu_Context);
    fn mu_begin(ctx: *mut mu_Context);
    fn mu_end(ctx: *mut mu_Context);
    fn mu_set_focus(ctx: *mut mu_Context, id: mu_Id);
    fn mu_push_id(ctx: *mut mu_Context, data: *const libc::c_void, size: libc::c_int);
    fn mu_pop_id(ctx: *mut mu_Context);
    fn mu_get_current_container(ctx: *mut mu_Context) -> *mut mu_Container;
    fn mu_input_mousemove(ctx: *mut mu_Context, x: libc::c_int, y: libc::c_int);
    fn mu_input_mousedown(
        ctx: *mut mu_Context,
        x: libc::c_int,
        y: libc::c_int,
        btn: libc::c_int,
    );
    fn mu_input_mouseup(
        ctx: *mut mu_Context,
        x: libc::c_int,
        y: libc::c_int,
        btn: libc::c_int,
    );
    fn mu_input_scroll(ctx: *mut mu_Context, x: libc::c_int, y: libc::c_int);
    fn mu_input_keydown(ctx: *mut mu_Context, key: libc::c_int);
    fn mu_input_keyup(ctx: *mut mu_Context, key: libc::c_int);
    fn mu_input_text(ctx: *mut mu_Context, text: *const libc::c_char);
    fn mu_next_command(ctx: *mut mu_Context, cmd: *mut *mut mu_Command) -> libc::c_int;
    fn mu_draw_rect(ctx: *mut mu_Context, rect: mu_Rect, color: mu_Color);
    fn mu_layout_row(
        ctx: *mut mu_Context,
        items: libc::c_int,
        widths: *const libc::c_int,
        height: libc::c_int,
    );
    fn mu_layout_begin_column(ctx: *mut mu_Context);
    fn mu_layout_end_column(ctx: *mut mu_Context);
    fn mu_layout_next(ctx: *mut mu_Context) -> mu_Rect;
    fn mu_draw_control_text(
        ctx: *mut mu_Context,
        str: *const libc::c_char,
        rect: mu_Rect,
        colorid: libc::c_int,
        opt: libc::c_int,
    );
    fn mu_text(ctx: *mut mu_Context, text: *const libc::c_char);
    fn mu_label(ctx: *mut mu_Context, text: *const libc::c_char);
    fn mu_button_ex(
        ctx: *mut mu_Context,
        label: *const libc::c_char,
        icon: libc::c_int,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_checkbox(
        ctx: *mut mu_Context,
        label: *const libc::c_char,
        state: *mut libc::c_int,
    ) -> libc::c_int;
    fn mu_textbox_ex(
        ctx: *mut mu_Context,
        buf: *mut libc::c_char,
        bufsz: libc::c_int,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_slider_ex(
        ctx: *mut mu_Context,
        value: *mut mu_Real,
        low: mu_Real,
        high: mu_Real,
        step: mu_Real,
        fmt: *const libc::c_char,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_header_ex(
        ctx: *mut mu_Context,
        label: *const libc::c_char,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_begin_treenode_ex(
        ctx: *mut mu_Context,
        label: *const libc::c_char,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_end_treenode(ctx: *mut mu_Context);
    fn mu_begin_window_ex(
        ctx: *mut mu_Context,
        title: *const libc::c_char,
        rect: mu_Rect,
        opt: libc::c_int,
    ) -> libc::c_int;
    fn mu_end_window(ctx: *mut mu_Context);
    fn mu_open_popup(ctx: *mut mu_Context, name: *const libc::c_char);
    fn mu_begin_popup(ctx: *mut mu_Context, name: *const libc::c_char) -> libc::c_int;
    fn mu_end_popup(ctx: *mut mu_Context);
    fn mu_begin_panel_ex(
        ctx: *mut mu_Context,
        name: *const libc::c_char,
        opt: libc::c_int,
    );
    fn r_get_text_width(text: *const libc::c_char, len: libc::c_int) -> libc::c_int;
    fn r_present();
    fn mu_end_panel(ctx: *mut mu_Context);
    fn r_init();
    fn r_draw_rect(rect: mu_Rect, color: mu_Color);
    fn r_draw_text(text: *const libc::c_char, pos: mu_Vec2, color: mu_Color);
    fn r_draw_icon(id: libc::c_int, rect: mu_Rect, color: mu_Color);
    fn r_get_text_height() -> libc::c_int;
    fn r_set_clip_rect(rect: mu_Rect);
    fn r_clear(color: mu_Color);
}
pub type __uint8_t = libc::c_uchar;
pub type __int16_t = libc::c_short;
pub type __uint16_t = libc::c_ushort;
pub type __int32_t = libc::c_int;
pub type __uint32_t = libc::c_uint;
pub type __int64_t = libc::c_long;
pub type int16_t = __int16_t;
pub type int32_t = __int32_t;
pub type int64_t = __int64_t;
pub type uint8_t = __uint8_t;
pub type uint16_t = __uint16_t;
pub type uint32_t = __uint32_t;
pub type Uint8 = uint8_t;
pub type Sint16 = int16_t;
pub type Uint16 = uint16_t;
pub type Sint32 = int32_t;
pub type Uint32 = uint32_t;
pub type Sint64 = int64_t;
pub type SDL_Scancode = libc::c_uint;
pub const SDL_NUM_SCANCODES: SDL_Scancode = 512;
pub const SDL_SCANCODE_AUDIOFASTFORWARD: SDL_Scancode = 286;
pub const SDL_SCANCODE_AUDIOREWIND: SDL_Scancode = 285;
pub const SDL_SCANCODE_APP2: SDL_Scancode = 284;
pub const SDL_SCANCODE_APP1: SDL_Scancode = 283;
pub const SDL_SCANCODE_SLEEP: SDL_Scancode = 282;
pub const SDL_SCANCODE_EJECT: SDL_Scancode = 281;
pub const SDL_SCANCODE_KBDILLUMUP: SDL_Scancode = 280;
pub const SDL_SCANCODE_KBDILLUMDOWN: SDL_Scancode = 279;
pub const SDL_SCANCODE_KBDILLUMTOGGLE: SDL_Scancode = 278;
pub const SDL_SCANCODE_DISPLAYSWITCH: SDL_Scancode = 277;
pub const SDL_SCANCODE_BRIGHTNESSUP: SDL_Scancode = 276;
pub const SDL_SCANCODE_BRIGHTNESSDOWN: SDL_Scancode = 275;
pub const SDL_SCANCODE_AC_BOOKMARKS: SDL_Scancode = 274;
pub const SDL_SCANCODE_AC_REFRESH: SDL_Scancode = 273;
pub const SDL_SCANCODE_AC_STOP: SDL_Scancode = 272;
pub const SDL_SCANCODE_AC_FORWARD: SDL_Scancode = 271;
pub const SDL_SCANCODE_AC_BACK: SDL_Scancode = 270;
pub const SDL_SCANCODE_AC_HOME: SDL_Scancode = 269;
pub const SDL_SCANCODE_AC_SEARCH: SDL_Scancode = 268;
pub const SDL_SCANCODE_COMPUTER: SDL_Scancode = 267;
pub const SDL_SCANCODE_CALCULATOR: SDL_Scancode = 266;
pub const SDL_SCANCODE_MAIL: SDL_Scancode = 265;
pub const SDL_SCANCODE_WWW: SDL_Scancode = 264;
pub const SDL_SCANCODE_MEDIASELECT: SDL_Scancode = 263;
pub const SDL_SCANCODE_AUDIOMUTE: SDL_Scancode = 262;
pub const SDL_SCANCODE_AUDIOPLAY: SDL_Scancode = 261;
pub const SDL_SCANCODE_AUDIOSTOP: SDL_Scancode = 260;
pub const SDL_SCANCODE_AUDIOPREV: SDL_Scancode = 259;
pub const SDL_SCANCODE_AUDIONEXT: SDL_Scancode = 258;
pub const SDL_SCANCODE_MODE: SDL_Scancode = 257;
pub const SDL_SCANCODE_RGUI: SDL_Scancode = 231;
pub const SDL_SCANCODE_RALT: SDL_Scancode = 230;
pub const SDL_SCANCODE_RSHIFT: SDL_Scancode = 229;
pub const SDL_SCANCODE_RCTRL: SDL_Scancode = 228;
pub const SDL_SCANCODE_LGUI: SDL_Scancode = 227;
pub const SDL_SCANCODE_LALT: SDL_Scancode = 226;
pub const SDL_SCANCODE_LSHIFT: SDL_Scancode = 225;
pub const SDL_SCANCODE_LCTRL: SDL_Scancode = 224;
pub const SDL_SCANCODE_KP_HEXADECIMAL: SDL_Scancode = 221;
pub const SDL_SCANCODE_KP_DECIMAL: SDL_Scancode = 220;
pub const SDL_SCANCODE_KP_OCTAL: SDL_Scancode = 219;
pub const SDL_SCANCODE_KP_BINARY: SDL_Scancode = 218;
pub const SDL_SCANCODE_KP_CLEARENTRY: SDL_Scancode = 217;
pub const SDL_SCANCODE_KP_CLEAR: SDL_Scancode = 216;
pub const SDL_SCANCODE_KP_PLUSMINUS: SDL_Scancode = 215;
pub const SDL_SCANCODE_KP_MEMDIVIDE: SDL_Scancode = 214;
pub const SDL_SCANCODE_KP_MEMMULTIPLY: SDL_Scancode = 213;
pub const SDL_SCANCODE_KP_MEMSUBTRACT: SDL_Scancode = 212;
pub const SDL_SCANCODE_KP_MEMADD: SDL_Scancode = 211;
pub const SDL_SCANCODE_KP_MEMCLEAR: SDL_Scancode = 210;
pub const SDL_SCANCODE_KP_MEMRECALL: SDL_Scancode = 209;
pub const SDL_SCANCODE_KP_MEMSTORE: SDL_Scancode = 208;
pub const SDL_SCANCODE_KP_EXCLAM: SDL_Scancode = 207;
pub const SDL_SCANCODE_KP_AT: SDL_Scancode = 206;
pub const SDL_SCANCODE_KP_SPACE: SDL_Scancode = 205;
pub const SDL_SCANCODE_KP_HASH: SDL_Scancode = 204;
pub const SDL_SCANCODE_KP_COLON: SDL_Scancode = 203;
pub const SDL_SCANCODE_KP_DBLVERTICALBAR: SDL_Scancode = 202;
pub const SDL_SCANCODE_KP_VERTICALBAR: SDL_Scancode = 201;
pub const SDL_SCANCODE_KP_DBLAMPERSAND: SDL_Scancode = 200;
pub const SDL_SCANCODE_KP_AMPERSAND: SDL_Scancode = 199;
pub const SDL_SCANCODE_KP_GREATER: SDL_Scancode = 198;
pub const SDL_SCANCODE_KP_LESS: SDL_Scancode = 197;
pub const SDL_SCANCODE_KP_PERCENT: SDL_Scancode = 196;
pub const SDL_SCANCODE_KP_POWER: SDL_Scancode = 195;
pub const SDL_SCANCODE_KP_XOR: SDL_Scancode = 194;
pub const SDL_SCANCODE_KP_F: SDL_Scancode = 193;
pub const SDL_SCANCODE_KP_E: SDL_Scancode = 192;
pub const SDL_SCANCODE_KP_D: SDL_Scancode = 191;
pub const SDL_SCANCODE_KP_C: SDL_Scancode = 190;
pub const SDL_SCANCODE_KP_B: SDL_Scancode = 189;
pub const SDL_SCANCODE_KP_A: SDL_Scancode = 188;
pub const SDL_SCANCODE_KP_BACKSPACE: SDL_Scancode = 187;
pub const SDL_SCANCODE_KP_TAB: SDL_Scancode = 186;
pub const SDL_SCANCODE_KP_RIGHTBRACE: SDL_Scancode = 185;
pub const SDL_SCANCODE_KP_LEFTBRACE: SDL_Scancode = 184;
pub const SDL_SCANCODE_KP_RIGHTPAREN: SDL_Scancode = 183;
pub const SDL_SCANCODE_KP_LEFTPAREN: SDL_Scancode = 182;
pub const SDL_SCANCODE_CURRENCYSUBUNIT: SDL_Scancode = 181;
pub const SDL_SCANCODE_CURRENCYUNIT: SDL_Scancode = 180;
pub const SDL_SCANCODE_DECIMALSEPARATOR: SDL_Scancode = 179;
pub const SDL_SCANCODE_THOUSANDSSEPARATOR: SDL_Scancode = 178;
pub const SDL_SCANCODE_KP_000: SDL_Scancode = 177;
pub const SDL_SCANCODE_KP_00: SDL_Scancode = 176;
pub const SDL_SCANCODE_EXSEL: SDL_Scancode = 164;
pub const SDL_SCANCODE_CRSEL: SDL_Scancode = 163;
pub const SDL_SCANCODE_CLEARAGAIN: SDL_Scancode = 162;
pub const SDL_SCANCODE_OPER: SDL_Scancode = 161;
pub const SDL_SCANCODE_OUT: SDL_Scancode = 160;
pub const SDL_SCANCODE_SEPARATOR: SDL_Scancode = 159;
pub const SDL_SCANCODE_RETURN2: SDL_Scancode = 158;
pub const SDL_SCANCODE_PRIOR: SDL_Scancode = 157;
pub const SDL_SCANCODE_CLEAR: SDL_Scancode = 156;
pub const SDL_SCANCODE_CANCEL: SDL_Scancode = 155;
pub const SDL_SCANCODE_SYSREQ: SDL_Scancode = 154;
pub const SDL_SCANCODE_ALTERASE: SDL_Scancode = 153;
pub const SDL_SCANCODE_LANG9: SDL_Scancode = 152;
pub const SDL_SCANCODE_LANG8: SDL_Scancode = 151;
pub const SDL_SCANCODE_LANG7: SDL_Scancode = 150;
pub const SDL_SCANCODE_LANG6: SDL_Scancode = 149;
pub const SDL_SCANCODE_LANG5: SDL_Scancode = 148;
pub const SDL_SCANCODE_LANG4: SDL_Scancode = 147;
pub const SDL_SCANCODE_LANG3: SDL_Scancode = 146;
pub const SDL_SCANCODE_LANG2: SDL_Scancode = 145;
pub const SDL_SCANCODE_LANG1: SDL_Scancode = 144;
pub const SDL_SCANCODE_INTERNATIONAL9: SDL_Scancode = 143;
pub const SDL_SCANCODE_INTERNATIONAL8: SDL_Scancode = 142;
pub const SDL_SCANCODE_INTERNATIONAL7: SDL_Scancode = 141;
pub const SDL_SCANCODE_INTERNATIONAL6: SDL_Scancode = 140;
pub const SDL_SCANCODE_INTERNATIONAL5: SDL_Scancode = 139;
pub const SDL_SCANCODE_INTERNATIONAL4: SDL_Scancode = 138;
pub const SDL_SCANCODE_INTERNATIONAL3: SDL_Scancode = 137;
pub const SDL_SCANCODE_INTERNATIONAL2: SDL_Scancode = 136;
pub const SDL_SCANCODE_INTERNATIONAL1: SDL_Scancode = 135;
pub const SDL_SCANCODE_KP_EQUALSAS400: SDL_Scancode = 134;
pub const SDL_SCANCODE_KP_COMMA: SDL_Scancode = 133;
pub const SDL_SCANCODE_VOLUMEDOWN: SDL_Scancode = 129;
pub const SDL_SCANCODE_VOLUMEUP: SDL_Scancode = 128;
pub const SDL_SCANCODE_MUTE: SDL_Scancode = 127;
pub const SDL_SCANCODE_FIND: SDL_Scancode = 126;
pub const SDL_SCANCODE_PASTE: SDL_Scancode = 125;
pub const SDL_SCANCODE_COPY: SDL_Scancode = 124;
pub const SDL_SCANCODE_CUT: SDL_Scancode = 123;
pub const SDL_SCANCODE_UNDO: SDL_Scancode = 122;
pub const SDL_SCANCODE_AGAIN: SDL_Scancode = 121;
pub const SDL_SCANCODE_STOP: SDL_Scancode = 120;
pub const SDL_SCANCODE_SELECT: SDL_Scancode = 119;
pub const SDL_SCANCODE_MENU: SDL_Scancode = 118;
pub const SDL_SCANCODE_HELP: SDL_Scancode = 117;
pub const SDL_SCANCODE_EXECUTE: SDL_Scancode = 116;
pub const SDL_SCANCODE_F24: SDL_Scancode = 115;
pub const SDL_SCANCODE_F23: SDL_Scancode = 114;
pub const SDL_SCANCODE_F22: SDL_Scancode = 113;
pub const SDL_SCANCODE_F21: SDL_Scancode = 112;
pub const SDL_SCANCODE_F20: SDL_Scancode = 111;
pub const SDL_SCANCODE_F19: SDL_Scancode = 110;
pub const SDL_SCANCODE_F18: SDL_Scancode = 109;
pub const SDL_SCANCODE_F17: SDL_Scancode = 108;
pub const SDL_SCANCODE_F16: SDL_Scancode = 107;
pub const SDL_SCANCODE_F15: SDL_Scancode = 106;
pub const SDL_SCANCODE_F14: SDL_Scancode = 105;
pub const SDL_SCANCODE_F13: SDL_Scancode = 104;
pub const SDL_SCANCODE_KP_EQUALS: SDL_Scancode = 103;
pub const SDL_SCANCODE_POWER: SDL_Scancode = 102;
pub const SDL_SCANCODE_APPLICATION: SDL_Scancode = 101;
pub const SDL_SCANCODE_NONUSBACKSLASH: SDL_Scancode = 100;
pub const SDL_SCANCODE_KP_PERIOD: SDL_Scancode = 99;
pub const SDL_SCANCODE_KP_0: SDL_Scancode = 98;
pub const SDL_SCANCODE_KP_9: SDL_Scancode = 97;
pub const SDL_SCANCODE_KP_8: SDL_Scancode = 96;
pub const SDL_SCANCODE_KP_7: SDL_Scancode = 95;
pub const SDL_SCANCODE_KP_6: SDL_Scancode = 94;
pub const SDL_SCANCODE_KP_5: SDL_Scancode = 93;
pub const SDL_SCANCODE_KP_4: SDL_Scancode = 92;
pub const SDL_SCANCODE_KP_3: SDL_Scancode = 91;
pub const SDL_SCANCODE_KP_2: SDL_Scancode = 90;
pub const SDL_SCANCODE_KP_1: SDL_Scancode = 89;
pub const SDL_SCANCODE_KP_ENTER: SDL_Scancode = 88;
pub const SDL_SCANCODE_KP_PLUS: SDL_Scancode = 87;
pub const SDL_SCANCODE_KP_MINUS: SDL_Scancode = 86;
pub const SDL_SCANCODE_KP_MULTIPLY: SDL_Scancode = 85;
pub const SDL_SCANCODE_KP_DIVIDE: SDL_Scancode = 84;
pub const SDL_SCANCODE_NUMLOCKCLEAR: SDL_Scancode = 83;
pub const SDL_SCANCODE_UP: SDL_Scancode = 82;
pub const SDL_SCANCODE_DOWN: SDL_Scancode = 81;
pub const SDL_SCANCODE_LEFT: SDL_Scancode = 80;
pub const SDL_SCANCODE_RIGHT: SDL_Scancode = 79;
pub const SDL_SCANCODE_PAGEDOWN: SDL_Scancode = 78;
pub const SDL_SCANCODE_END: SDL_Scancode = 77;
pub const SDL_SCANCODE_DELETE: SDL_Scancode = 76;
pub const SDL_SCANCODE_PAGEUP: SDL_Scancode = 75;
pub const SDL_SCANCODE_HOME: SDL_Scancode = 74;
pub const SDL_SCANCODE_INSERT: SDL_Scancode = 73;
pub const SDL_SCANCODE_PAUSE: SDL_Scancode = 72;
pub const SDL_SCANCODE_SCROLLLOCK: SDL_Scancode = 71;
pub const SDL_SCANCODE_PRINTSCREEN: SDL_Scancode = 70;
pub const SDL_SCANCODE_F12: SDL_Scancode = 69;
pub const SDL_SCANCODE_F11: SDL_Scancode = 68;
pub const SDL_SCANCODE_F10: SDL_Scancode = 67;
pub const SDL_SCANCODE_F9: SDL_Scancode = 66;
pub const SDL_SCANCODE_F8: SDL_Scancode = 65;
pub const SDL_SCANCODE_F7: SDL_Scancode = 64;
pub const SDL_SCANCODE_F6: SDL_Scancode = 63;
pub const SDL_SCANCODE_F5: SDL_Scancode = 62;
pub const SDL_SCANCODE_F4: SDL_Scancode = 61;
pub const SDL_SCANCODE_F3: SDL_Scancode = 60;
pub const SDL_SCANCODE_F2: SDL_Scancode = 59;
pub const SDL_SCANCODE_F1: SDL_Scancode = 58;
pub const SDL_SCANCODE_CAPSLOCK: SDL_Scancode = 57;
pub const SDL_SCANCODE_SLASH: SDL_Scancode = 56;
pub const SDL_SCANCODE_PERIOD: SDL_Scancode = 55;
pub const SDL_SCANCODE_COMMA: SDL_Scancode = 54;
pub const SDL_SCANCODE_GRAVE: SDL_Scancode = 53;
pub const SDL_SCANCODE_APOSTROPHE: SDL_Scancode = 52;
pub const SDL_SCANCODE_SEMICOLON: SDL_Scancode = 51;
pub const SDL_SCANCODE_NONUSHASH: SDL_Scancode = 50;
pub const SDL_SCANCODE_BACKSLASH: SDL_Scancode = 49;
pub const SDL_SCANCODE_RIGHTBRACKET: SDL_Scancode = 48;
pub const SDL_SCANCODE_LEFTBRACKET: SDL_Scancode = 47;
pub const SDL_SCANCODE_EQUALS: SDL_Scancode = 46;
pub const SDL_SCANCODE_MINUS: SDL_Scancode = 45;
pub const SDL_SCANCODE_SPACE: SDL_Scancode = 44;
pub const SDL_SCANCODE_TAB: SDL_Scancode = 43;
pub const SDL_SCANCODE_BACKSPACE: SDL_Scancode = 42;
pub const SDL_SCANCODE_ESCAPE: SDL_Scancode = 41;
pub const SDL_SCANCODE_RETURN: SDL_Scancode = 40;
pub const SDL_SCANCODE_0: SDL_Scancode = 39;
pub const SDL_SCANCODE_9: SDL_Scancode = 38;
pub const SDL_SCANCODE_8: SDL_Scancode = 37;
pub const SDL_SCANCODE_7: SDL_Scancode = 36;
pub const SDL_SCANCODE_6: SDL_Scancode = 35;
pub const SDL_SCANCODE_5: SDL_Scancode = 34;
pub const SDL_SCANCODE_4: SDL_Scancode = 33;
pub const SDL_SCANCODE_3: SDL_Scancode = 32;
pub const SDL_SCANCODE_2: SDL_Scancode = 31;
pub const SDL_SCANCODE_1: SDL_Scancode = 30;
pub const SDL_SCANCODE_Z: SDL_Scancode = 29;
pub const SDL_SCANCODE_Y: SDL_Scancode = 28;
pub const SDL_SCANCODE_X: SDL_Scancode = 27;
pub const SDL_SCANCODE_W: SDL_Scancode = 26;
pub const SDL_SCANCODE_V: SDL_Scancode = 25;
pub const SDL_SCANCODE_U: SDL_Scancode = 24;
pub const SDL_SCANCODE_T: SDL_Scancode = 23;
pub const SDL_SCANCODE_S: SDL_Scancode = 22;
pub const SDL_SCANCODE_R: SDL_Scancode = 21;
pub const SDL_SCANCODE_Q: SDL_Scancode = 20;
pub const SDL_SCANCODE_P: SDL_Scancode = 19;
pub const SDL_SCANCODE_O: SDL_Scancode = 18;
pub const SDL_SCANCODE_N: SDL_Scancode = 17;
pub const SDL_SCANCODE_M: SDL_Scancode = 16;
pub const SDL_SCANCODE_L: SDL_Scancode = 15;
pub const SDL_SCANCODE_K: SDL_Scancode = 14;
pub const SDL_SCANCODE_J: SDL_Scancode = 13;
pub const SDL_SCANCODE_I: SDL_Scancode = 12;
pub const SDL_SCANCODE_H: SDL_Scancode = 11;
pub const SDL_SCANCODE_G: SDL_Scancode = 10;
pub const SDL_SCANCODE_F: SDL_Scancode = 9;
pub const SDL_SCANCODE_E: SDL_Scancode = 8;
pub const SDL_SCANCODE_D: SDL_Scancode = 7;
pub const SDL_SCANCODE_C: SDL_Scancode = 6;
pub const SDL_SCANCODE_B: SDL_Scancode = 5;
pub const SDL_SCANCODE_A: SDL_Scancode = 4;
pub const SDL_SCANCODE_UNKNOWN: SDL_Scancode = 0;
pub type SDL_Keycode = Sint32;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_Keysym {
    pub scancode: SDL_Scancode,
    pub sym: SDL_Keycode,
    pub mod_0: Uint16,
    pub unused: Uint32,
}
pub type SDL_JoystickID = Sint32;
pub type SDL_TouchID = Sint64;
pub type SDL_FingerID = Sint64;
pub type SDL_GestureID = Sint64;
pub type C2RustUnnamed = libc::c_uint;
pub const SDL_LASTEVENT: C2RustUnnamed = 65535;
pub const SDL_USEREVENT: C2RustUnnamed = 32768;
pub const SDL_RENDER_DEVICE_RESET: C2RustUnnamed = 8193;
pub const SDL_RENDER_TARGETS_RESET: C2RustUnnamed = 8192;
pub const SDL_SENSORUPDATE: C2RustUnnamed = 4608;
pub const SDL_AUDIODEVICEREMOVED: C2RustUnnamed = 4353;
pub const SDL_AUDIODEVICEADDED: C2RustUnnamed = 4352;
pub const SDL_DROPCOMPLETE: C2RustUnnamed = 4099;
pub const SDL_DROPBEGIN: C2RustUnnamed = 4098;
pub const SDL_DROPTEXT: C2RustUnnamed = 4097;
pub const SDL_DROPFILE: C2RustUnnamed = 4096;
pub const SDL_CLIPBOARDUPDATE: C2RustUnnamed = 2304;
pub const SDL_MULTIGESTURE: C2RustUnnamed = 2050;
pub const SDL_DOLLARRECORD: C2RustUnnamed = 2049;
pub const SDL_DOLLARGESTURE: C2RustUnnamed = 2048;
pub const SDL_FINGERMOTION: C2RustUnnamed = 1794;
pub const SDL_FINGERUP: C2RustUnnamed = 1793;
pub const SDL_FINGERDOWN: C2RustUnnamed = 1792;
pub const SDL_CONTROLLERDEVICEREMAPPED: C2RustUnnamed = 1621;
pub const SDL_CONTROLLERDEVICEREMOVED: C2RustUnnamed = 1620;
pub const SDL_CONTROLLERDEVICEADDED: C2RustUnnamed = 1619;
pub const SDL_CONTROLLERBUTTONUP: C2RustUnnamed = 1618;
pub const SDL_CONTROLLERBUTTONDOWN: C2RustUnnamed = 1617;
pub const SDL_CONTROLLERAXISMOTION: C2RustUnnamed = 1616;
pub const SDL_JOYDEVICEREMOVED: C2RustUnnamed = 1542;
pub const SDL_JOYDEVICEADDED: C2RustUnnamed = 1541;
pub const SDL_JOYBUTTONUP: C2RustUnnamed = 1540;
pub const SDL_JOYBUTTONDOWN: C2RustUnnamed = 1539;
pub const SDL_JOYHATMOTION: C2RustUnnamed = 1538;
pub const SDL_JOYBALLMOTION: C2RustUnnamed = 1537;
pub const SDL_JOYAXISMOTION: C2RustUnnamed = 1536;
pub const SDL_MOUSEWHEEL: C2RustUnnamed = 1027;
pub const SDL_MOUSEBUTTONUP: C2RustUnnamed = 1026;
pub const SDL_MOUSEBUTTONDOWN: C2RustUnnamed = 1025;
pub const SDL_MOUSEMOTION: C2RustUnnamed = 1024;
pub const SDL_KEYMAPCHANGED: C2RustUnnamed = 772;
pub const SDL_TEXTINPUT: C2RustUnnamed = 771;
pub const SDL_TEXTEDITING: C2RustUnnamed = 770;
pub const SDL_KEYUP: C2RustUnnamed = 769;
pub const SDL_KEYDOWN: C2RustUnnamed = 768;
pub const SDL_SYSWMEVENT: C2RustUnnamed = 513;
pub const SDL_WINDOWEVENT: C2RustUnnamed = 512;
pub const SDL_DISPLAYEVENT: C2RustUnnamed = 336;
pub const SDL_APP_DIDENTERFOREGROUND: C2RustUnnamed = 262;
pub const SDL_APP_WILLENTERFOREGROUND: C2RustUnnamed = 261;
pub const SDL_APP_DIDENTERBACKGROUND: C2RustUnnamed = 260;
pub const SDL_APP_WILLENTERBACKGROUND: C2RustUnnamed = 259;
pub const SDL_APP_LOWMEMORY: C2RustUnnamed = 258;
pub const SDL_APP_TERMINATING: C2RustUnnamed = 257;
pub const SDL_QUIT: C2RustUnnamed = 256;
pub const SDL_FIRSTEVENT: C2RustUnnamed = 0;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_CommonEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_DisplayEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub display: Uint32,
    pub event: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub data1: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_WindowEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub event: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub data1: Sint32,
    pub data2: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_KeyboardEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub state: Uint8,
    pub repeat: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub keysym: SDL_Keysym,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_TextEditingEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub text: [libc::c_char; 32],
    pub start: Sint32,
    pub length: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_TextInputEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub text: [libc::c_char; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_MouseMotionEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub which: Uint32,
    pub state: Uint32,
    pub x: Sint32,
    pub y: Sint32,
    pub xrel: Sint32,
    pub yrel: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_MouseButtonEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub which: Uint32,
    pub button: Uint8,
    pub state: Uint8,
    pub clicks: Uint8,
    pub padding1: Uint8,
    pub x: Sint32,
    pub y: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_MouseWheelEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub which: Uint32,
    pub x: Sint32,
    pub y: Sint32,
    pub direction: Uint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_JoyAxisEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub axis: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub value: Sint16,
    pub padding4: Uint16,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_JoyBallEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub ball: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub xrel: Sint16,
    pub yrel: Sint16,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_JoyHatEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub hat: Uint8,
    pub value: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_JoyButtonEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub button: Uint8,
    pub state: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_JoyDeviceEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_ControllerAxisEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub axis: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
    pub value: Sint16,
    pub padding4: Uint16,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_ControllerButtonEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: SDL_JoystickID,
    pub button: Uint8,
    pub state: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_ControllerDeviceEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: Sint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_AudioDeviceEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: Uint32,
    pub iscapture: Uint8,
    pub padding1: Uint8,
    pub padding2: Uint8,
    pub padding3: Uint8,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_TouchFingerEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub touchId: SDL_TouchID,
    pub fingerId: SDL_FingerID,
    pub x: libc::c_float,
    pub y: libc::c_float,
    pub dx: libc::c_float,
    pub dy: libc::c_float,
    pub pressure: libc::c_float,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_MultiGestureEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub touchId: SDL_TouchID,
    pub dTheta: libc::c_float,
    pub dDist: libc::c_float,
    pub x: libc::c_float,
    pub y: libc::c_float,
    pub numFingers: Uint16,
    pub padding: Uint16,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_DollarGestureEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub touchId: SDL_TouchID,
    pub gestureId: SDL_GestureID,
    pub numFingers: Uint32,
    pub error: libc::c_float,
    pub x: libc::c_float,
    pub y: libc::c_float,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_DropEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub file: *mut libc::c_char,
    pub windowID: Uint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_SensorEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub which: Sint32,
    pub data: [libc::c_float; 6],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_QuitEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_UserEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub windowID: Uint32,
    pub code: Sint32,
    pub data1: *mut libc::c_void,
    pub data2: *mut libc::c_void,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SDL_SysWMEvent {
    pub type_0: Uint32,
    pub timestamp: Uint32,
    pub msg: *mut SDL_SysWMmsg,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub union SDL_Event {
    pub type_0: Uint32,
    pub common: SDL_CommonEvent,
    pub display: SDL_DisplayEvent,
    pub window: SDL_WindowEvent,
    pub key: SDL_KeyboardEvent,
    pub edit: SDL_TextEditingEvent,
    pub text: SDL_TextInputEvent,
    pub motion: SDL_MouseMotionEvent,
    pub button: SDL_MouseButtonEvent,
    pub wheel: SDL_MouseWheelEvent,
    pub jaxis: SDL_JoyAxisEvent,
    pub jball: SDL_JoyBallEvent,
    pub jhat: SDL_JoyHatEvent,
    pub jbutton: SDL_JoyButtonEvent,
    pub jdevice: SDL_JoyDeviceEvent,
    pub caxis: SDL_ControllerAxisEvent,
    pub cbutton: SDL_ControllerButtonEvent,
    pub cdevice: SDL_ControllerDeviceEvent,
    pub adevice: SDL_AudioDeviceEvent,
    pub sensor: SDL_SensorEvent,
    pub quit: SDL_QuitEvent,
    pub user: SDL_UserEvent,
    pub syswm: SDL_SysWMEvent,
    pub tfinger: SDL_TouchFingerEvent,
    pub mgesture: SDL_MultiGestureEvent,
    pub dgesture: SDL_DollarGestureEvent,
    pub drop: SDL_DropEvent,
    pub padding: [Uint8; 56],
}
pub type C2RustUnnamed_0 = libc::c_uint;
pub const MU_COMMAND_MAX: C2RustUnnamed_0 = 6;
pub const MU_COMMAND_ICON: C2RustUnnamed_0 = 5;
pub const MU_COMMAND_TEXT: C2RustUnnamed_0 = 4;
pub const MU_COMMAND_RECT: C2RustUnnamed_0 = 3;
pub const MU_COMMAND_CLIP: C2RustUnnamed_0 = 2;
pub const MU_COMMAND_JUMP: C2RustUnnamed_0 = 1;
pub type C2RustUnnamed_1 = libc::c_uint;
pub const MU_COLOR_MAX: C2RustUnnamed_1 = 14;
pub const MU_COLOR_SCROLLTHUMB: C2RustUnnamed_1 = 13;
pub const MU_COLOR_SCROLLBASE: C2RustUnnamed_1 = 12;
pub const MU_COLOR_BASEFOCUS: C2RustUnnamed_1 = 11;
pub const MU_COLOR_BASEHOVER: C2RustUnnamed_1 = 10;
pub const MU_COLOR_BASE: C2RustUnnamed_1 = 9;
pub const MU_COLOR_BUTTONFOCUS: C2RustUnnamed_1 = 8;
pub const MU_COLOR_BUTTONHOVER: C2RustUnnamed_1 = 7;
pub const MU_COLOR_BUTTON: C2RustUnnamed_1 = 6;
pub const MU_COLOR_PANELBG: C2RustUnnamed_1 = 5;
pub const MU_COLOR_TITLETEXT: C2RustUnnamed_1 = 4;
pub const MU_COLOR_TITLEBG: C2RustUnnamed_1 = 3;
pub const MU_COLOR_WINDOWBG: C2RustUnnamed_1 = 2;
pub const MU_COLOR_BORDER: C2RustUnnamed_1 = 1;
pub const MU_COLOR_TEXT: C2RustUnnamed_1 = 0;
pub type C2RustUnnamed_2 = libc::c_uint;
pub const MU_RES_CHANGE: C2RustUnnamed_2 = 4;
pub const MU_RES_SUBMIT: C2RustUnnamed_2 = 2;
pub const MU_RES_ACTIVE: C2RustUnnamed_2 = 1;
pub type C2RustUnnamed_3 = libc::c_uint;
pub const MU_OPT_EXPANDED: C2RustUnnamed_3 = 4096;
pub const MU_OPT_CLOSED: C2RustUnnamed_3 = 2048;
pub const MU_OPT_POPUP: C2RustUnnamed_3 = 1024;
pub const MU_OPT_AUTOSIZE: C2RustUnnamed_3 = 512;
pub const MU_OPT_HOLDFOCUS: C2RustUnnamed_3 = 256;
pub const MU_OPT_NOTITLE: C2RustUnnamed_3 = 128;
pub const MU_OPT_NOCLOSE: C2RustUnnamed_3 = 64;
pub const MU_OPT_NOSCROLL: C2RustUnnamed_3 = 32;
pub const MU_OPT_NORESIZE: C2RustUnnamed_3 = 16;
pub const MU_OPT_NOFRAME: C2RustUnnamed_3 = 8;
pub const MU_OPT_NOINTERACT: C2RustUnnamed_3 = 4;
pub const MU_OPT_ALIGNRIGHT: C2RustUnnamed_3 = 2;
pub const MU_OPT_ALIGNCENTER: C2RustUnnamed_3 = 1;
pub type C2RustUnnamed_4 = libc::c_uint;
pub const MU_MOUSE_MIDDLE: C2RustUnnamed_4 = 4;
pub const MU_MOUSE_RIGHT: C2RustUnnamed_4 = 2;
pub const MU_MOUSE_LEFT: C2RustUnnamed_4 = 1;
pub type C2RustUnnamed_5 = libc::c_uint;
pub const MU_KEY_RETURN: C2RustUnnamed_5 = 16;
pub const MU_KEY_BACKSPACE: C2RustUnnamed_5 = 8;
pub const MU_KEY_ALT: C2RustUnnamed_5 = 4;
pub const MU_KEY_CTRL: C2RustUnnamed_5 = 2;
pub const MU_KEY_SHIFT: C2RustUnnamed_5 = 1;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Context {
    pub text_width: Option::<
        unsafe extern "C" fn(mu_Font, *const libc::c_char, libc::c_int) -> libc::c_int,
    >,
    pub text_height: Option::<unsafe extern "C" fn(mu_Font) -> libc::c_int>,
    pub draw_frame: Option::<
        unsafe extern "C" fn(*mut mu_Context, mu_Rect, libc::c_int) -> (),
    >,
    pub _style: mu_Style,
    pub style: *mut mu_Style,
    pub hover: mu_Id,
    pub focus: mu_Id,
    pub last_id: mu_Id,
    pub last_rect: mu_Rect,
    pub last_zindex: libc::c_int,
    pub updated_focus: libc::c_int,
    pub frame: libc::c_int,
    pub hover_root: *mut mu_Container,
    pub next_hover_root: *mut mu_Container,
    pub scroll_target: *mut mu_Container,
    pub number_edit_buf: [libc::c_char; 127],
    pub number_edit: mu_Id,
    pub command_list: C2RustUnnamed_12,
    pub root_list: C2RustUnnamed_11,
    pub container_stack: C2RustUnnamed_10,
    pub clip_stack: C2RustUnnamed_9,
    pub id_stack: C2RustUnnamed_8,
    pub layout_stack: C2RustUnnamed_7,
    pub text_stack: C2RustUnnamed_6,
    pub container_pool: [mu_PoolItem; 48],
    pub containers: [mu_Container; 48],
    pub treenode_pool: [mu_PoolItem; 48],
    pub mouse_pos: mu_Vec2,
    pub last_mouse_pos: mu_Vec2,
    pub mouse_delta: mu_Vec2,
    pub scroll_delta: mu_Vec2,
    pub mouse_down: libc::c_int,
    pub mouse_pressed: libc::c_int,
    pub key_down: libc::c_int,
    pub key_pressed: libc::c_int,
    pub input_text: [libc::c_char; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Vec2 {
    pub x: libc::c_int,
    pub y: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_PoolItem {
    pub id: mu_Id,
    pub last_update: libc::c_int,
}
pub type mu_Id = libc::c_uint;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Container {
    pub head_idx: libc::c_int,
    pub tail_idx: libc::c_int,
    pub rect: mu_Rect,
    pub body: mu_Rect,
    pub content_size: mu_Vec2,
    pub scroll: mu_Vec2,
    pub zindex: libc::c_int,
    pub open: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Rect {
    pub x: libc::c_int,
    pub y: libc::c_int,
    pub w: libc::c_int,
    pub h: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_6 {
    pub idx: libc::c_int,
    pub items: [libc::c_char; 65536],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_7 {
    pub idx: libc::c_int,
    pub items: [mu_Layout; 16],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Layout {
    pub body: mu_Rect,
    pub next: mu_Rect,
    pub position: mu_Vec2,
    pub size: mu_Vec2,
    pub max: mu_Vec2,
    pub widths: [libc::c_int; 16],
    pub items: libc::c_int,
    pub item_index: libc::c_int,
    pub next_row: libc::c_int,
    pub next_type: libc::c_int,
    pub indent: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_8 {
    pub idx: libc::c_int,
    pub items: [mu_Id; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_9 {
    pub idx: libc::c_int,
    pub items: [mu_Rect; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_10 {
    pub idx: libc::c_int,
    pub items: [*mut mu_Container; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_11 {
    pub idx: libc::c_int,
    pub items: [*mut mu_Container; 32],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_12 {
    pub idx: libc::c_int,
    pub items: [mu_Command; 4096],
}
#[derive(Copy, Clone)]
#[repr(C)]
pub union mu_Command {
    pub type_0: libc::c_int,
    pub base: mu_BaseCommand,
    pub jump: mu_JumpCommand,
    pub clip: mu_ClipCommand,
    pub rect: mu_RectCommand,
    pub text: mu_TextCommand,
    pub icon: mu_IconCommand,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_IconCommand {
    pub base: mu_BaseCommand,
    pub rect: mu_Rect,
    pub id: libc::c_int,
    pub color: mu_Color,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Color {
    pub r: libc::c_uchar,
    pub g: libc::c_uchar,
    pub b: libc::c_uchar,
    pub a: libc::c_uchar,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_BaseCommand {
    pub type_0: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_TextCommand {
    pub base: mu_BaseCommand,
    pub font: mu_Font,
    pub pos: mu_Vec2,
    pub color: mu_Color,
    pub str_0: *mut libc::c_char,
}
pub type mu_Font = *mut libc::c_void;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_RectCommand {
    pub base: mu_BaseCommand,
    pub rect: mu_Rect,
    pub color: mu_Color,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_ClipCommand {
    pub base: mu_BaseCommand,
    pub rect: mu_Rect,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_JumpCommand {
    pub base: mu_BaseCommand,
    pub dst_idx: libc::c_int,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mu_Style {
    pub font: mu_Font,
    pub size: mu_Vec2,
    pub padding: libc::c_int,
    pub spacing: libc::c_int,
    pub indent: libc::c_int,
    pub title_height: libc::c_int,
    pub scrollbar_size: libc::c_int,
    pub thumb_size: libc::c_int,
    pub colors: [mu_Color; 14],
}
pub type mu_Real = libc::c_float;
#[derive(Copy, Clone)]
#[repr(C)]
pub struct C2RustUnnamed_13 {
    pub label: *const libc::c_char,
    pub idx: libc::c_int,
}
static mut logbuf: [libc::c_char; 64000] = [0; 64000];
static mut logbuf_updated: libc::c_int = 0 as libc::c_int;
static mut bg: [libc::c_float; 3] = [
    90 as libc::c_int as libc::c_float,
    95 as libc::c_int as libc::c_float,
    100 as libc::c_int as libc::c_float,
];
unsafe extern "C" fn write_log(mut text: *const libc::c_char) {
    if logbuf[0 as libc::c_int as usize] != 0 {
        strcat(logbuf.as_mut_ptr(), b"\n\0" as *const u8 as *const libc::c_char);
    }
    strcat(logbuf.as_mut_ptr(), text);
    logbuf_updated = 1 as libc::c_int;
}
unsafe extern "C" fn test_window(mut ctx: *mut mu_Context) {
    if mu_begin_window_ex(
        ctx,
        b"Demo Window\0" as *const u8 as *const libc::c_char,
        mu_rect(
            40 as libc::c_int,
            40 as libc::c_int,
            300 as libc::c_int,
            450 as libc::c_int,
        ),
        0 as libc::c_int,
    ) != 0
    {
        let mut win: *mut mu_Container = mu_get_current_container(ctx);
        (*win)
            .rect
            .w = if (*win).rect.w > 240 as libc::c_int {
            (*win).rect.w
        } else {
            240 as libc::c_int
        };
        (*win)
            .rect
            .h = if (*win).rect.h > 300 as libc::c_int {
            (*win).rect.h
        } else {
            300 as libc::c_int
        };
        if mu_header_ex(
            ctx,
            b"Window Info\0" as *const u8 as *const libc::c_char,
            0 as libc::c_int,
        ) != 0
        {
            let mut win_0: *mut mu_Container = mu_get_current_container(ctx);
            let mut buf: [libc::c_char; 64] = [0; 64];
            mu_layout_row(
                ctx,
                2 as libc::c_int,
                [54 as libc::c_int, -(1 as libc::c_int)].as_mut_ptr(),
                0 as libc::c_int,
            );
            mu_label(ctx, b"Position:\0" as *const u8 as *const libc::c_char);
            sprintf(
                buf.as_mut_ptr(),
                b"%d, %d\0" as *const u8 as *const libc::c_char,
                (*win_0).rect.x,
                (*win_0).rect.y,
            );
            mu_label(ctx, buf.as_mut_ptr());
            mu_label(ctx, b"Size:\0" as *const u8 as *const libc::c_char);
            sprintf(
                buf.as_mut_ptr(),
                b"%d, %d\0" as *const u8 as *const libc::c_char,
                (*win_0).rect.w,
                (*win_0).rect.h,
            );
            mu_label(ctx, buf.as_mut_ptr());
        }
        if mu_header_ex(
            ctx,
            b"Test Buttons\0" as *const u8 as *const libc::c_char,
            MU_OPT_EXPANDED as libc::c_int,
        ) != 0
        {
            mu_layout_row(
                ctx,
                3 as libc::c_int,
                [86 as libc::c_int, -(110 as libc::c_int), -(1 as libc::c_int)]
                    .as_mut_ptr(),
                0 as libc::c_int,
            );
            mu_label(ctx, b"Test buttons 1:\0" as *const u8 as *const libc::c_char);
            if mu_button_ex(
                ctx,
                b"Button 1\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
                MU_OPT_ALIGNCENTER as libc::c_int,
            ) != 0
            {
                write_log(b"Pressed button 1\0" as *const u8 as *const libc::c_char);
            }
            if mu_button_ex(
                ctx,
                b"Button 2\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
                MU_OPT_ALIGNCENTER as libc::c_int,
            ) != 0
            {
                write_log(b"Pressed button 2\0" as *const u8 as *const libc::c_char);
            }
            mu_label(ctx, b"Test buttons 2:\0" as *const u8 as *const libc::c_char);
            if mu_button_ex(
                ctx,
                b"Button 3\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
                MU_OPT_ALIGNCENTER as libc::c_int,
            ) != 0
            {
                write_log(b"Pressed button 3\0" as *const u8 as *const libc::c_char);
            }
            if mu_button_ex(
                ctx,
                b"Popup\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
                MU_OPT_ALIGNCENTER as libc::c_int,
            ) != 0
            {
                mu_open_popup(ctx, b"Test Popup\0" as *const u8 as *const libc::c_char);
            }
            if mu_begin_popup(ctx, b"Test Popup\0" as *const u8 as *const libc::c_char)
                != 0
            {
                mu_button_ex(
                    ctx,
                    b"Hello\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                );
                mu_button_ex(
                    ctx,
                    b"World\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                );
                mu_end_popup(ctx);
            }
        }
        if mu_header_ex(
            ctx,
            b"Tree and Text\0" as *const u8 as *const libc::c_char,
            MU_OPT_EXPANDED as libc::c_int,
        ) != 0
        {
            mu_layout_row(
                ctx,
                2 as libc::c_int,
                [140 as libc::c_int, -(1 as libc::c_int)].as_mut_ptr(),
                0 as libc::c_int,
            );
            mu_layout_begin_column(ctx);
            if mu_begin_treenode_ex(
                ctx,
                b"Test 1\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
            ) != 0
            {
                if mu_begin_treenode_ex(
                    ctx,
                    b"Test 1a\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                ) != 0
                {
                    mu_label(ctx, b"Hello\0" as *const u8 as *const libc::c_char);
                    mu_label(ctx, b"world\0" as *const u8 as *const libc::c_char);
                    mu_end_treenode(ctx);
                }
                if mu_begin_treenode_ex(
                    ctx,
                    b"Test 1b\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                ) != 0
                {
                    if mu_button_ex(
                        ctx,
                        b"Button 1\0" as *const u8 as *const libc::c_char,
                        0 as libc::c_int,
                        MU_OPT_ALIGNCENTER as libc::c_int,
                    ) != 0
                    {
                        write_log(
                            b"Pressed button 1\0" as *const u8 as *const libc::c_char,
                        );
                    }
                    if mu_button_ex(
                        ctx,
                        b"Button 2\0" as *const u8 as *const libc::c_char,
                        0 as libc::c_int,
                        MU_OPT_ALIGNCENTER as libc::c_int,
                    ) != 0
                    {
                        write_log(
                            b"Pressed button 2\0" as *const u8 as *const libc::c_char,
                        );
                    }
                    mu_end_treenode(ctx);
                }
                mu_end_treenode(ctx);
            }
            if mu_begin_treenode_ex(
                ctx,
                b"Test 2\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
            ) != 0
            {
                mu_layout_row(
                    ctx,
                    2 as libc::c_int,
                    [54 as libc::c_int, 54 as libc::c_int].as_mut_ptr(),
                    0 as libc::c_int,
                );
                if mu_button_ex(
                    ctx,
                    b"Button 3\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                ) != 0
                {
                    write_log(b"Pressed button 3\0" as *const u8 as *const libc::c_char);
                }
                if mu_button_ex(
                    ctx,
                    b"Button 4\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                ) != 0
                {
                    write_log(b"Pressed button 4\0" as *const u8 as *const libc::c_char);
                }
                if mu_button_ex(
                    ctx,
                    b"Button 5\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                ) != 0
                {
                    write_log(b"Pressed button 5\0" as *const u8 as *const libc::c_char);
                }
                if mu_button_ex(
                    ctx,
                    b"Button 6\0" as *const u8 as *const libc::c_char,
                    0 as libc::c_int,
                    MU_OPT_ALIGNCENTER as libc::c_int,
                ) != 0
                {
                    write_log(b"Pressed button 6\0" as *const u8 as *const libc::c_char);
                }
                mu_end_treenode(ctx);
            }
            if mu_begin_treenode_ex(
                ctx,
                b"Test 3\0" as *const u8 as *const libc::c_char,
                0 as libc::c_int,
            ) != 0
            {
                static mut checks: [libc::c_int; 3] = [
                    1 as libc::c_int,
                    0 as libc::c_int,
                    1 as libc::c_int,
                ];
                mu_checkbox(
                    ctx,
                    b"Checkbox 1\0" as *const u8 as *const libc::c_char,
                    &mut *checks.as_mut_ptr().offset(0 as libc::c_int as isize),
                );
                mu_checkbox(
                    ctx,
                    b"Checkbox 2\0" as *const u8 as *const libc::c_char,
                    &mut *checks.as_mut_ptr().offset(1 as libc::c_int as isize),
                );
                mu_checkbox(
                    ctx,
                    b"Checkbox 3\0" as *const u8 as *const libc::c_char,
                    &mut *checks.as_mut_ptr().offset(2 as libc::c_int as isize),
                );
                mu_end_treenode(ctx);
            }
            mu_layout_end_column(ctx);
            mu_layout_begin_column(ctx);
            mu_layout_row(
                ctx,
                1 as libc::c_int,
                [-(1 as libc::c_int)].as_mut_ptr(),
                0 as libc::c_int,
            );
            mu_text(
                ctx,
                b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. Maecenas lacinia, sem eu lacinia molestie, mi risus faucibus ipsum, eu varius magna felis a nulla.\0"
                    as *const u8 as *const libc::c_char,
            );
            mu_layout_end_column(ctx);
        }
        if mu_header_ex(
            ctx,
            b"Background Color\0" as *const u8 as *const libc::c_char,
            MU_OPT_EXPANDED as libc::c_int,
        ) != 0
        {
            mu_layout_row(
                ctx,
                2 as libc::c_int,
                [-(78 as libc::c_int), -(1 as libc::c_int)].as_mut_ptr(),
                74 as libc::c_int,
            );
            mu_layout_begin_column(ctx);
            mu_layout_row(
                ctx,
                2 as libc::c_int,
                [46 as libc::c_int, -(1 as libc::c_int)].as_mut_ptr(),
                0 as libc::c_int,
            );
            mu_label(ctx, b"Red:\0" as *const u8 as *const libc::c_char);
            mu_slider_ex(
                ctx,
                &mut *bg.as_mut_ptr().offset(0 as libc::c_int as isize),
                0 as libc::c_int as mu_Real,
                255 as libc::c_int as mu_Real,
                0 as libc::c_int as mu_Real,
                b"%.2f\0" as *const u8 as *const libc::c_char,
                MU_OPT_ALIGNCENTER as libc::c_int,
            );
            mu_label(ctx, b"Green:\0" as *const u8 as *const libc::c_char);
            mu_slider_ex(
                ctx,
                &mut *bg.as_mut_ptr().offset(1 as libc::c_int as isize),
                0 as libc::c_int as mu_Real,
                255 as libc::c_int as mu_Real,
                0 as libc::c_int as mu_Real,
                b"%.2f\0" as *const u8 as *const libc::c_char,
                MU_OPT_ALIGNCENTER as libc::c_int,
            );
            mu_label(ctx, b"Blue:\0" as *const u8 as *const libc::c_char);
            mu_slider_ex(
                ctx,
                &mut *bg.as_mut_ptr().offset(2 as libc::c_int as isize),
                0 as libc::c_int as mu_Real,
                255 as libc::c_int as mu_Real,
                0 as libc::c_int as mu_Real,
                b"%.2f\0" as *const u8 as *const libc::c_char,
                MU_OPT_ALIGNCENTER as libc::c_int,
            );
            mu_layout_end_column(ctx);
            let mut r: mu_Rect = mu_layout_next(ctx);
            mu_draw_rect(
                ctx,
                r,
                mu_color(
                    bg[0 as libc::c_int as usize] as libc::c_int,
                    bg[1 as libc::c_int as usize] as libc::c_int,
                    bg[2 as libc::c_int as usize] as libc::c_int,
                    255 as libc::c_int,
                ),
            );
            let mut buf_0: [libc::c_char; 32] = [0; 32];
            sprintf(
                buf_0.as_mut_ptr(),
                b"#%02X%02X%02X\0" as *const u8 as *const libc::c_char,
                bg[0 as libc::c_int as usize] as libc::c_int,
                bg[1 as libc::c_int as usize] as libc::c_int,
                bg[2 as libc::c_int as usize] as libc::c_int,
            );
            mu_draw_control_text(
                ctx,
                buf_0.as_mut_ptr(),
                r,
                MU_COLOR_TEXT as libc::c_int,
                MU_OPT_ALIGNCENTER as libc::c_int,
            );
        }
        mu_end_window(ctx);
    }
}
unsafe extern "C" fn log_window(mut ctx: *mut mu_Context) {
    if mu_begin_window_ex(
        ctx,
        b"Log Window\0" as *const u8 as *const libc::c_char,
        mu_rect(
            350 as libc::c_int,
            40 as libc::c_int,
            300 as libc::c_int,
            200 as libc::c_int,
        ),
        0 as libc::c_int,
    ) != 0
    {
        mu_layout_row(
            ctx,
            1 as libc::c_int,
            [-(1 as libc::c_int)].as_mut_ptr(),
            -(25 as libc::c_int),
        );
        mu_begin_panel_ex(
            ctx,
            b"Log Output\0" as *const u8 as *const libc::c_char,
            0 as libc::c_int,
        );
        let mut panel: *mut mu_Container = mu_get_current_container(ctx);
        mu_layout_row(
            ctx,
            1 as libc::c_int,
            [-(1 as libc::c_int)].as_mut_ptr(),
            -(1 as libc::c_int),
        );
        mu_text(ctx, logbuf.as_mut_ptr());
        mu_end_panel(ctx);
        if logbuf_updated != 0 {
            (*panel).scroll.y = (*panel).content_size.y;
            logbuf_updated = 0 as libc::c_int;
        }
        static mut buf: [libc::c_char; 128] = [0; 128];
        let mut submitted: libc::c_int = 0 as libc::c_int;
        mu_layout_row(
            ctx,
            2 as libc::c_int,
            [-(70 as libc::c_int), -(1 as libc::c_int)].as_mut_ptr(),
            0 as libc::c_int,
        );
        if mu_textbox_ex(
            ctx,
            buf.as_mut_ptr(),
            ::core::mem::size_of::<[libc::c_char; 128]>() as libc::c_ulong
                as libc::c_int,
            0 as libc::c_int,
        ) & MU_RES_SUBMIT as libc::c_int != 0
        {
            mu_set_focus(ctx, (*ctx).last_id);
            submitted = 1 as libc::c_int;
        }
        if mu_button_ex(
            ctx,
            b"Submit\0" as *const u8 as *const libc::c_char,
            0 as libc::c_int,
            MU_OPT_ALIGNCENTER as libc::c_int,
        ) != 0
        {
            submitted = 1 as libc::c_int;
        }
        if submitted != 0 {
            write_log(buf.as_mut_ptr());
            buf[0 as libc::c_int as usize] = '\0' as i32 as libc::c_char;
        }
        mu_end_window(ctx);
    }
}
unsafe extern "C" fn uint8_slider(
    mut ctx: *mut mu_Context,
    mut value: *mut libc::c_uchar,
    mut low: libc::c_int,
    mut high: libc::c_int,
) -> libc::c_int {
    static mut tmp: libc::c_float = 0.;
    mu_push_id(
        ctx,
        &mut value as *mut *mut libc::c_uchar as *const libc::c_void,
        ::core::mem::size_of::<*mut libc::c_uchar>() as libc::c_ulong as libc::c_int,
    );
    tmp = *value as libc::c_float;
    let mut res: libc::c_int = mu_slider_ex(
        ctx,
        &mut tmp,
        low as mu_Real,
        high as mu_Real,
        0 as libc::c_int as mu_Real,
        b"%.0f\0" as *const u8 as *const libc::c_char,
        MU_OPT_ALIGNCENTER as libc::c_int,
    );
    *value = tmp as libc::c_uchar;
    mu_pop_id(ctx);
    return res;
}
unsafe extern "C" fn style_window(mut ctx: *mut mu_Context) {
    static mut colors: [C2RustUnnamed_13; 15] = [
        {
            let mut init = C2RustUnnamed_13 {
                label: b"text:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_TEXT as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"border:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BORDER as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"windowbg:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_WINDOWBG as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"titlebg:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_TITLEBG as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"titletext:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_TITLETEXT as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"panelbg:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_PANELBG as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"button:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BUTTON as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"buttonhover:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BUTTONHOVER as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"buttonfocus:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BUTTONFOCUS as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"base:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BASE as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"basehover:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BASEHOVER as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"basefocus:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_BASEFOCUS as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"scrollbase:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_SCROLLBASE as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: b"scrollthumb:\0" as *const u8 as *const libc::c_char,
                idx: MU_COLOR_SCROLLTHUMB as libc::c_int,
            };
            init
        },
        {
            let mut init = C2RustUnnamed_13 {
                label: 0 as *const libc::c_char,
                idx: 0,
            };
            init
        },
    ];
    if mu_begin_window_ex(
        ctx,
        b"Style Editor\0" as *const u8 as *const libc::c_char,
        mu_rect(
            350 as libc::c_int,
            250 as libc::c_int,
            300 as libc::c_int,
            240 as libc::c_int,
        ),
        0 as libc::c_int,
    ) != 0
    {
        let mut sw: libc::c_int = ((*mu_get_current_container(ctx)).body.w
            as libc::c_double * 0.14f64) as libc::c_int;
        mu_layout_row(
            ctx,
            6 as libc::c_int,
            [80 as libc::c_int, sw, sw, sw, sw, -(1 as libc::c_int)].as_mut_ptr(),
            0 as libc::c_int,
        );
        let mut i: libc::c_int = 0 as libc::c_int;
        while !(colors[i as usize].label).is_null() {
            mu_label(ctx, colors[i as usize].label);
            uint8_slider(
                ctx,
                &mut (*((*(*ctx).style).colors).as_mut_ptr().offset(i as isize)).r,
                0 as libc::c_int,
                255 as libc::c_int,
            );
            uint8_slider(
                ctx,
                &mut (*((*(*ctx).style).colors).as_mut_ptr().offset(i as isize)).g,
                0 as libc::c_int,
                255 as libc::c_int,
            );
            uint8_slider(
                ctx,
                &mut (*((*(*ctx).style).colors).as_mut_ptr().offset(i as isize)).b,
                0 as libc::c_int,
                255 as libc::c_int,
            );
            uint8_slider(
                ctx,
                &mut (*((*(*ctx).style).colors).as_mut_ptr().offset(i as isize)).a,
                0 as libc::c_int,
                255 as libc::c_int,
            );
            mu_draw_rect(ctx, mu_layout_next(ctx), (*(*ctx).style).colors[i as usize]);
            i += 1;
        }
        mu_end_window(ctx);
    }
}
unsafe extern "C" fn process_frame(mut ctx: *mut mu_Context) {
    mu_begin(ctx);
    style_window(ctx);
    log_window(ctx);
    test_window(ctx);
    mu_end(ctx);
}
static mut button_map: [libc::c_char; 256] = [
    0,
    MU_MOUSE_LEFT as libc::c_int as libc::c_char,
    MU_MOUSE_MIDDLE as libc::c_int as libc::c_char,
    MU_MOUSE_RIGHT as libc::c_int as libc::c_char,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
];
static mut key_map: [libc::c_char; 256] = [
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    MU_KEY_BACKSPACE as libc::c_int as libc::c_char,
    0,
    0,
    0,
    0,
    MU_KEY_RETURN as libc::c_int as libc::c_char,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    MU_KEY_CTRL as libc::c_int as libc::c_char,
    MU_KEY_SHIFT as libc::c_int as libc::c_char,
    MU_KEY_ALT as libc::c_int as libc::c_char,
    0,
    MU_KEY_CTRL as libc::c_int as libc::c_char,
    MU_KEY_SHIFT as libc::c_int as libc::c_char,
    MU_KEY_ALT as libc::c_int as libc::c_char,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
];
unsafe extern "C" fn text_width(
    mut font: mu_Font,
    mut text: *const libc::c_char,
    mut len: libc::c_int,
) -> libc::c_int {
    if len == -(1 as libc::c_int) {
        len = strlen(text) as libc::c_int;
    }
    return r_get_text_width(text, len);
}
unsafe extern "C" fn text_height(mut font: mu_Font) -> libc::c_int {
    return r_get_text_height();
}
pub fn main() {
    unsafe {
        SDL_Init(
            0x1 as libc::c_uint | 0x10 as libc::c_uint | 0x20 as libc::c_uint
                | 0x4000 as libc::c_uint | 0x200 as libc::c_uint | 0x1000 as libc::c_uint
                | 0x2000 as libc::c_uint | 0x8000 as libc::c_uint,
        );
        r_init();
        let mut ctx: *mut mu_Context = malloc(
            ::core::mem::size_of::<mu_Context>() as libc::c_ulong,
        ) as *mut mu_Context;
        mu_init(ctx);
        (*ctx)
            .text_width = Some(
            text_width
                as unsafe extern "C" fn(
                mu_Font,
                *const libc::c_char,
                libc::c_int,
            ) -> libc::c_int,
        );
        (*ctx)
            .text_height = Some(text_height as unsafe extern "C" fn(mu_Font) -> libc::c_int);
        loop {
            let mut e: SDL_Event = SDL_Event { type_0: 0 };
            while SDL_PollEvent(&mut e) != 0 {
                match e.type_0 {
                    256 => {
                        exit(0 as libc::c_int);
                    }
                    1024 => {
                        mu_input_mousemove(ctx, e.motion.x, e.motion.y);
                    }
                    1027 => {
                        mu_input_scroll(
                            ctx,
                            0 as libc::c_int,
                            e.wheel.y * -(30 as libc::c_int),
                        );
                    }
                    771 => {
                        mu_input_text(ctx, (e.text.text).as_mut_ptr());
                    }
                    1025 | 1026 => {
                        let mut b: libc::c_int = button_map[(e.button.button as libc::c_int
                            & 0xff as libc::c_int) as usize] as libc::c_int;
                        if b != 0
                            && e.type_0 == SDL_MOUSEBUTTONDOWN as libc::c_int as libc::c_uint
                        {
                            mu_input_mousedown(ctx, e.button.x, e.button.y, b);
                        }
                        if b != 0
                            && e.type_0 == SDL_MOUSEBUTTONUP as libc::c_int as libc::c_uint
                        {
                            mu_input_mouseup(ctx, e.button.x, e.button.y, b);
                        }
                    }
                    768 | 769 => {
                        let mut c: libc::c_int = key_map[(e.key.keysym.sym
                            & 0xff as libc::c_int) as usize] as libc::c_int;
                        if c != 0 && e.type_0 == SDL_KEYDOWN as libc::c_int as libc::c_uint {
                            mu_input_keydown(ctx, c);
                        }
                        if c != 0 && e.type_0 == SDL_KEYUP as libc::c_int as libc::c_uint {
                            mu_input_keyup(ctx, c);
                        }
                    }
                    _ => {}
                }
            }
            process_frame(ctx);
            r_clear(
                mu_color(
                    bg[0 as libc::c_int as usize] as libc::c_int,
                    bg[1 as libc::c_int as usize] as libc::c_int,
                    bg[2 as libc::c_int as usize] as libc::c_int,
                    255 as libc::c_int,
                ),
            );
            let mut cmd: *mut mu_Command = 0 as *mut mu_Command;
            while mu_next_command(ctx, &mut cmd) != 0 {
                match (*cmd).type_0 {
                    4 => {
                        r_draw_text((*cmd).text.str_0, (*cmd).text.pos, (*cmd).text.color);
                    }
                    3 => {
                        r_draw_rect((*cmd).rect.rect, (*cmd).rect.color);
                    }
                    5 => {
                        r_draw_icon((*cmd).icon.id, (*cmd).icon.rect, (*cmd).icon.color);
                    }
                    2 => {
                        r_set_clip_rect((*cmd).clip.rect);
                    }
                    _ => {}
                }
            }
            r_present();
        };
    }
}
