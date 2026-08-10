use std::time::{Duration, Instant};

use gpui::{
    AbsoluteLength, BorderStyle, Bounds, Div, Edges, EffectUniforms, ElementId, Hsla,
    InteractiveElement, Interactivity, IntoElement, MouseButton, PaintBackdropEffect,
    ParentElement, Pixels, Point, RenderOnce, Rgba, StyleRefinement, Styled, div, hsla, point, px,
    quad, size,
};

use crate::{
    GLASS_GEOMETRY_SLOT, GLASS_INTERACTION_SLOT, GLASS_LIGHT_SLOT, GLASS_OPTICS_SLOT,
    GLASS_SURFACE_SLOT, GLASS_TINT_SLOT, frosted_glass_shader, gel_glass_shader,
};

const GLASS_OVERSCAN: f32 = 32.0;
const GLASS_REFERENCE_VELOCITY: f32 = 600.0;

fn clamp_unit_velocity(velocity: Point<f32>) -> (Point<f32>, f32) {
    let magnitude = (velocity.x * velocity.x + velocity.y * velocity.y).sqrt();
    if magnitude <= 1.0 {
        return (velocity, magnitude);
    }
    (
        Point {
            x: velocity.x / magnitude,
            y: velocity.y / magnitude,
        },
        1.0,
    )
}

fn normalize_translation_velocity(velocity: Point<f32>) -> Point<f32> {
    clamp_unit_velocity(Point {
        x: velocity.x / GLASS_REFERENCE_VELOCITY,
        y: velocity.y / GLASS_REFERENCE_VELOCITY,
    })
    .0
}

fn scale_glass_pixel_uniforms(
    mut uniforms: EffectUniforms,
    scale_factor: f32,
    radius: Pixels,
) -> EffectUniforms {
    let mut optics = uniforms.slots()[GLASS_OPTICS_SLOT];
    optics[0] *= scale_factor;
    optics[1] *= scale_factor;
    uniforms.set_slot(GLASS_OPTICS_SLOT, optics);

    let mut geometry = uniforms.slots()[GLASS_GEOMETRY_SLOT];
    geometry[0] *= scale_factor;
    geometry[1] = radius.as_f32() * scale_factor;
    geometry[2] *= scale_factor;
    uniforms.set_slot(GLASS_GEOMETRY_SLOT, geometry);
    uniforms
}

#[derive(Clone, Copy, Debug, Default)]
struct GlassInteractionFrame {
    pressure: f32,
    velocity: Point<f32>,
}

#[derive(Debug)]
struct GlassInteractionState {
    pressure: f32,
    pressure_velocity: f32,
    target_pressure: f32,
    pointer_velocity: Point<f32>,
    last_pointer: Option<Point<Pixels>>,
    last_pointer_at: Instant,
    last_frame_at: Instant,
}

impl Default for GlassInteractionState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            pressure: 0.0,
            pressure_velocity: 0.0,
            target_pressure: 0.0,
            pointer_velocity: Point::default(),
            last_pointer: None,
            last_pointer_at: now,
            last_frame_at: now,
        }
    }
}

impl GlassInteractionState {
    fn set_pressed(&mut self, pressed: bool, position: Point<Pixels>, style: GlassStyle) {
        self.target_pressure = if style == GlassStyle::Gel && pressed {
            1.0
        } else {
            0.0
        };
        self.last_pointer = Some(position);
        self.last_pointer_at = Instant::now();
    }

    fn record_pointer(&mut self, position: Point<Pixels>, style: GlassStyle) {
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_pointer_at)
            .as_secs_f32();
        self.record_pointer_by(position, elapsed, style);
        self.last_pointer_at = now;
    }

    fn record_pointer_by(&mut self, position: Point<Pixels>, elapsed: f32, style: GlassStyle) {
        if style != GlassStyle::Gel || self.target_pressure < 0.5 {
            self.pointer_velocity = Point::default();
            self.last_pointer = Some(position);
            return;
        }
        if let Some(previous) = self.last_pointer
            && (0.001..=0.12).contains(&elapsed)
        {
            let measured = Point {
                x: ((position.x.as_f32() - previous.x.as_f32()) / elapsed / 900.0).clamp(-1.0, 1.0),
                y: ((position.y.as_f32() - previous.y.as_f32()) / elapsed / 900.0).clamp(-1.0, 1.0),
            };
            let blend = 1.0 - (-elapsed * 22.0).exp();
            self.pointer_velocity.x += (measured.x - self.pointer_velocity.x) * blend;
            self.pointer_velocity.y += (measured.y - self.pointer_velocity.y) * blend;
        }
        self.last_pointer = Some(position);
    }

    fn advance(&mut self, style: GlassStyle) -> GlassInteractionFrame {
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_frame_at)
            .as_secs_f32()
            .clamp(0.0, 1.0 / 20.0);
        self.last_frame_at = now;
        self.advance_by(elapsed, style)
    }

    fn advance_by(&mut self, elapsed: f32, style: GlassStyle) -> GlassInteractionFrame {
        let elapsed = elapsed.clamp(0.0, 1.0 / 20.0);
        if style != GlassStyle::Gel {
            self.pressure = 0.0;
            self.pressure_velocity = 0.0;
            self.target_pressure = 0.0;
            self.pointer_velocity = Point::default();
            return GlassInteractionFrame::default();
        }

        let stiffness = 185.0;
        let damping = 21.0;
        let velocity_decay = 6.5;
        let acceleration =
            (self.target_pressure - self.pressure) * stiffness - self.pressure_velocity * damping;
        self.pressure_velocity += acceleration * elapsed;
        self.pressure += self.pressure_velocity * elapsed;
        self.pressure = self.pressure.clamp(-0.12, 1.08);

        let pointer_decay = (-elapsed * velocity_decay).exp();
        self.pointer_velocity.x *= pointer_decay;
        self.pointer_velocity.y *= pointer_decay;
        GlassInteractionFrame {
            pressure: self.pressure,
            velocity: self.pointer_velocity,
        }
    }
}

/// Material style and interaction model used by [`GlassPanel`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GlassStyle {
    /// Stable diffuse glass whose backdrop is scattered rather than lensed.
    #[default]
    Frosted,
    /// Continuously animated elastic glass with pronounced deformation.
    Gel,
}

/// Optical density presets for [`GlassPanel`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GlassMaterial {
    /// Light blur with more of the original backdrop visible.
    Thin,
    /// Balanced refraction, blur, and surface highlight.
    #[default]
    Regular,
    /// Strong diffusion and a denser glass edge.
    Thick,
}

impl GlassMaterial {
    fn parameters(self, style: GlassStyle) -> (Pixels, [f32; 4], [f32; 4]) {
        match (style, self) {
            (GlassStyle::Frosted, Self::Thin) => {
                (px(8.0), [0.0, 0.0, 0.34, 1.04], [0.12, 0.30, 0.0, 0.0])
            }
            (GlassStyle::Frosted, Self::Regular) => {
                (px(14.0), [0.0, 0.0, 0.18, 1.02], [0.16, 0.42, 0.0, 0.0])
            }
            (GlassStyle::Frosted, Self::Thick) => {
                (px(22.0), [0.0, 0.0, 0.08, 1.0], [0.20, 0.55, 0.0, 0.0])
            }
            (GlassStyle::Gel, Self::Thin) => {
                (px(10.0), [10.0, 1.0, 0.62, 1.08], [0.13, 0.34, 0.64, 0.72])
            }
            (GlassStyle::Gel, Self::Regular) => {
                (px(14.0), [15.0, 1.8, 0.56, 1.12], [0.18, 0.52, 0.76, 1.0])
            }
            (GlassStyle::Gel, Self::Thick) => {
                (px(22.0), [19.0, 2.4, 0.42, 1.14], [0.22, 0.62, 0.82, 0.86])
            }
        }
    }
}

/// A container backed by frosted or elastic gel glass.
///
/// Content is painted after the material, while the effect itself samples only
/// scene content already behind the panel. On renderers without backdrop
/// support, the translucent tint and border remain visible as a fallback.
/// Its container has the same default style and child layout behavior as
/// [`gpui::div`]; the material is painted without adding layout children.
///
/// See the [`glass_guide`](crate::glass_guide) for the rendering model,
/// layer-ordering rules, recipes, and a complete parameter reference.
pub struct GlassPanel {
    div: Div,
    style: GlassStyle,
    material: GlassMaterial,
    blur_radius: Option<Pixels>,
    tint: Option<Hsla>,
    edge_color: Hsla,
    edge_visible: bool,
    animated: bool,
    animation_id: ElementId,
    animation_duration: Duration,
    optics: Option<[f32; 4]>,
    surface: Option<[f32; 4]>,
    shader_tint: Option<[f32; 4]>,
    translation_velocity: Point<f32>,
    deformation: f32,
    wave_strength: f32,
    glass_opacity: f32,
}

impl Default for GlassPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl GlassPanel {
    /// Creates regular frosted glass.
    pub fn new() -> Self {
        Self {
            div: div(),
            style: GlassStyle::Frosted,
            material: GlassMaterial::Regular,
            blur_radius: None,
            tint: None,
            edge_color: hsla(0.0, 0.0, 1.0, 0.78),
            edge_visible: true,
            animated: true,
            animation_id: "gpui-liquid-glass".into(),
            animation_duration: Duration::from_secs(7),
            optics: None,
            surface: None,
            shader_tint: None,
            translation_velocity: Point::default(),
            deformation: 1.0,
            wave_strength: 1.0,
            glass_opacity: 1.0,
        }
    }

    /// Creates stable diffuse glass with backdrop scattering and blur.
    pub fn frosted() -> Self {
        Self::new().style(GlassStyle::Frosted)
    }

    /// Creates continuously animated elastic gel glass.
    pub fn gel() -> Self {
        Self::new().style(GlassStyle::Gel)
    }

    /// Selects the frosted or elastic gel material model.
    pub fn style(mut self, style: GlassStyle) -> Self {
        self.style = style;
        self
    }

    /// Assigns the standard GPUI element id without wrapping the component.
    ///
    /// `GlassPanel` is a [`RenderOnce`] component rather than a low-level
    /// [`gpui::Element`], so the blanket [`InteractiveElement::id`] method
    /// would produce an unusable `Stateful<GlassPanel>`. This inherent method
    /// forwards the id to the internal [`Div`] and intentionally returns
    /// `Self`, preserving component-specific and [`ParentElement`] chaining.
    /// The id also isolates Gel press and pointer interaction state.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.div.interactivity().element_id = Some(id.into());
        self
    }

    /// Selects the preset blur radius, optics, and surface parameters.
    ///
    /// Explicit [`Self::optics`] and [`Self::surface`] values override their
    /// respective preset arrays.
    pub fn material(mut self, material: GlassMaterial) -> Self {
        self.material = material;
        self
    }

    /// Overrides the material preset's backdrop blur radius.
    pub fn blur_radius(mut self, radius: Pixels) -> Self {
        self.blur_radius = Some(px(radius.as_f32().max(0.0)));
        self
    }

    /// Sets a uniform radius for both the GPUI container and glass silhouette.
    ///
    /// Generic [`Styled`] rounding methods are also reflected by the material;
    /// this method is the convenient uniform-radius form.
    pub fn radius(mut self, radius: impl Into<AbsoluteLength>) -> Self {
        let radius = radius.into();
        self.div = self.div.rounded(radius);
        self
    }

    /// Sets the conventional material tint and unsupported-renderer fallback.
    ///
    /// Its alpha is used as shader tint strength unless [`Self::shader_tint`]
    /// is set.
    pub fn tint(mut self, tint: impl Into<Hsla>) -> Self {
        self.tint = Some(tint.into());
        self
    }

    /// Sets the color and base strength of the bright refractive material edge.
    ///
    /// This is separate from the normal GPUI [`Styled::border_color`]. Edge
    /// width and additional brightness are controlled by the first two values
    /// passed to [`Self::surface`].
    pub fn edge_color(mut self, color: impl Into<Hsla>) -> Self {
        self.edge_color = color.into();
        self
    }

    /// Shows or hides the bright and shaded material edge.
    ///
    /// Hiding the edge preserves the glass silhouette, tint, blur, and
    /// refraction. It also removes the unsupported-renderer fallback border.
    pub fn edge_visible(mut self, visible: bool) -> Self {
        self.edge_visible = visible;
        self
    }

    /// Enables or disables Gel's time- and pointer-driven response.
    ///
    /// Frosted is time-independent; its pixels still update when the live
    /// backdrop or panel geometry changes.
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    /// Sets the shader animation identity and ambient loop duration.
    ///
    /// The duration controls one shader cycle, not the press/release spring.
    /// When [`Self::id`] is absent, this id is also the fallback key for the
    /// panel's interaction state.
    pub fn animation(mut self, id: impl Into<ElementId>, duration: Duration) -> Self {
        self.animation_id = id.into();
        self.animation_duration = duration;
        self
    }

    /// Overrides `[refraction_px, dispersion_px, raw_detail, saturation]`.
    ///
    /// `raw_detail` mixes blurred (`0`) and raw/refracted (`1`) backdrop
    /// samples. See [`crate::glass_guide`] for ranges and preset values.
    pub fn optics(mut self, value: [f32; 4]) -> Self {
        self.optics = Some(value);
        self
    }

    /// Overrides `[edge_width, highlight, pointer_response, style_parameter]`.
    ///
    /// All four values are dimensionless material coefficients:
    ///
    /// - `edge_width` is relative to the material's internal edge thickness
    ///   and is clamped to `0.001..=0.5` by the shader;
    /// - `highlight` multiplies refractive edge light;
    /// - `pointer_response` scales Gel's press-driven local refraction and is
    ///   ignored by Frosted;
    /// - `style_parameter` controls Gel's continuous contour/interior flow.
    ///   Frosted ignores it. [`Self::wave_strength`] scales it for Gel.
    pub fn surface(mut self, value: [f32; 4]) -> Self {
        self.surface = Some(value);
        self
    }

    /// Overrides `[red, green, blue, strength]` for the shader-backed path.
    ///
    /// When set, this takes precedence over [`Self::tint`] in the shader;
    /// `tint` remains the unsupported-renderer fallback color.
    pub fn shader_tint(mut self, value: [f32; 4]) -> Self {
        self.shader_tint = Some(value);
        self
    }

    /// Scales refraction, lens magnification, and chromatic dispersion.
    ///
    /// Zero keeps the backdrop undistorted while retaining blur, tint, and
    /// edge lighting. Negative values are clamped to zero.
    pub fn deformation(mut self, strength: f32) -> Self {
        self.deformation = strength.max(0.0);
        self
    }

    /// Scales Gel's continuous material motion.
    ///
    /// Frosted ignores this setting. Negative values are clamped to zero.
    pub fn wave_strength(mut self, strength: f32) -> Self {
        self.wave_strength = strength.max(0.0);
        self
    }

    /// Sets post-shader compositing opacity in `0.0..=1.0`.
    ///
    /// This fades the complete refraction, blur, tint, dispersion, and edge
    /// result back toward the original scene. It does not directly control
    /// backdrop visibility; for a less see-through panel, normally leave this
    /// at `1.0` and increase tint strength or reduce `optics[2]`.
    pub fn glass_opacity(mut self, opacity: f32) -> Self {
        self.glass_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Supplies the panel's translation velocity in logical pixels per second.
    ///
    /// Pointer motion describes interaction across the surface, while this
    /// value describes movement of the glass body itself. Draggable or
    /// spring-animated Gel callers should update it every frame. Frosted and
    /// Frosted ignores it.
    /// Around 600 px/s reaches the reference motion strength; larger values
    /// are normalized without changing direction.
    pub fn translation_velocity(mut self, velocity: Point<f32>) -> Self {
        self.translation_velocity = velocity;
        self
    }
}

impl Styled for GlassPanel {
    fn style(&mut self) -> &mut StyleRefinement {
        self.div.style()
    }
}

impl InteractiveElement for GlassPanel {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.div.interactivity()
    }
}

impl ParentElement for GlassPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = gpui::AnyElement>) {
        self.div.extend(elements);
    }
}

impl IntoElement for GlassPanel {
    type Element = gpui::ViewElement<Self>;

    #[track_caller]
    fn into_element(self) -> Self::Element {
        gpui::ViewElement::new(self)
    }
}

impl RenderOnce for GlassPanel {
    fn render(mut self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let (preset_blur_radius, preset_optics, preset_surface) =
            self.material.parameters(self.style);
        let blur_radius = self.blur_radius.unwrap_or(preset_blur_radius);
        let backdrop_supported = window.supports_backdrop_blur();
        let interaction_id = self
            .interactivity()
            .element_id
            .clone()
            .unwrap_or_else(|| self.animation_id.clone());
        let interaction =
            window.use_keyed_state(interaction_id, cx, |_, _| GlassInteractionState::default());
        let interaction_frame = interaction.update(cx, |state, _| state.advance(self.style));
        let translation_velocity = if self.style == GlassStyle::Gel {
            normalize_translation_velocity(self.translation_velocity)
        } else {
            Point::default()
        };
        let (material_velocity, material_speed) = clamp_unit_velocity(Point {
            x: interaction_frame.velocity.x + translation_velocity.x,
            y: interaction_frame.velocity.y + translation_velocity.y,
        });
        let tint: Rgba = self
            .tint
            .unwrap_or_else(|| match self.style {
                GlassStyle::Frosted => hsla(0.58, 0.45, 0.96, 0.10),
                GlassStyle::Gel => hsla(0.58, 0.55, 0.96, 0.055),
            })
            .into();
        let mut edge_light: Rgba = self.edge_color.into();
        if !self.edge_visible {
            edge_light.a = 0.0;
        }
        let mut optics = self.optics.unwrap_or(preset_optics);
        optics[0] *= self.deformation;
        optics[1] *= self.deformation;
        let mut surface = self.surface.unwrap_or(preset_surface);
        if self.style == GlassStyle::Gel {
            surface[3] *= self.wave_strength;
        }
        let should_animate = self.animated && backdrop_supported && self.style == GlassStyle::Gel;
        let mut uniforms = EffectUniforms::new()
            .with_slot(GLASS_OPTICS_SLOT, optics)
            .with_slot(GLASS_SURFACE_SLOT, surface)
            .with_slot(
                GLASS_TINT_SLOT,
                self.shader_tint.unwrap_or([tint.r, tint.g, tint.b, tint.a]),
            )
            .with_slot(
                GLASS_GEOMETRY_SLOT,
                [
                    GLASS_OVERSCAN,
                    0.0,
                    18.0 + blur_radius.as_f32() * 0.22,
                    0.092 * self.deformation,
                ],
            )
            .with_slot(
                GLASS_INTERACTION_SLOT,
                [
                    interaction_frame.pressure,
                    material_velocity.x,
                    material_velocity.y,
                    material_speed,
                ],
            )
            .with_slot(
                GLASS_LIGHT_SLOT,
                [edge_light.r, edge_light.g, edge_light.b, edge_light.a],
            );
        if (self.style == GlassStyle::Gel && !self.animated) || !backdrop_supported {
            let mut surface = uniforms.slots()[GLASS_SURFACE_SLOT];
            surface[2] = 0.0;
            surface[3] = 0.0;
            uniforms.set_slot(GLASS_SURFACE_SLOT, surface);
            uniforms.set_slot(GLASS_INTERACTION_SLOT, [0.0; 4]);
        }

        let shader = match self.style {
            GlassStyle::Frosted => frosted_glass_shader(),
            GlassStyle::Gel => gel_glass_shader(),
        };
        let animation_time = if should_animate {
            let animation_started_at =
                window.use_keyed_state(self.animation_id, cx, |_, _| Instant::now());
            window.request_animation_frame();
            animation_started_at
                .read(cx)
                .elapsed()
                .as_secs_f32()
                .rem_euclid(self.animation_duration.as_secs_f32().max(f32::EPSILON))
                / self.animation_duration.as_secs_f32().max(f32::EPSILON)
        } else {
            0.0
        };
        let glass_opacity = self.glass_opacity;
        let fallback_edge_color =
            self.edge_color
                .opacity(if self.edge_visible { 1.0 } else { 0.0 });
        let decoration = move |bounds: Bounds<Pixels>,
                               resolved_style: &gpui::Style,
                               window: &mut gpui::Window,
                               _cx: &mut gpui::App| {
            let corner_radii = resolved_style
                .corner_radii
                .clone()
                .to_pixels(window.rem_size())
                .clamp_radii_for_quad_size(bounds.size);
            let shader_radius = corner_radii
                .top_left
                .max(corner_radii.top_right)
                .max(corner_radii.bottom_right)
                .max(corner_radii.bottom_left);
            let paint_uniforms =
                scale_glass_pixel_uniforms(uniforms, window.scale_factor(), shader_radius);

            let overscan = px(GLASS_OVERSCAN);
            let effect_bounds = Bounds {
                origin: point(bounds.origin.x - overscan, bounds.origin.y - overscan),
                size: size(
                    bounds.size.width + overscan * 2.0,
                    bounds.size.height + overscan * 2.0,
                ),
            };
            let mouse = window.mouse_position();
            let width = effect_bounds.size.width.as_f32().max(f32::EPSILON);
            let height = effect_bounds.size.height.as_f32().max(f32::EPSILON);
            let pointer = Point {
                x: (mouse.x.as_f32() - effect_bounds.origin.x.as_f32()) / width,
                y: (mouse.y.as_f32() - effect_bounds.origin.y.as_f32()) / height,
            };

            window.paint_backdrop_effect(
                PaintBackdropEffect::new(effect_bounds, blur_radius, shader.clone())
                    .uniforms(paint_uniforms)
                    .time(animation_time)
                    .pointer(pointer, effect_bounds.contains(&mouse))
                    .opacity(glass_opacity),
            );

            if !backdrop_supported {
                window.paint_quad(quad(
                    bounds,
                    corner_radii,
                    tint.opacity(glass_opacity),
                    Edges::all(px(1.0)),
                    Edges::all(fallback_edge_color.opacity(glass_opacity)),
                    BorderStyle::default(),
                ));
            }
        };

        let move_state = interaction.clone();
        let down_state = interaction.clone();
        let up_state = interaction.clone();
        let up_out_state = interaction;
        let move_style = self.style;
        let down_style = self.style;
        let up_style = self.style;
        let up_out_style = self.style;
        self.div
            .on_paint_before_children(decoration)
            .on_mouse_move(move |event, window, cx| {
                if move_style != GlassStyle::Gel {
                    return;
                }
                move_state.update(cx, |state, cx| {
                    state.record_pointer(event.position, move_style);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if down_style != GlassStyle::Gel {
                    return;
                }
                down_state.update(cx, |state, cx| {
                    state.set_pressed(true, event.position, down_style);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_up(MouseButton::Left, move |event, window, cx| {
                if up_style != GlassStyle::Gel {
                    return;
                }
                up_state.update(cx, |state, cx| {
                    state.set_pressed(false, event.position, up_style);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                if up_out_style != GlassStyle::Gel {
                    return;
                }
                up_out_state.update(cx, |state, cx| {
                    state.set_pressed(false, event.position, up_out_style);
                    cx.notify();
                });
                window.refresh();
            })
    }
}

/// Creates a default [`GlassPanel`].
pub fn glass_panel() -> GlassPanel {
    GlassPanel::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_density_increases_blur() {
        for style in [GlassStyle::Frosted, GlassStyle::Gel] {
            assert!(
                GlassMaterial::Thin.parameters(style).0
                    < GlassMaterial::Regular.parameters(style).0
            );
            assert!(
                GlassMaterial::Regular.parameters(style).0
                    < GlassMaterial::Thick.parameters(style).0
            );
        }
    }

    #[test]
    fn gel_pressure_uses_a_damped_release_spring() {
        let mut state = GlassInteractionState::default();
        state.target_pressure = 1.0;
        for _ in 0..30 {
            state.advance_by(1.0 / 60.0, GlassStyle::Gel);
        }
        assert!(state.pressure > 0.9);

        state.target_pressure = 0.0;
        let mut crossed_rest = false;
        for _ in 0..90 {
            crossed_rest |= state.advance_by(1.0 / 60.0, GlassStyle::Gel).pressure < 0.0;
        }
        assert!(
            crossed_rest,
            "release should overshoot the resting position"
        );
        assert!(state.pressure.abs() < 0.01);
    }

    #[test]
    fn glass_panel_constructors_select_all_styles() {
        assert_eq!(GlassPanel::new().style, GlassStyle::Frosted);
        assert_eq!(GlassPanel::frosted().style, GlassStyle::Frosted);
        assert_eq!(GlassPanel::gel().style, GlassStyle::Gel);
    }

    #[test]
    fn translation_velocity_is_normalized_without_losing_direction() {
        let velocity = normalize_translation_velocity(Point {
            x: GLASS_REFERENCE_VELOCITY,
            y: GLASS_REFERENCE_VELOCITY,
        });
        let expected = std::f32::consts::FRAC_1_SQRT_2;
        assert!((velocity.x - expected).abs() < 0.0001);
        assert!((velocity.y - expected).abs() < 0.0001);
    }

    #[test]
    fn non_gel_styles_ignore_pointer_and_pressure() {
        let position = Point {
            x: px(20.0),
            y: px(20.0),
        };
        let style = GlassStyle::Frosted;
        let mut state = GlassInteractionState::default();
        state.set_pressed(true, position, style);
        state.record_pointer_by(position, 1.0 / 60.0, style);
        state.record_pointer_by(
            Point {
                x: px(80.0),
                y: px(40.0),
            },
            1.0 / 60.0,
            style,
        );
        let frame = state.advance_by(1.0 / 60.0, style);
        assert_eq!(frame.pressure, 0.0);
        assert_eq!(frame.velocity, Point::default());
    }

    #[test]
    fn semantic_controls_clamp_invalid_values() {
        let panel = GlassPanel::new()
            .blur_radius(px(-4.0))
            .deformation(-1.0)
            .wave_strength(-1.0)
            .glass_opacity(2.0);
        assert_eq!(panel.blur_radius, Some(px(0.0)));
        assert_eq!(panel.deformation, 0.0);
        assert_eq!(panel.wave_strength, 0.0);
        assert_eq!(panel.glass_opacity, 1.0);
    }

    #[test]
    fn glass_edge_can_be_hidden() {
        assert!(GlassPanel::new().edge_visible);
        assert!(!GlassPanel::new().edge_visible(false).edge_visible);
    }

    #[test]
    fn component_id_preserves_the_panel_type_and_forwards_to_the_div() {
        let mut panel = GlassPanel::new().id("documented-glass-panel").child(div());
        assert_eq!(
            panel.div.interactivity().element_id.as_ref(),
            Some(&ElementId::from("documented-glass-panel"))
        );
        let _: gpui::ViewElement<GlassPanel> = panel.into_element();
    }

    #[test]
    fn default_container_style_matches_div() {
        let mut panel = GlassPanel::new();
        let mut ordinary_div = div();
        assert_eq!(Styled::style(&mut panel), ordinary_div.style());
    }

    #[test]
    fn glass_pixel_uniforms_follow_device_scale() {
        let uniforms = EffectUniforms::new()
            .with_slot(GLASS_OPTICS_SLOT, [12.0, 2.0, 0.5, 1.0])
            .with_slot(GLASS_GEOMETRY_SLOT, [32.0, 0.0, 20.0, 0.1]);
        let scaled = scale_glass_pixel_uniforms(uniforms, 2.0, px(48.0));

        assert_eq!(scaled.slots()[GLASS_OPTICS_SLOT], [24.0, 4.0, 0.5, 1.0]);
        assert_eq!(scaled.slots()[GLASS_GEOMETRY_SLOT], [64.0, 96.0, 40.0, 0.1]);
    }
}
