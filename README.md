# GPUI

This is an independently maintained GPUI repository extracted from [Zed](https://github.com/zed-industries/zed).

GPUI is a GPU-accelerated UI framework written in Rust. It provides core building blocks for element layout, text rendering, window management, input handling, state management, and asynchronous tasks. This repository retains the GPUI core, platform backends, and supporting crates while removing code specific to the Zed editor.

It currently includes support for Linux, macOS, Windows, and the web, along with the WGPU rendering backend and Tokio integration.

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
