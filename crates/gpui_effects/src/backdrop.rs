use gpui::prelude::*;
use gpui::{
    App, BackdropShader, Bounds, EffectUniforms, Element, ElementId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, PaintBackdropEffect, Pixels, Point, Style,
    StyleRefinement, Styled, Window,
};

/// Uniform slot containing refraction, dispersion, raw-detail mix, and saturation.
pub const GLASS_OPTICS_SLOT: usize = 0;
/// Uniform slot containing edge width, highlight, pointer response, and a style parameter.
pub const GLASS_SURFACE_SLOT: usize = 1;
/// Uniform slot containing tint RGB and tint strength.
pub const GLASS_TINT_SLOT: usize = 2;
/// Uniform slot containing overscan, corner radius, optical thickness, and magnification.
pub const GLASS_GEOMETRY_SLOT: usize = 3;
/// Uniform slot containing pressure, pointer velocity, and normalized speed.
pub const GLASS_INTERACTION_SLOT: usize = 4;
/// Uniform slot containing the edge-light RGB color and intensity.
pub const GLASS_LIGHT_SLOT: usize = 5;

/// Constructs a styled element that composites a custom shader over scene
/// content already painted behind it.
pub fn backdrop_effect(shader: BackdropShader) -> BackdropEffect {
    BackdropEffect::new(shader)
}

/// Constructs a stable frosted-glass material with diffuse backdrop scattering.
pub fn frosted_glass() -> BackdropEffect {
    backdrop_effect(frosted_glass_shader())
        .blur_radius(gpui::px(14.0))
        .uniform(GLASS_OPTICS_SLOT, [0.0, 0.0, 0.18, 1.02])
        .uniform(GLASS_SURFACE_SLOT, [0.16, 0.42, 0.0, 0.0])
        .uniform(GLASS_TINT_SLOT, [0.88, 0.94, 1.0, 0.10])
        .uniform(GLASS_GEOMETRY_SLOT, [0.0, 16.0, 18.0, 0.0])
        .uniform(GLASS_INTERACTION_SLOT, [0.0; 4])
        .uniform(GLASS_LIGHT_SLOT, [1.0, 1.0, 1.0, 0.58])
}

/// Constructs the elastic gel-glass material retained for playful surfaces.
pub fn gel_glass() -> BackdropEffect {
    backdrop_effect(gel_glass_shader())
        .blur_radius(gpui::px(14.0))
        .uniform(GLASS_OPTICS_SLOT, [15.0, 1.8, 0.56, 1.12])
        .uniform(GLASS_SURFACE_SLOT, [0.18, 0.52, 0.76, 1.0])
        .uniform(GLASS_TINT_SLOT, [0.88, 0.94, 1.0, 0.055])
        .uniform(GLASS_GEOMETRY_SLOT, [0.0, 16.0, 18.0, 0.092])
        .uniform(GLASS_INTERACTION_SLOT, [0.0; 4])
        .uniform(GLASS_LIGHT_SLOT, [1.0, 1.0, 1.0, 0.72])
}

/// Constructs a clear liquid-glass material with rounded lens refraction.
pub fn liquid_glass() -> BackdropEffect {
    backdrop_effect(liquid_glass_shader())
        .blur_radius(gpui::px(2.0))
        .uniform(GLASS_OPTICS_SLOT, [18.0, 0.55, 0.94, 1.10])
        .uniform(GLASS_SURFACE_SLOT, [0.16, 0.34, 0.0, 1.20])
        .uniform(GLASS_TINT_SLOT, [0.82, 0.93, 1.0, 0.055])
        .uniform(GLASS_GEOMETRY_SLOT, [0.0, 16.0, 18.0, 0.092])
        .uniform(GLASS_INTERACTION_SLOT, [0.0; 4])
        .uniform(GLASS_LIGHT_SLOT, [0.88, 0.96, 1.0, 0.72])
}

/// Returns the portable shader used by [`frosted_glass`].
pub fn frosted_glass_shader() -> BackdropShader {
    BackdropShader::wgsl(include_str!("shaders/frosted_glass.wgsl"))
}

/// Returns the portable shader used by [`gel_glass`].
pub fn gel_glass_shader() -> BackdropShader {
    BackdropShader::wgsl(include_str!("shaders/gel_glass.wgsl"))
}

/// Returns the portable shader used by [`liquid_glass`].
pub fn liquid_glass_shader() -> BackdropShader {
    BackdropShader::wgsl(include_str!("shaders/liquid_glass.wgsl"))
}

/// A styled leaf element that samples both raw and blurred scene content.
///
/// The renderer prepares the backdrop inputs; the shader handles only material
/// compositing. A normal style background and border are painted afterwards,
/// so they can provide a tint and a graceful fallback on unsupported backends.
pub struct BackdropEffect {
    shader: BackdropShader,
    uniforms: EffectUniforms,
    time: f32,
    blur_radius: Pixels,
    opacity: f32,
    style: StyleRefinement,
}

impl BackdropEffect {
    /// Creates a backdrop element from a portable shader.
    pub fn new(shader: BackdropShader) -> Self {
        Self {
            shader,
            uniforms: EffectUniforms::default(),
            time: 0.0,
            blur_radius: gpui::px(0.0),
            opacity: 1.0,
            style: StyleRefinement::default(),
        }
    }

    /// Replaces all user-defined uniform slots.
    pub fn uniforms(mut self, uniforms: EffectUniforms) -> Self {
        self.uniforms = uniforms;
        self
    }

    /// Sets one four-component user uniform slot.
    pub fn uniform(mut self, index: usize, value: [f32; 4]) -> Self {
        self.uniforms.set_slot(index, value);
        self
    }

    /// Sets the radius used to prepare the blurred backdrop input.
    pub fn blur_radius(mut self, radius: Pixels) -> Self {
        self.blur_radius = radius;
        self
    }

    /// Sets normalized animation time supplied to the shader.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }

    /// Sets opacity applied after shader evaluation.
    pub fn effect_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Returns the shader backing this element.
    pub fn shader(&self) -> &BackdropShader {
        &self.shader
    }
}

impl IntoElement for BackdropEffect {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BackdropEffect {
    type RequestLayoutState = Style;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.refine(&self.style);
        let layout_id = window.request_layout(style.clone(), [], cx);
        (layout_id, style)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Style,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        style: &mut Style,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let corner_radii = style
            .corner_radii
            .to_pixels(window.rem_size())
            .clamp_radii_for_quad_size(bounds.size);
        let mouse = window.mouse_position();
        let pointer_active = bounds.contains(&mouse);
        let width = bounds.size.width.as_f32().max(f32::EPSILON);
        let height = bounds.size.height.as_f32().max(f32::EPSILON);
        let pointer = Point {
            x: (mouse.x.as_f32() - bounds.origin.x.as_f32()) / width,
            y: (mouse.y.as_f32() - bounds.origin.y.as_f32()) / height,
        };

        let effect = PaintBackdropEffect::new(bounds, self.blur_radius, self.shader.clone())
            .uniforms(self.uniforms)
            .time(self.time)
            .pointer(pointer, pointer_active)
            .corner_radii(corner_radii)
            .opacity(self.opacity);
        window.paint_backdrop_effect(effect);

        // Paint conventional tint, shadow, and border over the material. This
        // also leaves a useful translucent panel when backdrop shaders are not
        // supported by the active renderer.
        style.paint(bounds, window, cx, |_window, _cx| {});
    }
}

impl Styled for BackdropEffect {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optical_shader_identities_are_stable_and_distinct() {
        assert_eq!(frosted_glass_shader().id(), frosted_glass_shader().id());
        assert_eq!(gel_glass_shader().id(), gel_glass_shader().id());
        assert_eq!(liquid_glass_shader().id(), liquid_glass_shader().id());
        assert_ne!(frosted_glass_shader().id(), gel_glass_shader().id());
        assert_ne!(frosted_glass_shader().id(), liquid_glass_shader().id());
        assert_ne!(gel_glass_shader().id(), liquid_glass_shader().id());
    }
}
