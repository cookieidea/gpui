#![allow(clippy::module_inception)]
pub use uic_macros::file_enum;

pub mod assets;
pub mod components;
pub mod desktop;
pub mod utils;

pub fn init(cx: &mut gpui::App) {
    components::input::init(cx);
    components::modal::init(cx);
    components::toast::init(cx);
}
