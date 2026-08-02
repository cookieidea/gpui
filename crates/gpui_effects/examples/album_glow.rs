use std::{path::PathBuf, sync::Arc, time::Instant};

use gpui::{
    App, Bounds, Context, Image, ImageFormat, ImageSource, ObjectFit, Render, Window, WindowBounds,
    WindowOptions, div, img, prelude::*, px, rgba, size,
};
use gpui_effects::album_glow;
use gpui_platform::application;

struct AlbumGlowPreview {
    cover: ImageSource,
    started_at: Instant,
}

impl AlbumGlowPreview {
    fn new(cover: ImageSource) -> Self {
        Self {
            cover,
            started_at: Instant::now(),
        }
    }
}

impl Render for AlbumGlowPreview {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let elapsed = self.started_at.elapsed().as_secs_f32();

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(gpui::rgb(0x171922))
            .child(
                album_glow(self.cover.clone())
                    .time(elapsed)
                    .absolute()
                    .inset_0(),
            )
            .child(div().absolute().inset_0().bg(rgba(0x080a100d)))
            .child(
                img(self.cover.clone())
                    .id("album-cover")
                    .absolute()
                    .left(px(48.))
                    .bottom(px(48.))
                    .size(px(280.))
                    .object_fit(ObjectFit::Cover)
                    .rounded(px(28.))
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.), px(24.), rgba(0x00000080).into())
                            .blur_radius(px(64.))
                            .spread_radius(px(-16.)),
                    ]),
            )
    }
}

fn cover_source() -> ImageSource {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .map(ImageSource::from)
        .unwrap_or_else(|| {
            Arc::new(Image::from_bytes(
                ImageFormat::Svg,
                include_bytes!("album-cover.svg").to_vec(),
            ))
            .into()
        })
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AlbumGlowPreview::new(cover_source())),
        )
        .expect("failed to open album glow preview");
    });
}
