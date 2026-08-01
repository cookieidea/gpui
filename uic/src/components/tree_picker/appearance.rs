use gpui::{Hsla, Pixels, SharedString, hsla, px};

use crate::components::input::InputAppearance;

#[derive(Clone, Debug)]
pub struct TreePickerAppearance {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub border: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub accent: Hsla,
    pub icon_background: Hsla,
    pub row_height: Pixels,
    pub min_height: Pixels,
    pub max_height: Pixels,
    pub radius: Pixels,
    pub indent: Pixels,
    pub search: InputAppearance,
    pub directory_icon: SharedString,
    pub file_icon: SharedString,
    pub expanded_icon: SharedString,
    pub collapsed_icon: SharedString,
    pub selected_icon: SharedString,
    pub search_icon: SharedString,
}

impl Default for TreePickerAppearance {
    fn default() -> Self {
        Self {
            background: hsla(0., 0., 1., 1.),
            foreground: hsla(0., 0., 0.08, 1.),
            muted: hsla(0., 0., 0.45, 1.),
            border: hsla(0., 0., 0.85, 1.),
            hover: hsla(0.61, 0.85, 0.55, 0.08),
            selected: hsla(0.61, 0.85, 0.55, 0.14),
            accent: hsla(0.61, 0.85, 0.55, 1.),
            icon_background: hsla(0.61, 0.85, 0.55, 0.1),
            row_height: px(46.),
            min_height: px(280.),
            max_height: px(440.),
            radius: px(10.),
            indent: px(20.),
            search: InputAppearance::default(),
            directory_icon: "".into(),
            file_icon: "".into(),
            expanded_icon: "".into(),
            collapsed_icon: "".into(),
            selected_icon: "".into(),
            search_icon: "".into(),
        }
    }
}
