# GPUI

This is an independently maintained GPUI repository extracted from [Zed](https://github.com/zed-industries/zed).

GPUI is a GPU-accelerated UI framework written in Rust. It provides core building blocks for element layout, text rendering, window management, input handling, state management, and asynchronous tasks. This repository retains the GPUI core, platform backends, and supporting crates while removing code specific to the Zed editor.

It currently includes support for Linux, macOS, Windows, and the web, along with the WGPU rendering backend and Tokio integration.

## What This Fork Adds

Development after the initial GPUI import focuses on reusable visual effects,
media playback, UI components, and desktop integration.

### `gpui_effects`

[`gpui_effects`](crates/gpui_effects) extends GPUI with reusable GPU-backed
visual components:

- Extensible WGSL effects with uniforms and zero-, one-, two-, or four-image
  inputs.
- Built-in Aurora, Plasma, Color Orbs, Album Glow, and Album Ripples effects.
- Shader effects and gradient fills masked by arbitrary elements, including
  ready-to-use text and SVG helpers.
- `FrostedGlass` panels with strong backdrop blur, light/dark appearances,
  normal `div()` layout behavior, and mergeable rounded surfaces.
- A page-flip component with rigid, soft, and curl styles, single- or
  double-page layouts, lazy content providers, and preloading.
- `MotionLayer` for coordinated movement along linear, curved, or custom paths.
- Timed text with character or word timing, gradient reveal, grouped emphasis,
  and playback-clock integration.

See the [glass guide](crates/gpui_effects/docs/glass.md) and the complete
[`gpui_effects` documentation](crates/gpui_effects/README.md).

### `gpui_media`

[`gpui_media`](crates/gpui_media) provides reusable video playback for GPUI
applications. See its [documentation](crates/gpui_media/README.md) for usage.

### Other Extensions

- Additional rendering and styling primitives, including per-side border
  colors, animated gradients, color SVGs, and platform backdrop effects.
- Desktop integrations such as native system trays, Wayland internal drag and
  drop, drag icons, and screen capture.
- [`uic`](uic), a reusable component library with generated Lucide icons and
  components such as `ColorPicker`.

## Getting Started

The Rust toolchain is defined in `rust-toolchain.toml`. After cloning the repository, run an example with:

```sh
cargo run --example hello_world
```

More examples:

```sh
cargo run --example image_gallery
cargo run --example text
cargo run --example svg
```

Check the entire workspace with:

```sh
cargo check --workspace
```

Example source code is available in [`crates/gpui/examples`](crates/gpui/examples).

## License

This is a mixed-license repository:

- GPUI-related code derived from Zed remains licensed under the Apache License
  2.0. See [LICENSE-APACHE](LICENSE-APACHE).
- The independently developed `gpui_effects`, `gpui_media`, `uic`, and
  `uic-macros` crates are licensed under the MIT License. See the `LICENSE` file
  in each crate.
- Third-party assets retain their original licenses. In particular, the Lucide
  icons bundled by `uic` retain the Lucide ISC and Feather MIT license text in
  [`uic/assets/icons/LICENSE`](uic/assets/icons/LICENSE).
