# gpui_effects

GPU-driven visual effects and reusable effect components for GPUI applications.
WGSL is the canonical shader implementation, with GPUI providing the render
pipeline and `gpui_effects` providing higher-level components and presets.

## Guides

- [Liquid glass](docs/liquid_glass.md): rendering model, layer ordering,
  parameter reference, recipes, animation, fallback behavior, and
  troubleshooting for `GlassPanel`.

## Examples

Run the interactive liquid-glass example from the workspace root:

```sh
cargo run -p gpui_effects --example liquid_glass
```

Other examples in `examples/` demonstrate gradients, masked effects, motion
layers, and page-flip effects.

## License

MIT. See [LICENSE](LICENSE).
