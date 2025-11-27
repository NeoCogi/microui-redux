use microui_redux::{AtlasHandle, Dimensioni};

#[cfg(feature = "builder")]
use microui_redux::builder;

pub fn default_slots() -> Vec<Dimensioni> { vec![Dimensioni::new(64, 64), Dimensioni::new(24, 32), Dimensioni::new(64, 24)] }

#[cfg(feature = "builder")]
pub fn atlas_config<'a>(slots: &'a [Dimensioni]) -> builder::Config<'a> {
    builder::Config {
        texture_height: 256,
        texture_width: 256,
        white_icon: String::from("assets/WHITE.png"),
        close_icon: String::from("assets/CLOSE.png"),
        expand_icon: String::from("assets/PLUS.png"),
        collapse_icon: String::from("assets/MINUS.png"),
        check_icon: String::from("assets/CHECK.png"),
        expand_down_icon: String::from("assets/EXPAND_DOWN.png"),
        open_folder_16_icon: String::from("assets/OPEN_FOLDER_16.png"),
        closed_folder_16_icon: String::from("assets/CLOSED_FOLDER_16.png"),
        file_16_icon: String::from("assets/FILE_16.png"),
        default_font: String::from("assets/NORMAL.ttf"),
        default_font_size: 12,
        slots,
    }
}

#[cfg(feature = "builder")]
pub fn load_atlas(slots: &[Dimensioni]) -> AtlasHandle { builder::Builder::from_config(&atlas_config(slots)).expect("valid atlas config").to_atlas() }

#[cfg(not(feature = "builder"))]
mod prebuilt {
    use microui_redux::AtlasHandle;
    include!(concat!(env!("OUT_DIR"), "/prebuilt_atlas.rs"));

    pub fn load() -> AtlasHandle { AtlasHandle::from(&PREBUILT_ATLAS) }
}

#[cfg(not(feature = "builder"))]
pub fn load_atlas(_slots: &[Dimensioni]) -> AtlasHandle { prebuilt::load() }
