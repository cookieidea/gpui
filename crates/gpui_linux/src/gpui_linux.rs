#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::{HeadlessWindowFactory, current_platform, headless_platform_with_window_factory};
