# Liquid glass usage guide

[`GlassPanel`](crate::GlassPanel) is a container with a dynamic glass material.
Use it like a GPUI `div`: set its size and layout, then add text, icons, or
controls with `.child(...)` and `.children(...)`.

## Basic usage

```rust,ignore
use std::time::Duration;

use gpui::{div, prelude::*, px, rgba};
use gpui_effects::{GlassMaterial, GlassPanel};

GlassPanel::new()
    .id("settings-panel")
    .animation("settings-glass", Duration::from_secs(7))
    .material(GlassMaterial::Regular)
    .radius(px(18.0))
    .tint(rgba(0x18203330))
    .edge_color(rgba(0xffffff45))
    .w(px(360.0))
    .p(px(20.0))
    .child(div().child("Settings"))
```

`GlassPanel` supports normal GPUI layout methods such as `.absolute()`,
`.flex()`, `.w(...)`, `.p(...)`, `.shadow(...)`, and `.child(...)`.

## Material and custom blur

Start with a material preset:

| Material | Default blur | Visual result | Good for |
| --- | ---: | --- | --- |
| `Thin` | 10 px | Clear backdrop, light diffusion | toolbars, small floating controls |
| `Regular` | 14 px | Balanced glass | cards, popovers, general panels |
| `Thick` | 22 px | Softer backdrop, denser material | menus and readability-first overlays |

The preset chooses blur, refraction, edge, and motion defaults. To keep a
preset but use a custom blur size, add:

```rust,ignore
GlassPanel::new()
    .material(GlassMaterial::Regular)
    .blur_radius(px(18.0))
```

`blur_radius` only changes backdrop blur. It does not directly change tint,
refraction strength, or glass opacity.

The exact preset values are:

| Material | `optics` | `surface` |
| --- | --- | --- |
| `Thin` | `[10.0, 1.0, 0.62, 1.08]` | `[0.13, 0.34, 0.64, 0.72]` |
| `Regular` | `[15.0, 1.8, 0.56, 1.12]` | `[0.18, 0.52, 0.76, 1.00]` |
| `Thick` | `[19.0, 2.4, 0.42, 1.14]` | `[0.22, 0.62, 0.82, 0.86]` |

You normally do not need to copy these arrays. They are useful when starting
from a preset and changing one advanced value.

## Important parameters

| Method | Meaning | Typical values |
| --- | --- | --- |
| `material(...)` | Selects the base material preset | `Thin`, `Regular`, `Thick` |
| `blur_radius(px(...))` | Overrides the preset blur size | `8..24 px` |
| `radius(px(...))` | Sets the visible and shader corner radius together | Match the component shape |
| `tint(color)` | Sets glass color; the color alpha controls tint strength | Alpha `0.05..0.80` |
| `glass_opacity(value)` | Fades the complete glass result back toward the original scene | Usually `1.0` |
| `edge_color(color)` | Sets the bright refractive edge color | Usually white or a cool neutral |
| `deformation(value)` | Scales refraction, lens magnification, and dispersion | `0.6..1.5` |
| `wave_strength(value)` | Scales the slow default surface flow | `0.0..1.2` |
| `animated(false)` | Disables continuous ambient/pointer animation | Static or reduced-motion UI |
| `translation_velocity(point)` | Adds stretch and inertia while the panel itself moves | Logical pixels per second |

For normal application UI, tune the parameters above before using `optics`,
`surface`, or `shader_tint`.

## Controlling transparency

`glass_opacity` is the opacity of the **finished glass effect**, not simply the
opacity of its dark background.

- `glass_opacity(1.0)` displays the complete blur, tint, refraction, and edge.
- Lower values fade that result and reveal more of the original scene.
- `glass_opacity(0.0)` hides the material, but panel children remain visible.

Therefore, if a menu is too transparent, do not start by lowering
`glass_opacity`. Keep it at `1.0` and use one or more of these:

```rust,ignore
.material(GlassMaterial::Thick) // more blur
.blur_radius(px(22.0))          // custom blur
.tint(rgba(0x101624b8))         // stronger dark tint
.optics([12.0, 1.2, 0.30, 1.0]) // less raw backdrop detail
```

If the glass is too dense, reduce the `tint` alpha, select `Thin`, or increase
the third `optics` value.

## Controlling the glass edge

The refractive glass edge and a normal GPUI border are separate effects.

### Refractive material edge

```rust,ignore
.edge_color(rgba(0xdceaffd0))
.surface([0.16, 0.55, 0.75, 0.80])
```

- `edge_color(...)` controls edge color; its alpha controls base edge strength.
- `surface[0]` controls edge width. Try `0.10..0.25`.
- `surface[1]` multiplies edge brightness. Try `0.25..0.75`.

The selected material and custom blur radius can also make the optical rim look
slightly thicker or softer.

### Normal GPUI border

```rust,ignore
.border_1()
.border_color(rgba(0xffffff20))
```

This draws an ordinary fixed outline. It does not refract or animate. It can be
combined with `edge_color`, but using both at high strength usually makes the
panel edge look too thick.

## Refraction controls

For advanced tuning:

```rust,ignore
.optics([refraction, dispersion, raw_detail, saturation])
```

| Index | Meaning | Suggested range |
| ---: | --- | ---: |
| `0` | Refraction displacement in pixels | `8.0..22.0` |
| `1` | RGB color dispersion in pixels | `0.5..2.5` |
| `2` | Blurred (`0`) to raw/refracted (`1`) backdrop detail | `0.20..0.70` |
| `3` | Backdrop saturation; `1` keeps the original saturation | `0.9..1.2` |

`deformation(...)` also scales refraction, dispersion, and lens magnification.
Use `optics` to choose the character of the material and `deformation` as the
overall effect-strength control.

## Surface and interaction controls

```rust,ignore
.surface([edge_width, edge_brightness, press_response, ambient_motion])
```

These are dimensionless material coefficients, not pixels:

| Index | Meaning and scale | Suggested range |
| ---: | --- | ---: |
| `0` | Edge width relative to the material's internal thickness; shader-clamped to `0.001..=0.5` | `0.10..0.25` |
| `1` | Multiplier for refractive edge-light brightness | `0.25..0.75` |
| `2` | Gain applied to press-driven local refraction | `0.4..1.0` |
| `3` | Multiplier for ambient flow and contour motion | `0.5..1.2` |

`wave_strength(...)` multiplies the fourth value. For example,
`.surface([0.16, 0.55, 0.75, 1.0]).wave_strength(0.4)` keeps only 40% of the
ambient motion.

Press and release response is built in. Moving the glass component itself is
different: draggable or animated panels should measure their velocity and pass
it through `translation_velocity(...)` on each render.

### Overall deformation

`deformation(...)` is the quickest way to scale the optical deformation while
keeping the selected material character:

- `0.0`: no refraction, dispersion, or lens magnification; blur, tint, and the
  edge remain visible;
- `0.6..0.9`: restrained application UI;
- `1.0`: the selected material's normal strength;
- `1.1..1.5`: more obvious decorative deformation.

Very large values make background text difficult to recognize.

### Ambient animation

`wave_strength(...)` scales the slow continuous flow. It does not control the
press/release spring or explicit translation velocity.

- `0.0`: no default wave;
- `0.3..0.7`: subtle application UI;
- `1.0`: the material preset's normal motion;
- values above `1.0`: stronger moving contour and backdrop flow.

Use `.animated(false)` for a completely static or reduced-motion surface. It
disables the continuous shader loop and ambient/pointer motion. Explicit
translation velocity can still be supplied.

### Moving or draggable glass

The panel cannot infer its velocity from `.left(...)` or `.top(...)`. Measure
the position change in the owning view and pass logical pixels per second:

```rust,ignore
GlassPanel::new()
    .id("draggable-inspector")
    .animation("draggable-inspector-glass", Duration::from_secs(6))
    .translation_velocity(self.drag_velocity)
    .absolute()
    .left(self.position.x)
    .top(self.position.y)
```

Update the velocity on each render and decay or reset it when movement stops.
About `600 px/s` reaches the reference motion strength; larger values are
normalized while preserving direction.

## Tint and `shader_tint`

Most callers should only use `tint(...)`:

```rust,ignore
.tint(rgba(0x10162490))
```

For precise shader color control, `shader_tint` accepts normalized red, green,
blue, and tint strength:

```rust,ignore
.tint(rgba(0x10162490)) // also used by the fallback renderer
.shader_tint([0.035, 0.055, 0.10, 0.62])
```

When `shader_tint` is set, it replaces `tint` on the full shader path. Keep a
similar `tint` value so the fallback appearance remains consistent.

## IDs and helper return types

Assign a stable, unique id to each interactive panel:

```rust,ignore
GlassPanel::new().id("file-toolbar-glass")
```

`GlassPanel::id(...)` returns `GlassPanel`, so helper functions should return
`GlassPanel` or `impl IntoElement`, not `Stateful<GlassPanel>`:

```rust,ignore
fn toolbar_group(id: &'static str, buttons: Vec<gpui::AnyElement>) -> GlassPanel {
    GlassPanel::new()
        .id(id)
        .h(px(40.0))
        .px(px(3.0))
        .flex()
        .items_center()
        .children(buttons)
}
```

`.animation(id, duration)` configures the material animation. If `.id(...)` is
not supplied, its animation id is also used for interaction state, so it must
then be unique.

The animation duration controls one ambient shader cycle. It does not control
how long the press/release spring takes.

## Layer order

Backdrop effects are paint-order dependent:

```text
earlier siblings / scene content
              |
              v
       raw + blurred capture
              |
              v
       GlassPanel material
              |
              v
       GlassPanel children
              |
              v
        later siblings
```

Glass only refracts content that is painted before it:

- Earlier siblings are behind the glass and are refracted.
- `.child(...)` content is sharp and appears on the glass.
- Later siblings appear above the glass and are unchanged.

Put menu labels, icons, and buttons inside the `GlassPanel`. If they are
distorted, they were probably painted before the panel instead.

There is currently no separate “inside the liquid” layer. An object must be
either behind the glass and refracted, or above/inside the component and kept
sharp.

## Recipes

### Readable context menu

```rust,ignore
GlassPanel::new()
    .id("finder-context-menu")
    .animation("finder-context-menu-glass", Duration::from_secs(7))
    .material(GlassMaterial::Thick)
    .blur_radius(px(20.0))
    .radius(px(18.0))
    .tint(rgba(0x101624b8))
    .glass_opacity(1.0)
    .edge_color(rgba(0xc6d6f06b))
    .optics([12.0, 1.35, 0.30, 1.05])
    .surface([0.14, 0.48, 0.75, 0.60])
    .deformation(0.9)
    .wave_strength(0.55)
    .w(px(226.0))
    .p(px(6.0))
    .child(menu_items)
```

### Light floating toolbar

```rust,ignore
GlassPanel::new()
    .id("editor-toolbar")
    .animation("editor-toolbar-glass", Duration::from_secs(8))
    .material(GlassMaterial::Thin)
    .radius(px(14.0))
    .tint(rgba(0x18203335))
    .glass_opacity(1.0)
    .edge_color(rgba(0xffffff40))
    .deformation(0.75)
    .wave_strength(0.35)
    .child(toolbar_content)
```

## Renderer fallback

When backdrop shaders are unavailable, the panel keeps its layout and children
and displays a translucent tint and border. Blur, refraction, dispersion, and
dynamic edge lighting require backdrop support.

Use `window.supports_backdrop_blur()` only when the application needs to show a
diagnostic or choose a different layout. Normal callers do not need to branch;
`GlassPanel` selects its fallback automatically.

## Troubleshooting

### Lowering `glass_opacity` makes the background clearer

This is expected because it fades the finished glass result. Restore
`glass_opacity(1.0)`, then increase tint alpha, increase blur, or reduce
`optics[2]`.

### The edge is too thick or too bright

Reduce `edge_color` alpha, `surface[0]`, or `surface[1]`. If `.border_1()` is
also present, remove it temporarily to determine whether the visible line is
the refractive edge or the normal GPUI border.

### Text or icons are distorted

Put them inside `.child(...)` or paint them after the panel. Earlier siblings
are intentionally sampled by the glass.

### The background does not refract

Check that backdrop support is available, the background is painted before the
panel, the panel has non-zero bounds, and both `glass_opacity` and `deformation`
are greater than zero.

### Multiple panels react together

Give every independently interactive panel a unique `.id(...)`. If no element
id is supplied, its animation id becomes the interaction-state key.

### A dragged panel moves without stretching

Position changes alone do not provide motion information. Measure velocity and
pass it through `translation_velocity(...)` every render.

### Rounded corners and the glass silhouette do not match

Use `GlassPanel::radius(...)`. A generic rounded style only changes the normal
GPUI container and does not update the material geometry.

Run the interactive example with:

```sh
cargo run -p gpui_effects --example liquid_glass
```
