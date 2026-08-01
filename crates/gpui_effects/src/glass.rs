use std::time::{Duration, Instant};

use gpui::{
    AbsoluteLength, Animation, AnimationExt, AnyElement, Div, EffectUniforms, ElementId, Hsla,
    InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce, Rgba,
    StyleRefinement, Styled, div, hsla, px,
};

use crate::{
    LIQUID_GLASS_GEOMETRY_SLOT, LIQUID_GLASS_INTERACTION_SLOT, LIQUID_GLASS_LIGHT_SLOT,
    LIQUID_GLASS_OPTICS_SLOT, LIQUID_GLASS_SURFACE_SLOT, LIQUID_GLASS_TINT_SLOT, liquid_glass,
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
    fn set_pressed(&mut self, pressed: bool, position: Point<Pixels>) {
        self.target_pressure = if pressed { 1.0 } else { 0.0 };
        self.last_pointer = Some(position);
        self.last_pointer_at = Instant::now();
    }

    fn record_pointer(&mut self, position: Point<Pixels>) {
        let now = Instant::now();
        if self.target_pressure < 0.5 {
            self.last_pointer = Some(position);
            self.last_pointer_at = now;
            return;
        }
        let elapsed = now
            .saturating_duration_since(self.last_pointer_at)
            .as_secs_f32();
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
        self.last_pointer_at = now;
    }

    fn advance(&mut self) -> GlassInteractionFrame {
        let now = Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_frame_at)
            .as_secs_f32()
            .clamp(0.0, 1.0 / 20.0);
        self.last_frame_at = now;
        self.advance_by(elapsed)
    }

    fn advance_by(&mut self, elapsed: f32) -> GlassInteractionFrame {
        let elapsed = elapsed.clamp(0.0, 1.0 / 20.0);

        // Slightly under-damped spring: one restrained overshoot on release.
        let acceleration =
            (self.target_pressure - self.pressure) * 185.0 - self.pressure_velocity * 21.0;
        self.pressure_velocity += acceleration * elapsed;
        self.pressure += self.pressure_velocity * elapsed;
        self.pressure = self.pressure.clamp(-0.12, 1.08);

        let velocity_decay = (-elapsed * 6.5).exp();
        self.pointer_velocity.x *= velocity_decay;
        self.pointer_velocity.y *= velocity_decay;
        GlassInteractionFrame {
            pressure: self.pressure,
            velocity: self.pointer_velocity,
        }
    }
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
    fn parameters(self) -> (Pixels, [f32; 4], [f32; 4]) {
        match self {
            Self::Thin => (px(10.0), [10.0, 1.0, 0.62, 1.08], [0.13, 0.34, 0.64, 0.72]),
            Self::Regular => (px(14.0), [15.0, 1.8, 0.56, 1.12], [0.18, 0.52, 0.76, 1.0]),
            Self::Thick => (px(22.0), [19.0, 2.4, 0.42, 1.14], [0.22, 0.62, 0.82, 0.86]),
        }
    }
}

/// A container backed by the dynamic liquid-glass backdrop effect.
///
/// Content is painted after the material, while the effect itself samples only
/// scene content already behind the panel. On renderers without backdrop
/// support, the translucent tint and border remain visible as a fallback.
#[derive(IntoElement)]
pub struct GlassPanel {
    div: Div,
    children: Vec<AnyElement>,
    material: GlassMaterial,
    radius: AbsoluteLength,
    tint: Hsla,
    border_color: Hsla,
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
    /// Creates a regular, animated glass panel.
    pub fn new() -> Self {
        Self {
            div: div().relative().rounded(px(16.0)),
            children: Vec::new(),
            material: GlassMaterial::Regular,
            radius: px(16.0).into(),
            tint: hsla(0.58, 0.55, 0.96, 0.07),
            border_color: hsla(0.0, 0.0, 1.0, 0.24),
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

    /// Selects an optical density preset.
    pub fn material(mut self, material: GlassMaterial) -> Self {
        self.material = material;
        self
    }

    /// Sets the panel and material corner radius.
    pub fn radius(mut self, radius: impl Into<AbsoluteLength>) -> Self {
        let radius = radius.into();
        self.radius = radius;
        self.div = self.div.rounded(radius);
        self
    }

    /// Sets the conventional translucent overlay painted over the material.
    pub fn tint(mut self, tint: impl Into<Hsla>) -> Self {
        self.tint = tint.into();
        self
    }

    /// Sets the bright glass edge color.
    pub fn border_color(mut self, color: impl Into<Hsla>) -> Self {
        self.border_color = color.into();
        self
    }

    /// Enables or disables time- and pointer-driven surface animation.
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    /// Sets the animation identity and duration.
    pub fn animation(mut self, id: impl Into<ElementId>, duration: Duration) -> Self {
        self.animation_id = id.into();
        self.animation_duration = duration;
        self
    }

    /// Overrides refraction, dispersion, raw-detail mix, and saturation.
    pub fn optics(mut self, value: [f32; 4]) -> Self {
        self.optics = Some(value);
        self
    }

    /// Overrides edge width, highlight, pointer response, and motion.
    pub fn surface(mut self, value: [f32; 4]) -> Self {
        self.surface = Some(value);
        self
    }

    /// Overrides shader tint RGB and shader tint strength.
    pub fn shader_tint(mut self, value: [f32; 4]) -> Self {
        self.shader_tint = Some(value);
        self
    }

    /// Scales lens, edge, and chromatic deformation. Zero keeps the backdrop undistorted.
    pub fn deformation(mut self, strength: f32) -> Self {
        self.deformation = strength.max(0.0);
        self
    }

    /// Scales the ambient animated surface flow. Zero disables the default wave.
    pub fn wave_strength(mut self, strength: f32) -> Self {
        self.wave_strength = strength.max(0.0);
        self
    }

    /// Sets the material opacity without changing its tint color.
    pub fn glass_opacity(mut self, opacity: f32) -> Self {
        self.glass_opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Supplies the panel's translation velocity in logical pixels per second.
    ///
    /// Pointer motion describes interaction across the surface, while this
    /// value describes movement of the glass body itself. Draggable or
    /// spring-animated callers should update it every frame so the leading
    /// edge stretches and the trailing edge lags behind.
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

impl ParentElement for GlassPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for GlassPanel {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let (blur_radius, preset_optics, preset_surface) = self.material.parameters();
        let should_animate = self.animated && window.supports_backdrop_blur();
        let interaction = window.use_keyed_state(self.animation_id.clone(), cx, |_, _| {
            GlassInteractionState::default()
        });
        let interaction_frame = interaction.update(cx, |state, _| state.advance());
        let translation_velocity = normalize_translation_velocity(self.translation_velocity);
        let (material_velocity, material_speed) = clamp_unit_velocity(Point {
            x: interaction_frame.velocity.x + translation_velocity.x,
            y: interaction_frame.velocity.y + translation_velocity.y,
        });
        let tint: Rgba = self.tint.into();
        let edge_light: Rgba = self.border_color.into();
        let mut optics = self.optics.unwrap_or(preset_optics);
        optics[0] *= self.deformation;
        optics[1] *= self.deformation;
        let mut surface = self.surface.unwrap_or(preset_surface);
        surface[3] *= self.wave_strength;
        let mut uniforms = EffectUniforms::new()
            .with_slot(LIQUID_GLASS_OPTICS_SLOT, optics)
            .with_slot(LIQUID_GLASS_SURFACE_SLOT, surface)
            .with_slot(
                LIQUID_GLASS_TINT_SLOT,
                self.shader_tint.unwrap_or([tint.r, tint.g, tint.b, tint.a]),
            )
            .with_slot(
                LIQUID_GLASS_GEOMETRY_SLOT,
                [
                    GLASS_OVERSCAN,
                    self.radius.to_pixels(window.rem_size()).as_f32(),
                    18.0 + blur_radius.as_f32() * 0.22,
                    0.092 * self.deformation,
                ],
            )
            .with_slot(
                LIQUID_GLASS_INTERACTION_SLOT,
                [
                    interaction_frame.pressure,
                    material_velocity.x,
                    material_velocity.y,
                    material_speed,
                ],
            )
            .with_slot(
                LIQUID_GLASS_LIGHT_SLOT,
                [edge_light.r, edge_light.g, edge_light.b, 0.78],
            );
        if !should_animate {
            let mut surface = uniforms.slots()[LIQUID_GLASS_SURFACE_SLOT];
            surface[2] = 0.0;
            surface[3] = 0.0;
            uniforms.set_slot(LIQUID_GLASS_SURFACE_SLOT, surface);
        }

        let overlay = liquid_glass()
            .uniforms(uniforms)
            .blur_radius(blur_radius)
            .effect_opacity(self.glass_opacity)
            .absolute()
            .left(px(-GLASS_OVERSCAN))
            .right(px(-GLASS_OVERSCAN))
            .top(px(-GLASS_OVERSCAN))
            .bottom(px(-GLASS_OVERSCAN));
        let overlay = if should_animate {
            overlay
                .with_animation(
                    self.animation_id,
                    Animation::new(self.animation_duration).repeat(),
                    |effect, time| effect.time(time),
                )
                .into_any_element()
        } else {
            overlay.into_any_element()
        };

        let fallback_alpha = if window.supports_backdrop_blur() {
            0.0
        } else {
            self.glass_opacity
        };
        let fallback = div()
            .absolute()
            .inset_0()
            .rounded(self.radius)
            .bg(self.tint.opacity(fallback_alpha))
            .border_1()
            .border_color(self.border_color.opacity(fallback_alpha));

        let move_state = interaction.clone();
        let down_state = interaction.clone();
        let up_state = interaction.clone();
        let up_out_state = interaction;
        self.div
            .on_mouse_move(move |event, window, cx| {
                move_state.update(cx, |state, cx| {
                    state.record_pointer(event.position);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                down_state.update(cx, |state, cx| {
                    state.set_pressed(true, event.position);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_up(MouseButton::Left, move |event, window, cx| {
                up_state.update(cx, |state, cx| {
                    state.set_pressed(false, event.position);
                    cx.notify();
                });
                window.refresh();
            })
            .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                up_out_state.update(cx, |state, cx| {
                    state.set_pressed(false, event.position);
                    cx.notify();
                });
                window.refresh();
            })
            .child(overlay)
            .child(fallback)
            .children(self.children)
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
        assert!(GlassMaterial::Thin.parameters().0 < GlassMaterial::Regular.parameters().0);
        assert!(GlassMaterial::Regular.parameters().0 < GlassMaterial::Thick.parameters().0);
    }

    #[test]
    fn pressure_uses_a_damped_release_spring() {
        let mut state = GlassInteractionState::default();
        state.target_pressure = 1.0;
        for _ in 0..30 {
            state.advance_by(1.0 / 60.0);
        }
        assert!(state.pressure > 0.9);

        state.target_pressure = 0.0;
        let mut crossed_rest = false;
        for _ in 0..90 {
            crossed_rest |= state.advance_by(1.0 / 60.0).pressure < 0.0;
        }
        assert!(
            crossed_rest,
            "release should overshoot the resting position"
        );
        assert!(state.pressure.abs() < 0.01);
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
    fn hover_motion_does_not_drive_the_surface() {
        let mut state = GlassInteractionState::default();
        state.record_pointer(Point {
            x: px(10.0),
            y: px(10.0),
        });
        state.record_pointer(Point {
            x: px(80.0),
            y: px(40.0),
        });
        assert_eq!(state.pointer_velocity, Point::default());
    }

    #[test]
    fn semantic_controls_clamp_invalid_values() {
        let panel = GlassPanel::new()
            .deformation(-1.0)
            .wave_strength(-1.0)
            .glass_opacity(2.0);
        assert_eq!(panel.deformation, 0.0);
        assert_eq!(panel.wave_strength, 0.0);
        assert_eq!(panel.glass_opacity, 1.0);
    }
}
