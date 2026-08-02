# gpui_effects

GPU-driven visual effects and reusable effect components for GPUI applications.
WGSL is the canonical shader implementation, with GPUI providing the render
pipeline and `gpui_effects` providing higher-level components and presets.

## Guides

- [Liquid glass](docs/liquid_glass.md): rendering model, layer ordering,
  parameter reference, recipes, animation, fallback behavior, and
  troubleshooting for `GlassPanel`.
- [Timed text](docs/timed_text.md): arbitrary character/word timings, gradient
  reveal, grouped lift/scale emphasis, and playback-clock integration.

## Examples

Run the interactive liquid-glass example from the workspace root:

```sh
cargo run -p gpui_effects --example liquid_glass
```

Other examples in `examples/` demonstrate gradients, masked effects, motion
layers, and page-flip effects.

Run the timed-text example (also shown inside liquid glass):

```sh
cargo run -p gpui_effects --example timed_text
```

## License

MIT. See [LICENSE](LICENSE).
