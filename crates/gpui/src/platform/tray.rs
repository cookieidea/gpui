use crate::{Action, MenuItem, SharedString};
use anyhow::{Result, ensure};
use std::sync::Arc;

/// Identifies a system tray item created by an application.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TrayId(pub(crate) u32);

impl TrayId {
    /// Creates an identifier from a platform callback value.
    #[doc(hidden)]
    pub fn from_u32(id: u32) -> Self {
        Self(id)
    }

    /// Returns the process-local numeric identifier for this tray item.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

/// One RGBA image representation of a system tray icon.
#[derive(Clone, Debug)]
pub struct TrayIconImage {
    /// Pixels in non-premultiplied RGBA byte order, from top-left to bottom-right.
    pub rgba: Arc<[u8]>,
    /// Image width in physical pixels.
    pub width: u32,
    /// Image height in physical pixels.
    pub height: u32,
}

impl TrayIconImage {
    /// Creates an icon image from non-premultiplied RGBA pixels.
    pub fn new(rgba: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Result<Self> {
        ensure!(
            width > 0 && height > 0,
            "tray icon dimensions must be non-zero"
        );
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .map(|len| len as usize)
            .ok_or_else(|| anyhow::anyhow!("tray icon dimensions overflow"))?;
        let rgba = rgba.into();
        ensure!(
            rgba.len() == expected_len,
            "tray icon contains {} bytes, expected {expected_len}",
            rgba.len()
        );
        Ok(Self {
            rgba,
            width,
            height,
        })
    }
}

/// A system tray icon with one or more image representations.
#[derive(Clone, Debug)]
pub struct TrayIcon {
    images: Arc<[TrayIconImage]>,
    template: bool,
}

impl TrayIcon {
    /// Creates a tray icon with one RGBA image representation.
    pub fn from_rgba(rgba: impl Into<Arc<[u8]>>, width: u32, height: u32) -> Result<Self> {
        Self::from_images([TrayIconImage::new(rgba, width, height)?])
    }

    /// Creates a tray icon from multiple image representations.
    ///
    /// Supplying common sizes such as 16, 24, and 32 pixels lets the platform
    /// choose a sharp representation for the current display scale.
    pub fn from_images(images: impl IntoIterator<Item = TrayIconImage>) -> Result<Self> {
        let images = images.into_iter().collect::<Vec<_>>();
        ensure!(
            !images.is_empty(),
            "a tray icon must contain at least one image"
        );
        Ok(Self {
            images: images.into(),
            template: false,
        })
    }

    /// Marks this as a monochrome template icon.
    ///
    /// macOS uses template icons to adapt automatically to the menu bar
    /// appearance. Other platforms may ignore this hint.
    pub fn template(mut self, template: bool) -> Self {
        self.template = template;
        self
    }

    /// Returns the available image representations.
    pub fn images(&self) -> &[TrayIconImage] {
        &self.images
    }

    /// Returns whether this icon should be treated as a monochrome template.
    pub fn is_template(&self) -> bool {
        self.template
    }
}

/// The axis associated with a tray scroll event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrayScrollAxis {
    /// Horizontal scrolling.
    Horizontal,
    /// Vertical scrolling.
    Vertical,
}

/// An interaction delivered by the platform for a system tray item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrayEvent {
    /// The tray item was activated using the platform's primary activation gesture.
    PrimaryActivate,
    /// The tray item was activated using the platform's secondary activation gesture.
    SecondaryActivate,
    /// The user scrolled over the tray item.
    Scroll {
        /// The platform-provided scroll amount.
        delta: f32,
        /// The scroll axis.
        axis: TrayScrollAxis,
    },
}

/// Configuration used to create or replace a system tray item.
pub struct TrayOptions {
    /// The icon shown by the system tray host.
    pub icon: TrayIcon,
    /// Optional hover text shown by the tray host.
    pub tooltip: Option<SharedString>,
    /// The native context menu associated with the tray item.
    pub menu: Vec<MenuItem>,
    /// An action dispatched for the platform's primary activation gesture.
    pub activate: Option<Box<dyn Action>>,
}

impl TrayOptions {
    /// Creates tray options with the given icon.
    pub fn new(icon: TrayIcon) -> Self {
        Self {
            icon,
            tooltip: None,
            menu: Vec::new(),
            activate: None,
        }
    }

    /// Sets the tray tooltip.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Sets the native tray menu.
    pub fn menu(mut self, menu: impl IntoIterator<Item = MenuItem>) -> Self {
        self.menu = menu.into_iter().collect();
        self
    }

    /// Sets the action dispatched when the tray item is primarily activated.
    pub fn on_activate(mut self, action: impl Action) -> Self {
        self.activate = Some(Box::new(action));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_tray_icon_pixels() {
        assert!(TrayIcon::from_rgba([0_u8; 16], 2, 2).is_ok());
        assert!(TrayIcon::from_rgba([0_u8; 15], 2, 2).is_err());
        assert!(TrayIcon::from_rgba([], 0, 1).is_err());
        assert!(TrayIcon::from_images([]).is_err());
    }
}
