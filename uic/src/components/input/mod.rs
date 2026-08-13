macro_rules! input_appearance {
    ($ty:ty) => {
        impl $ty {
            pub fn background(mut self, background: gpui::Hsla) -> Self {
                self.appearance.background = background;
                self
            }

            pub fn foreground(mut self, foreground: gpui::Hsla) -> Self {
                self.appearance.foreground = foreground;
                self
            }

            pub fn placeholder(mut self, placeholder: gpui::Hsla) -> Self {
                self.appearance.placeholder = placeholder;
                self
            }

            pub fn border(mut self, border: gpui::Hsla) -> Self {
                self.appearance.border = border;
                self
            }

            pub fn focus_border(mut self, focus_border: gpui::Hsla) -> Self {
                self.appearance.focus_border = focus_border;
                self
            }

            pub fn caret(mut self, caret: gpui::Hsla) -> Self {
                self.appearance.caret = caret;
                self
            }

            pub fn selection(mut self, selection: gpui::Hsla) -> Self {
                self.appearance.selection = selection;
                self
            }

            pub fn caret_width(mut self, caret_width: gpui::Pixels) -> Self {
                self.appearance.caret_width = caret_width;
                self
            }

            pub fn caret_height(mut self, caret_height: gpui::Pixels) -> Self {
                self.appearance.caret_height = caret_height;
                self
            }

            pub fn height(mut self, height: gpui::Pixels) -> Self {
                self.appearance.height = height;
                self.appearance.multiline_height = height;
                self.rows = None;
                self
            }

            pub fn multiline_height(mut self, height: gpui::Pixels) -> Self {
                self.appearance.multiline_height = height;
                self.rows = None;
                self
            }

            /// Sets the number of visible rows for a multi-line input.
            ///
            /// The final height is derived from the effective line height, vertical padding,
            /// and border width at render time, so the order of these builder calls does not
            /// matter.
            pub fn rows(mut self, rows: usize) -> Self {
                self.rows = Some(rows.max(1));
                self
            }

            pub fn line_height(mut self, line_height: gpui::Pixels) -> Self {
                self.appearance.line_height = line_height;
                self.line_height_explicit = true;
                self
            }

            pub fn radius(mut self, radius: gpui::Pixels) -> Self {
                self.appearance.radius = radius;
                self
            }

            pub fn border_width(mut self, border_width: gpui::Pixels) -> Self {
                self.appearance.border_width = border_width;
                self
            }

            pub fn padding_x(mut self, padding_x: gpui::Pixels) -> Self {
                self.appearance.padding_x = padding_x;
                self
            }

            pub fn padding_y(mut self, padding_y: gpui::Pixels) -> Self {
                self.appearance.padding_y = padding_y;
                self
            }

            pub fn gap(mut self, gap: gpui::Pixels) -> Self {
                self.appearance.gap = gap;
                self
            }

            pub fn font_size(mut self, font_size: gpui::Pixels) -> Self {
                self.appearance.font_size = font_size;
                if !self.line_height_explicit {
                    self.appearance.line_height = font_size * 1.5;
                }
                self
            }
        }
    };
}

mod actions;
mod appearance;
mod element;
#[allow(clippy::module_inception)]
mod input;
mod read_only;
mod state;

use gpui::SharedString;

pub(crate) use actions::Submit;
pub use actions::init;
pub use appearance::InputAppearance;
pub use input::Input;
pub use read_only::ReadOnlyInput;
pub use state::TextInput;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputMode {
    #[default]
    /// A single-line plain-text field. Enter emits [`InputEvent::Submit`].
    Text,
    /// A single-line field that masks its displayed value.
    Password,
    /// A soft-wrapping multi-line editor. Enter inserts a newline and Ctrl/Cmd+Enter submits.
    Multiline,
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    Change(SharedString),
    Submit(SharedString),
}

impl InputEvent {
    pub fn text(&self) -> &SharedString {
        match self {
            InputEvent::Change(shared_string) => shared_string,
            InputEvent::Submit(shared_string) => shared_string,
        }
    }
}
