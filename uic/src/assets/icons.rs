use crate::file_enum;
use gpui::SharedString;

#[file_enum(path = "assets/icons", ext = "svg", rename_all = "PascalCase")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LucideIcons {}

impl From<LucideIcons> for SharedString {
    fn from(icon: LucideIcons) -> Self {
        SharedString::from(icon.path())
    }
}

impl LucideIcons {
    pub fn path(&self) -> String {
        format!("icons/{self}")
    }
}

#[cfg(test)]
mod tests {
    use super::LucideIcons;

    #[test]
    fn generated_icon_names_include_the_extension() {
        assert_eq!(LucideIcons::ArrowDown.to_string(), "arrow-down.svg");
        assert_eq!(LucideIcons::Minus.path(), "icons/minus.svg");
        assert_eq!(LucideIcons::Volume2.path(), "icons/volume-2.svg");
    }
}
