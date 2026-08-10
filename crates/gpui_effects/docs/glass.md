# Glass materials

[`GlassPanel`](crate::GlassPanel) is a container for frosted or elastic gel
glass. Content is painted after the material and remains crisp.

```rust,ignore
use gpui::{div, prelude::*, px};
use gpui_effects::{GlassMaterial, GlassPanel};

GlassPanel::frosted()
    .material(GlassMaterial::Regular)
    .radius(px(18.0))
    .p(px(20.0))
    .child(div().child("Settings"))
```

`GlassPanel::new()` is equivalent to `GlassPanel::frosted()`.

`GlassPanel` has the same default container style and child-layout behavior as
`div()`. The material is painted on that container without inserting hidden
flex/grid children or expanding its scrollable content. This makes an existing
`div()` replaceable with `GlassPanel::new()` while preserving its layout; add a
radius explicitly when the design calls for one.

## Styles

```rust,ignore
GlassPanel::frosted().child(content) // stable backdrop diffusion
GlassPanel::gel().child(content)     // elastic contour and interaction
```

Frosted glass scatters the backdrop without deforming it. Gel retains the
playful animated material: contour waves, movement stretch, press bulge,
interior flow, and release rebound.

## Material presets

| Material | Frosted blur | Gel blur |
| --- | ---: | ---: |
| `Thin` | 8 px | 10 px |
| `Regular` | 14 px | 14 px |
| `Thick` | 22 px | 22 px |

The preset selects blur, tint, edge, and material response. Override only the
blur with `.blur_radius(px(...))`.

## Common controls

| Method | Meaning |
| --- | --- |
| `style(...)` | Selects `Frosted` or `Gel` |
| `material(...)` | Selects `Thin`, `Regular`, or `Thick` |
| `blur_radius(...)` | Overrides backdrop blur |
| `radius(...)` | Sets container and shader radius together |
| `tint(...)` | Sets material tint and fallback color |
| `edge_color(...)` | Sets refractive edge color |
| `edge_visible(false)` | Hides material edge lighting and fallback border |
| `deformation(...)` | Scales Gel refraction and dispersion |
| `wave_strength(...)` | Scales Gel ambient motion |
| `translation_velocity(...)` | Supplies Gel body velocity in logical px/s |
| `animated(false)` | Disables Gel animation and pointer response |

## Gel movement

The component cannot infer velocity from `.left(...)` or `.top(...)`. Measure
the position delta in the owning view and pass logical pixels per second:

```rust,ignore
GlassPanel::gel()
    .id("draggable-gel")
    .translation_velocity(self.drag_velocity)
    .absolute()
    .left(self.position.x)
    .top(self.position.y)
```

About `600 px/s` reaches the reference strength; larger values are normalized
while preserving direction. Assign a unique `.id(...)` to every independently
interactive Gel panel.

## Layer order

Backdrop effects are paint-order dependent:

```text
earlier scene content
        ↓
raw + blurred backdrop capture
        ↓
GlassPanel material
        ↓
GlassPanel children
        ↓
later siblings
```

Earlier siblings are diffused or deformed. Children and later siblings are not.
Unsupported renderers retain the normal tint and border fallback.

Run the interactive Frosted and Gel comparison with:

```sh
cargo run -p gpui_effects --example glass
```
