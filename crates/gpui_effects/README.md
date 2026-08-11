# gpui_effects

GPU-driven visual effects and reusable effect components for GPUI applications.
WGSL is the canonical shader implementation, with GPUI providing the render
pipeline and `gpui_effects` providing higher-level components and presets.

## Guides

- [Glass](docs/glass.md): Frosted, Gel, and Liquid materials for `GlassPanel`.
- [Timed text](docs/timed_text.md): arbitrary character/word timings, gradient
  reveal, grouped lift/scale emphasis, and playback-clock integration.

## Examples

Run the `GlassPanel` examples from the workspace root:

```sh
# Frosted and Gel
cargo run -p gpui_effects --example glass

# Liquid
cargo run -p gpui_effects --example liquid_glass
```

Other examples in `examples/` demonstrate gradients, masked effects, motion
layers, and page-flip effects.

Run the standalone timed-text example:

```sh
cargo run -p gpui_effects --example timed_text
```

## License

MIT. See [LICENSE](LICENSE).
