mod icons;
use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

pub use icons::LucideIcons;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
pub struct LucideAssets;

impl LucideAssets {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LucideAssets {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetSource for LucideAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|file| Some(file.data))
            .ok_or_else(|| anyhow!("embedded asset not found: {path}"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|item| item.starts_with(path).then(|| item.into()))
            .collect())
    }
}
