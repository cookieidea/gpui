use gpui::{Hsla, Pixels, hsla, px};

#[derive(Clone, Copy, Debug)]
pub struct ModalButtonAppearance {
    pub background: Hsla,
    pub foreground: Hsla,
    pub border: Hsla,
    pub hover_background: Hsla,
    pub border_width: Pixels,
    pub radius: Pixels,
    pub height: Pixels,
    pub padding_x: Pixels,
}

#[derive(Clone, Copy, Debug)]
pub struct ModalAppearance {
    pub backdrop: Hsla,
    pub background: Hsla,
    pub foreground: Hsla,
    pub show_border: bool,
    pub border: Hsla,
    pub border_width: Pixels,
    pub radius: Pixels,
    pub header_padding_x: Pixels,
    pub header_padding_y: Pixels,
    pub body_padding_x: Pixels,
    pub body_padding_y: Pixels,
    pub footer_padding_x: Pixels,
    pub footer_padding_y: Pixels,
    pub section_border: Hsla,
    pub footer_gap: Pixels,
    pub ok_button: ModalButtonAppearance,
    pub cancel_button: ModalButtonAppearance,
}

impl ModalButtonAppearance {
    pub fn background(mut self, color: Hsla) -> Self {
        self.background = color;
        self
    }

    pub fn foreground(mut self, color: Hsla) -> Self {
        self.foreground = color;
        self
    }

    pub fn border_color(mut self, color: Hsla) -> Self {
        self.border = color;
        self
    }

    pub fn hover_background(mut self, color: Hsla) -> Self {
        self.hover_background = color;
        self
    }

    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }
}

impl ModalAppearance {
    pub fn backdrop(mut self, color: Hsla) -> Self {
        self.backdrop = color;
        self
    }

    pub fn background(mut self, color: Hsla) -> Self {
        self.background = color;
        self
    }

    pub fn foreground(mut self, color: Hsla) -> Self {
        self.foreground = color;
        self
    }

    /// Shows or hides the panel border and the header/footer dividers.
    pub fn border(mut self, show: bool) -> Self {
        self.show_border = show;
        self
    }

    pub fn border_color(mut self, color: Hsla) -> Self {
        self.border = color;
        self
    }

    pub fn radius(mut self, radius: Pixels) -> Self {
        self.radius = radius;
        self
    }

    pub fn ok_button(mut self, appearance: ModalButtonAppearance) -> Self {
        self.ok_button = appearance;
        self
    }

    pub fn cancel_button(mut self, appearance: ModalButtonAppearance) -> Self {
        self.cancel_button = appearance;
        self
    }
}

impl Default for ModalAppearance {
    fn default() -> Self {
        Self {
            backdrop: hsla(0., 0., 0., 0.45),
            background: hsla(0., 0., 1., 1.),
            foreground: hsla(0., 0., 0.08, 1.),
            show_border: true,
            border: hsla(0., 0., 0., 0.12),
            border_width: px(1.),
            radius: px(12.),
            header_padding_x: px(24.),
            header_padding_y: px(10.),
            body_padding_x: px(24.),
            body_padding_y: px(16.),
            footer_padding_x: px(24.),
            footer_padding_y: px(12.),
            section_border: hsla(0., 0., 0., 0.08),
            footer_gap: px(10.),
            ok_button: ModalButtonAppearance {
                background: hsla(0.61, 0.85, 0.55, 1.),
                foreground: hsla(0., 0., 1., 1.),
                border: hsla(0.61, 0.85, 0.55, 1.),
                hover_background: hsla(0.61, 0.75, 0.48, 1.),
                border_width: px(1.),
                radius: px(8.),
                height: px(36.),
                padding_x: px(16.),
            },
            cancel_button: ModalButtonAppearance {
                background: hsla(0., 0., 1., 1.),
                foreground: hsla(0., 0., 0.16, 1.),
                border: hsla(0., 0., 0., 0.18),
                hover_background: hsla(0., 0., 0.96, 1.),
                border_width: px(1.),
                radius: px(8.),
                height: px(36.),
                padding_x: px(16.),
            },
        }
    }
}
