use gpui::{Hsla, Pixels, hsla, px};

#[derive(Clone, Copy, Debug)]
pub struct ColorPickerAppearance {
    pub background: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub marker: Hsla,
    pub area_height: Pixels,
    pub hue_width: Pixels,
    pub marker_size: Pixels,
    pub radius: Pixels,
    pub padding: Pixels,
    pub gap: Pixels,
}

impl Default for ColorPickerAppearance {
    fn default() -> Self {
        Self {
            background: hsla(0.52, 0.12, 0.11, 1.0),
            border: hsla(0.52, 0.08, 0.26, 1.0),
            accent: hsla(0.52, 0.85, 0.48, 1.0),
            marker: hsla(0.0, 0.0, 1.0, 1.0),
            area_height: px(320.0),
            hue_width: px(34.0),
            marker_size: px(18.0),
            radius: px(12.0),
            padding: px(16.0),
            gap: px(14.0),
        }
    }
}
