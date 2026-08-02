use crate::{Effect, effect, image_effect};
use gpui::prelude::*;
use gpui::{EffectShader, ImageSource, Rgba};

fn color_slot(color: impl Into<Rgba>) -> [f32; 4] {
    let color = color.into();
    [color.r, color.g, color.b, color.a]
}

/// Returns an animated aurora-style effect with four customizable colors.
pub fn aurora<C>(colors: [C; 4]) -> Effect
where
    C: Copy + Into<Rgba>,
{
    effect(aurora_shader())
        .uniform(0, color_slot(colors[0]))
        .uniform(1, color_slot(colors[1]))
        .uniform(2, color_slot(colors[2]))
        .uniform(3, color_slot(colors[3]))
        .uniform(4, [1.15, 0.8, 1.0, 0.0])
}

/// Returns the portable shader used by [`aurora`].
pub fn aurora_shader() -> EffectShader {
    EffectShader::wgsl(AURORA_WGSL)
}

/// Returns a smoothly looping plasma effect with four customizable colors.
pub fn plasma<C>(colors: [C; 4]) -> Effect
where
    C: Copy + Into<Rgba>,
{
    effect(plasma_shader())
        .uniform(0, color_slot(colors[0]))
        .uniform(1, color_slot(colors[1]))
        .uniform(2, color_slot(colors[2]))
        .uniform(3, color_slot(colors[3]))
        .uniform(4, [1.0, 1.0, 1.0, 0.0])
}

/// Returns the portable shader used by [`plasma`].
pub fn plasma_shader() -> EffectShader {
    EffectShader::wgsl(PLASMA_WGSL)
}

/// Returns a smoothly looping color-orb fusion effect.
pub fn color_orbs<C>(colors: [C; 4]) -> Effect
where
    C: Copy + Into<Rgba>,
{
    effect(color_orbs_shader())
        .uniform(0, color_slot(colors[0]))
        .uniform(1, color_slot(colors[1]))
        .uniform(2, color_slot(colors[2]))
        .uniform(3, color_slot(colors[3]))
        .uniform(4, [4.5, 1.0, 1.0, 0.0])
}

/// Returns the portable shader used by [`color_orbs`].
pub fn color_orbs_shader() -> EffectShader {
    EffectShader::wgsl(COLOR_ORBS_WGSL)
}

/// Returns a blurred, slowly moving atmosphere derived from album artwork.
///
/// Uniform slot 0 contains diffusion, saturation, brightness, and motion
/// strength. Slot 1 contains glow strength, ripple displacement, ripple light,
/// and ring definition. Callers may override either slot with
/// [`Effect::uniform`].
///
/// Pass monotonically increasing elapsed seconds to [`Effect::time`]. Scaling
/// that value changes the flow speed without introducing a repeating cycle.
pub fn album_glow(source: impl Into<ImageSource>) -> Effect {
    image_effect(source, album_glow_shader())
        .uniform(0, [0.16, 1.72, 0.94, 0.28])
        .uniform(1, [0.30, 0.0, 0.0, 0.0])
        .bg(gpui::rgb(0x2a2834))
}

/// Returns a defined water-ripple treatment derived from album artwork.
///
/// Unlike [`album_glow`], this preset deliberately exposes the bright and dark
/// edges of each expanding ring.
pub fn album_ripples(source: impl Into<ImageSource>) -> Effect {
    image_effect(source, album_ripples_shader())
        .uniform(0, [0.14, 1.65, 0.9, 1.0])
        .uniform(1, [0.48, 0.075, 0.28, 1.0])
        .bg(gpui::rgb(0x2a2834))
}

/// Returns the image-sampling shader used by [`album_glow`] and [`album_ripples`].
pub fn album_glow_shader() -> EffectShader {
    EffectShader::wgsl_image(ALBUM_GLOW_WGSL)
}

/// Returns the image-sampling shader used by [`album_ripples`].
pub fn album_ripples_shader() -> EffectShader {
    album_glow_shader()
}

const AURORA_WGSL: &str = r#"
fn aurora_band(distance: f32, width: f32) -> f32 {
    let normalized = distance / max(width, 0.0001);
    return exp(-normalized * normalized);
}

fn aurora_palette(position: f32, params: EffectParams) -> vec4<f32> {
    let cursor = fract(position) * 4.0;
    let distance0 = min(abs(cursor), 4.0 - abs(cursor));
    let distance1 = min(abs(cursor - 1.0), 4.0 - abs(cursor - 1.0));
    let distance2 = min(abs(cursor - 2.0), 4.0 - abs(cursor - 2.0));
    let distance3 = min(abs(cursor - 3.0), 4.0 - abs(cursor - 3.0));
    let weights = vec4<f32>(
        1.0 - smoothstep(0.0, 1.0, distance0),
        1.0 - smoothstep(0.0, 1.0, distance1),
        1.0 - smoothstep(0.0, 1.0, distance2),
        1.0 - smoothstep(0.0, 1.0, distance3),
    );
    let total = max(dot(weights, vec4<f32>(1.0)), 0.0001);
    return (
        params.slots[0] * weights.x +
        params.slots[1] * weights.y +
        params.slots[2] * weights.z +
        params.slots[3] * weights.w
    ) / total;
}

fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let tau = 6.28318530718;
    let phase = input.time * tau;
    let scale = params.slots[4].x;
    let x = input.uv.x * scale;
    let y = input.uv.y;

    // Three independent curtains avoid the horizontal layers produced by a
    // conventional stacked gradient. Every displacement is analytic, so the
    // motion remains continuous without a noise grid becoming visible.
    let center0 = 0.42
        + sin(x * tau * 0.72 + phase) * 0.13
        + sin(x * tau * 1.85 - phase * 1.7) * 0.035;
    let center1 = 0.55
        + sin(x * tau * 0.58 - phase + 1.7) * 0.16
        + cos(x * tau * 2.2 + phase * 2.0) * 0.03;
    let center2 = 0.67
        + sin(x * tau * 0.91 + phase + 3.4) * 0.12
        + sin(x * tau * 2.65 - phase * 2.0) * 0.025;

    let folds0 = 0.72 + 0.28 * pow(0.5 + 0.5 * sin(x * tau * 8.0 + phase * 2.0), 2.0);
    let folds1 = 0.72 + 0.28 * pow(0.5 + 0.5 * sin(x * tau * 10.0 - phase), 2.0);
    let folds2 = 0.72 + 0.28 * pow(0.5 + 0.5 * cos(x * tau * 7.0 + phase), 2.0);
    let band0 = aurora_band(y - center0, 0.19) * folds0;
    let band1 = aurora_band(y - center1, 0.22) * folds1;
    let band2 = aurora_band(y - center2, 0.24) * folds2;

    // A wider, dimmer halo turns the ribbons into soft light curtains rather
    // than bright contour lines.
    let halo0 = aurora_band(y - center0, 0.38) * 0.22;
    let halo1 = aurora_band(y - center1, 0.43) * 0.19;
    let halo2 = aurora_band(y - center2, 0.46) * 0.17;
    let intensity0 = band0 + halo0;
    let intensity1 = band1 + halo1;
    let intensity2 = band2 + halo2;

    let color0 = aurora_palette(x * 0.23 + sin(phase) * 0.18, params);
    let color1 = aurora_palette(x * 0.19 + sin(-phase + 1.1) * 0.14 + 0.31, params);
    let color2 = aurora_palette(x * 0.16 + sin(phase + 0.7) * 0.11 + 0.62, params);
    let total = max(intensity0 + intensity1 + intensity2, 0.0001);
    let curtain = (
        color0 * intensity0 +
        color1 * intensity1 +
        color2 * intensity2
    ) / total;

    let background_color = aurora_palette(input.uv.x * 0.32 + sin(phase) * 0.06, params);
    let background = background_color.rgb * mix(0.16, 0.27, y);
    let strength = 1.0 - exp(-total * 0.92);
    let glow = 0.9 + 0.18 * clamp(total / 2.4, 0.0, 1.0);
    let rgb = pow(
        max(mix(background, curtain.rgb * glow, strength), vec3<f32>(0.0)),
        vec3<f32>(0.88),
    );
    let alpha = dot(vec4<f32>(
        params.slots[0].a,
        params.slots[1].a,
        params.slots[2].a,
        params.slots[3].a,
    ), vec4<f32>(0.25));
    return vec4<f32>(rgb, alpha);
}
"#;

const PLASMA_WGSL: &str = r#"
fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let tau = 6.28318530718;
    let phase = input.time * tau;
    let point = (input.uv - vec2<f32>(0.5)) * params.slots[4].x;
    let a = sin((point.x * 5.0 + phase) + sin(point.y * 4.0 - phase));
    let b = sin((point.y * 6.0 - phase) + cos(point.x * 3.0 + phase));
    let c = sin(length(point + vec2<f32>(cos(phase), sin(phase)) * 0.35) * 9.0 - phase);
    let first = mix(params.slots[0], params.slots[1], 0.5 + 0.5 * a);
    let second = mix(params.slots[2], params.slots[3], 0.5 + 0.5 * b);
    return mix(first, second, 0.5 + 0.5 * c);
}
"#;

const COLOR_ORBS_WGSL: &str = r#"
fn effect_orb_weight(point: vec2<f32>, center: vec2<f32>, sharpness: f32) -> f32 {
    let delta = point - center;
    return exp(-dot(delta, delta) * sharpness);
}

fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let tau = 6.28318530718;
    let phase = input.time * tau;
    let aspect = input.size.x / max(input.size.y, 1.0);
    let point = (input.uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
    let radius = vec2<f32>(0.32 * aspect, 0.3);
    let center0 = vec2<f32>(cos(phase), sin(phase)) * radius;
    let center1 = vec2<f32>(cos(phase + 1.5707963), sin(phase * 2.0 + 1.2)) * radius;
    let center2 = vec2<f32>(cos(-phase + 3.0), sin(phase + 2.8)) * radius;
    let center3 = vec2<f32>(cos(phase * 2.0 + 4.5), sin(-phase + 4.0)) * radius;
    let sharpness = params.slots[4].x;
    let weights = vec4<f32>(
        effect_orb_weight(point, center0, sharpness),
        effect_orb_weight(point, center1, sharpness),
        effect_orb_weight(point, center2, sharpness),
        effect_orb_weight(point, center3, sharpness),
    );
    let total = max(dot(weights, vec4<f32>(1.0)), 0.0001);
    let color = (
        params.slots[0] * weights.x +
        params.slots[1] * weights.y +
        params.slots[2] * weights.z +
        params.slots[3] * weights.w
    ) / total;
    let glow = clamp(total * 0.42, 0.0, 1.0);
    let rgb = color.rgb * (0.72 + glow * 0.45);
    return vec4<f32>(rgb, clamp(color.a, 0.0, 1.0));
}
"#;

const ALBUM_GLOW_WGSL: &str = r#"
fn album_glow_weight(point: vec2<f32>, center: vec2<f32>, softness: f32) -> f32 {
    let delta = point - center;
    return exp(-dot(delta, delta) * softness);
}

fn album_glow_color(input: EffectInput, uv: vec2<f32>) -> vec3<f32> {
    return sample_effect_image(input, clamp(uv, vec2<f32>(0.03), vec2<f32>(0.97))).rgb;
}

fn album_glow_color_priority(color: vec3<f32>) -> f32 {
    let highest = max(color.r, max(color.g, color.b));
    let lowest = min(color.r, min(color.g, color.b));
    let chroma = highest - lowest;
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let relative_saturation = chroma / max(highest, 0.001);
    let colorfulness = smoothstep(0.025, 0.24, chroma);
    let visible_midtone = smoothstep(0.025, 0.22, luminance)
        * (1.0 - smoothstep(0.86, 1.0, luminance));
    // A tiny neutral floor keeps monochrome artwork usable, while saturated
    // samples win decisively over white backgrounds and large gray subjects.
    return 0.035
        + colorfulness * (0.55 + relative_saturation * 1.85)
        + visible_midtone * 0.10;
}

fn album_glow_hash(seed: f32) -> f32 {
    return fract(sin(seed * 91.173 + 17.17) * 43758.5453);
}

fn album_glow_flow_life(progress: f32, seed: f32) -> f32 {
    let fade_in = mix(0.07, 0.17, album_glow_hash(seed + 1.0));
    let fade_out = mix(0.72, 0.89, album_glow_hash(seed + 2.0));
    return smoothstep(0.0, fade_in, progress)
        * (1.0 - smoothstep(fade_out, 1.0, progress));
}

fn album_glow_flow_center(
    progress: f32,
    seed: f32,
    motion: f32,
) -> vec2<f32> {
    let angle = album_glow_hash(seed + 0.17) * 6.28318530718;
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let bend_frequency = mix(0.65, 1.55, album_glow_hash(seed + 3.0));
    let bend = sin(progress * 3.14159265359)
        * (
            sin(progress * 6.28318530718 * bend_frequency + seed * 2.7) * 0.28
            + (album_glow_hash(seed + 4.0) - 0.5) * 0.42
        )
        * (0.55 + motion);
    let curved_angle = angle + bend;
    let direction = vec2<f32>(cos(curved_angle), sin(curved_angle));
    let tangent = vec2<f32>(-direction.y, direction.x);
    let max_radius = mix(0.62, 0.92, album_glow_hash(seed + 5.0));
    let radial_wobble = sin(progress * 3.14159265359)
        * sin(progress * 6.28318530718 * 1.37 + seed)
        * mix(0.025, 0.075, album_glow_hash(seed + 6.0));
    let radius = mix(0.018, max_radius, eased) + radial_wobble;
    let curve = sin(progress * 3.14159265359)
        * sin(progress * 6.28318530718 * bend_frequency + seed * 1.9)
        * mix(0.035, 0.13, album_glow_hash(seed + 7.0))
        * (0.5 + motion);
    return vec2<f32>(0.5) + direction * radius + tangent * curve;
}

fn album_glow_flow_warp(uv: vec2<f32>, time: f32, motion: f32) -> vec2<f32> {
    let point = uv - vec2<f32>(0.5);
    let flow_x = sin(point.y * 5.17 + time * 0.83
        + sin(point.x * 3.11 - time * 0.37) * 0.72);
    let flow_y = cos(point.x * 4.63 - time * 0.61
        + sin(point.y * 3.73 + time * 0.29) * 0.68);
    let secondary = vec2<f32>(
        sin((point.x - point.y) * 3.31 + time * 0.41),
        cos((point.x + point.y) * 2.87 - time * 0.53),
    );
    let strength = 0.035 + motion * 0.035;
    return uv + (vec2<f32>(flow_x, flow_y) + secondary * 0.45) * strength;
}

fn album_glow_stream_weight(
    point: vec2<f32>,
    center: vec2<f32>,
    softness: f32,
    progress: f32,
    seed: f32,
) -> f32 {
    let direction = normalize(center - vec2<f32>(0.5) + vec2<f32>(0.0001));
    let tangent = vec2<f32>(-direction.y, direction.x);
    let delta = point - center;
    let along = dot(delta, direction);
    let across = dot(delta, tangent);
    // Keep a long, soft trail toward the center, but give the leading edge a
    // tighter falloff. A broad transport layer keeps neighboring colors in
    // contact; a faint, narrower core only makes the flow direction readable.
    let leading_edge = smoothstep(-0.035, 0.085, along);
    let broad_along_softness = mix(softness * 0.30, softness * 3.1, leading_edge);
    let core_along_softness = mix(softness * 0.52, softness * 7.0, leading_edge);
    let width_phase = along * mix(11.0, 19.0, album_glow_hash(seed + 10.0)) + seed * 1.7;
    let width_variation = mix(0.84, 1.18, 0.5 + 0.5 * sin(width_phase));
    let broad_cross_softness = softness * mix(5.8, 2.4, progress) * width_variation;
    let core_cross_softness = softness * mix(12.0, 5.2, progress) * width_variation;

    // Two incommensurate bends prevent a plume from reading as a straight
    // gradient band. Their amplitude grows away from birth and fades before
    // the source is replaced, so there is no visible reset seam.
    let bend_phase = progress * 6.28318530718 + seed;
    let primary_bend = sin(
        along * mix(7.0, 13.0, album_glow_hash(seed + 11.0)) + bend_phase,
    );
    let secondary_bend = sin(
        along * mix(15.0, 24.0, album_glow_hash(seed + 12.0))
            - bend_phase * 0.63
            + seed * 2.3,
    );
    let bend_amount = mix(0.045, 0.105, album_glow_hash(seed + 13.0))
        * sin(progress * 3.14159265359);
    let warped_across = across + (primary_bend + secondary_bend * 0.38) * bend_amount;
    let broad_distance = along * along * broad_along_softness
        + warped_across * warped_across * broad_cross_softness;
    let core_distance = along * along * core_along_softness
        + warped_across * warped_across * core_cross_softness;
    return exp(-broad_distance) * 0.78 + exp(-core_distance) * 0.22;
}

fn album_drop_wave(
    point: vec2<f32>,
    origin: vec2<f32>,
    aspect: f32,
    progress: f32,
    max_radius: f32,
    width: f32,
) -> vec4<f32> {
    let delta = point - origin;
    let metric_delta = delta * vec2<f32>(aspect, 1.0);
    let distance = max(length(metric_delta), 0.0001);
    let radius = progress * max_radius;
    let signed_distance = (distance - radius) / max(width, 0.0001);
    let life = sin(progress * 3.14159265359);
    let crest = exp(-signed_distance * signed_distance) * life;
    let trough_distance = signed_distance + 1.25;
    let trough = exp(-trough_distance * trough_distance * 1.25) * life;
    let displacement = signed_distance * crest;
    let direction = metric_delta / distance / vec2<f32>(aspect, 1.0);
    let halo = exp(-signed_distance * signed_distance * 0.28) * life;
    return vec4<f32>(direction * displacement, crest - trough * 0.42, halo);
}

fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let tau = 6.28318530718;
    let blur = max(params.slots[0].x, 0.0);
    let saturation = max(params.slots[0].y, 0.0);
    let brightness = max(params.slots[0].z, 0.0);
    let motion = max(params.slots[0].w, 0.0);
    let glow_strength = max(params.slots[1].x, 0.0);
    let ripple_displacement = max(params.slots[1].y, 0.0);
    let ripple_light = max(params.slots[1].z, 0.0);
    let ring_definition = clamp(params.slots[1].w, 0.0, 1.0);
    let diffusion = clamp(blur / 0.18, 0.0, 1.0);
    // Album glow consumes continuously increasing seconds and advances one
    // source lifetime roughly every 56 seconds. Album ripples retains the
    // normalized periodic clock used by its repeating preview.
    let glow_time = input.time * 0.018;
    let animation_time = mix(glow_time, input.time, ring_definition);
    let phase = animation_time * tau;

    // Pull a compact palette from the artwork's inner area. Avoiding the outer
    // margin prevents a light frame or letterboxed cover from becoming the
    // dominant background color.
    let palette_drift = vec2<f32>(cos(phase), sin(phase)) * 0.025 * motion;
    let color0 = album_glow_color(input, vec2<f32>(0.28, 0.34) + palette_drift);
    let color1 = album_glow_color(
        input,
        vec2<f32>(0.72, 0.36) + vec2<f32>(-palette_drift.y, palette_drift.x),
    );
    let color2 = album_glow_color(input, vec2<f32>(0.16, 0.58) - palette_drift);
    let color3 = album_glow_color(
        input,
        vec2<f32>(0.84, 0.58) + vec2<f32>(palette_drift.y, -palette_drift.x),
    );
    let color4 = album_glow_color(input, vec2<f32>(0.32, 0.78) + palette_drift.yx * 0.6);
    let color5 = album_glow_color(input, vec2<f32>(0.68, 0.80) - palette_drift.yx * 0.7);
    let priority0 = album_glow_color_priority(color0);
    let priority1 = album_glow_color_priority(color1);
    let priority2 = album_glow_color_priority(color2);
    let priority3 = album_glow_color_priority(color3);
    let priority4 = album_glow_color_priority(color4);
    let priority5 = album_glow_color_priority(color5);
    let palette_weight0 = priority0 * priority0 * priority0;
    let palette_weight1 = priority1 * priority1 * priority1;
    let palette_weight2 = priority2 * priority2 * priority2;
    let palette_weight3 = priority3 * priority3 * priority3;
    let palette_weight4 = priority4 * priority4 * priority4;
    let palette_weight5 = priority5 * priority5 * priority5;
    let palette_total = max(
        palette_weight0
            + palette_weight1
            + palette_weight2
            + palette_weight3
            + palette_weight4
            + palette_weight5,
        0.0001,
    );
    let palette_color = (
        color0 * palette_weight0
        + color1 * palette_weight1
        + color2 * palette_weight2
        + color3 * palette_weight3
        + color4 * palette_weight4
        + color5 * palette_weight5
    ) / palette_total;

    // Three staggered drops produce one local wavefront each. Distances are
    // measured in element-height units, preserving a circular wave on a wide card.
    let aspect = input.size.x / max(input.size.y, 1.0);
    let ripple_origin0 = vec2<f32>(0.20, 0.44) +
        vec2<f32>(cos(phase), sin(phase)) * vec2<f32>(0.025, 0.02) * motion;
    let ripple_origin1 = vec2<f32>(0.78, 0.58) +
        vec2<f32>(cos(-phase + 1.8), sin(-phase + 1.8)) * vec2<f32>(0.022, 0.018) * motion;
    let ripple_origin2 = vec2<f32>(0.50, 0.34) +
        vec2<f32>(cos(phase + 3.6), sin(phase + 3.6)) * vec2<f32>(0.024, 0.018) * motion;
    let max_ripple_radius = mix(1.55, 1.2 + diffusion * 0.25, ring_definition);
    let diffuse_width = 0.22 + diffusion * 0.16;
    let defined_width = 0.10 + diffusion * 0.08;
    let ripple_width = mix(diffuse_width, defined_width, ring_definition);
    let ripple0 = album_drop_wave(
        input.uv,
        ripple_origin0,
        aspect,
        fract(input.time),
        max_ripple_radius,
        ripple_width,
    );
    let ripple1 = album_drop_wave(
        input.uv,
        ripple_origin1,
        aspect,
        fract(input.time + 0.3333333),
        max_ripple_radius,
        ripple_width * 1.06,
    );
    let ripple2 = album_drop_wave(
        input.uv,
        ripple_origin2,
        aspect,
        fract(input.time + 0.6666667),
        max_ripple_radius,
        ripple_width * 0.96,
    );
    let flowing_point = album_glow_flow_warp(input.uv, glow_time, motion);
    let base_point = mix(flowing_point, input.uv, ring_definition);
    let point = base_point + (ripple0.xy + ripple1.xy + ripple2.xy)
        * ripple_displacement
        * motion
        * mix(0.12, 1.0, ring_definition);
    let orbit_center0 = vec2<f32>(-0.04, 0.08) +
        vec2<f32>(cos(phase + 0.2), sin(phase + 0.2)) * vec2<f32>(0.13, 0.09) * motion;
    let orbit_center1 = vec2<f32>(0.82, -0.08) +
        vec2<f32>(cos(phase + 1.4), sin(phase + 1.4)) * vec2<f32>(0.12, 0.10) * motion;
    let orbit_center2 = vec2<f32>(-0.06, 0.78) +
        vec2<f32>(cos(phase + 2.6), sin(phase + 2.6)) * vec2<f32>(0.14, 0.09) * motion;
    let orbit_center3 = vec2<f32>(1.04, 0.62) +
        vec2<f32>(cos(phase + 3.8), sin(phase + 3.8)) * vec2<f32>(0.13, 0.11) * motion;
    let orbit_center4 = vec2<f32>(0.38, 0.42) +
        vec2<f32>(cos(-phase + 0.8), sin(-phase + 0.8)) * vec2<f32>(0.17, 0.13) * motion;
    let orbit_center5 = vec2<f32>(0.68, 1.04) +
        vec2<f32>(cos(-phase + 2.2), sin(-phase + 2.2)) * vec2<f32>(0.15, 0.10) * motion;

    // Album glow emits staggered color fields from the center. Every source
    // fades before its normalized time wraps, so the long loop has no jump.
    // The ripple preset mixes back to the orbiting centers above.
    let clock0 = glow_time;
    let clock1 = glow_time + 0.11;
    let clock2 = glow_time + 0.29;
    let clock3 = glow_time + 0.47;
    let clock4 = glow_time + 0.71;
    let clock5 = glow_time + 0.86;
    let flow0 = fract(clock0);
    let flow1 = fract(clock1);
    let flow2 = fract(clock2);
    let flow3 = fract(clock3);
    let flow4 = fract(clock4);
    let flow5 = fract(clock5);
    let seed0 = 1.3 + floor(clock0) * 13.71;
    let seed1 = 2.9 + floor(clock1) * 17.37;
    let seed2 = 4.7 + floor(clock2) * 19.91;
    let seed3 = 6.1 + floor(clock3) * 23.53;
    let seed4 = 8.3 + floor(clock4) * 29.17;
    let seed5 = 9.7 + floor(clock5) * 31.61;
    let center0 = mix(album_glow_flow_center(flow0, seed0, motion), orbit_center0, ring_definition);
    let center1 = mix(album_glow_flow_center(flow1, seed1, motion), orbit_center1, ring_definition);
    let center2 = mix(album_glow_flow_center(flow2, seed2, motion), orbit_center2, ring_definition);
    let center3 = mix(album_glow_flow_center(flow3, seed3, motion), orbit_center3, ring_definition);
    let center4 = mix(album_glow_flow_center(flow4, seed4, motion), orbit_center4, ring_definition);
    let center5 = mix(album_glow_flow_center(flow5, seed5, motion), orbit_center5, ring_definition);
    let life0 = mix(album_glow_flow_life(flow0, seed0), 1.0, ring_definition);
    let life1 = mix(album_glow_flow_life(flow1, seed1), 1.0, ring_definition);
    let life2 = mix(album_glow_flow_life(flow2, seed2), 1.0, ring_definition);
    let life3 = mix(album_glow_flow_life(flow3, seed3), 1.0, ring_definition);
    let life4 = mix(album_glow_flow_life(flow4, seed4), 1.0, ring_definition);
    let life5 = mix(album_glow_flow_life(flow5, seed5), 1.0, ring_definition);

    // A larger diffusion value lowers the falloff, so every output pixel mixes
    // several cover colors instead of showing isolated bands or blobs.
    let softness = mix(5.2, 2.4, diffusion);
    let softness0 = softness * mix(mix(1.72, 0.62, flow0), 1.0, ring_definition);
    let softness1 = softness * mix(mix(1.42, 0.74, flow1), 1.0, ring_definition);
    let softness2 = softness * mix(mix(1.83, 0.66, flow2), 1.0, ring_definition);
    let softness3 = softness * mix(mix(1.51, 0.58, flow3), 1.0, ring_definition);
    let softness4 = softness * mix(mix(1.67, 0.78, flow4), 1.0, ring_definition);
    let softness5 = softness * mix(mix(1.36, 0.63, flow5), 1.0, ring_definition);
    let weight0 = mix(
        album_glow_stream_weight(point, center0, softness0, flow0, seed0),
        album_glow_weight(point, center0, softness0),
        ring_definition,
    ) * priority0 * life0;
    let weight1 = mix(
        album_glow_stream_weight(point, center1, softness1, flow1, seed1),
        album_glow_weight(point, center1, softness1),
        ring_definition,
    ) * priority1 * life1;
    let weight2 = mix(
        album_glow_stream_weight(point, center2, softness2, flow2, seed2),
        album_glow_weight(point, center2, softness2),
        ring_definition,
    ) * priority2 * life2;
    let weight3 = mix(
        album_glow_stream_weight(point, center3, softness3, flow3, seed3),
        album_glow_weight(point, center3, softness3),
        ring_definition,
    ) * priority3 * life3;
    let weight4 = mix(
        album_glow_stream_weight(point, center4, softness4, flow4, seed4),
        album_glow_weight(point, center4, softness4),
        ring_definition,
    ) * priority4 * life4;
    let weight5 = mix(
        album_glow_stream_weight(point, center5, softness5, flow5, seed5),
        album_glow_weight(point, center5, softness5),
        ring_definition,
    ) * priority5 * life5;
    let total = max(weight0 + weight1 + weight2 + weight3 + weight4 + weight5, 0.0001);
    let flow_color = (
        color0 * weight0
        + color1 * weight1
        + color2 * weight2
        + color3 * weight3
        + color4 * weight4
        + color5 * weight5
    ) / total;
    // Outside a plume, settle toward one coherent artwork-derived base color
    // instead of normalizing a nearly-zero nearest source into a hard color
    // territory. Inside a plume, let the transported color take over softly.
    let flow_coverage = smoothstep(0.045, 1.25, total);
    let flow_mix = mix(0.04 + flow_coverage * 0.86, 1.0, ring_definition);
    var rgb = mix(palette_color * 0.82, flow_color, flow_mix);

    // Re-evaluate the moving sources with a tighter falloff to create broad
    // emissive halos on top of the already diffused color field.
    let glow_softness = softness * 2.5;
    let glow0_softness = glow_softness * softness0 / softness;
    let glow1_softness = glow_softness * softness1 / softness;
    let glow2_softness = glow_softness * softness2 / softness;
    let glow3_softness = glow_softness * softness3 / softness;
    let glow4_softness = glow_softness * softness4 / softness;
    let glow5_softness = glow_softness * softness5 / softness;
    let glow0 = mix(
        album_glow_stream_weight(point, center0, glow0_softness, flow0, seed0),
        album_glow_weight(point, center0, glow0_softness),
        ring_definition,
    ) * priority0 * life0;
    let glow1 = mix(
        album_glow_stream_weight(point, center1, glow1_softness, flow1, seed1),
        album_glow_weight(point, center1, glow1_softness),
        ring_definition,
    ) * priority1 * life1;
    let glow2 = mix(
        album_glow_stream_weight(point, center2, glow2_softness, flow2, seed2),
        album_glow_weight(point, center2, glow2_softness),
        ring_definition,
    ) * priority2 * life2;
    let glow3 = mix(
        album_glow_stream_weight(point, center3, glow3_softness, flow3, seed3),
        album_glow_weight(point, center3, glow3_softness),
        ring_definition,
    ) * priority3 * life3;
    let glow4 = mix(
        album_glow_stream_weight(point, center4, glow4_softness, flow4, seed4),
        album_glow_weight(point, center4, glow4_softness),
        ring_definition,
    ) * priority4 * life4;
    let glow5 = mix(
        album_glow_stream_weight(point, center5, glow5_softness, flow5, seed5),
        album_glow_weight(point, center5, glow5_softness),
        ring_definition,
    ) * priority5 * life5;
    let glow_total = max(glow0 + glow1 + glow2 + glow3 + glow4 + glow5, 0.0001);
    let glow_color = (
        color0 * glow0 +
        color1 * glow1 +
        color2 * glow2 +
        color3 * glow3 +
        color4 * glow4 +
        color5 * glow5
    ) / glow_total;
    let glow_mask = clamp(glow_total * 0.22, 0.0, 0.5);
    rgb += glow_color * glow_mask * glow_strength;

    // The crest is a single soft ring, while the wider halo gives it the diffuse
    // light of a drop spreading across calm water.
    let ripple_crest = ripple0.z + ripple1.z + ripple2.z;
    let ripple_halo = ripple0.w + ripple1.w + ripple2.w;
    rgb *= 1.0
        + ripple_crest * ripple_light * ring_definition * motion
        + ripple_halo
            * ripple_light
            * (1.0 - ring_definition)
            * 0.14
            * motion;

    let luminance = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3<f32>(luminance), rgb, saturation) * brightness;
    let centered = input.uv - vec2<f32>(0.5);
    let vignette_point = centered * vec2<f32>(0.85, 1.15);
    let vignette = 1.0 - smoothstep(0.18, 0.82, dot(vignette_point, vignette_point));
    let lower_shade = mix(1.0, 0.72, smoothstep(0.35, 1.0, input.uv.y));
    let pulse = 1.0 - 0.01 * motion + 0.01 * motion * sin(phase);
    let light_center = vec2<f32>(0.5) + vec2<f32>(
        cos(phase + 2.0) * 0.36 * motion,
        sin(phase + 2.0) * 0.24 * motion,
    );
    let light_delta = (input.uv - light_center) * vec2<f32>(1.0, 1.35);
    let moving_light = exp(-dot(light_delta, light_delta) * 5.0);
    rgb *= mix(0.72, 1.0, vignette)
        * lower_shade
        * pulse
        * (0.92 + moving_light * 0.16 * motion);
    return vec4<f32>(max(rgb, vec3<f32>(0.0)), 1.0);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_shaders_have_stable_distinct_ids() {
        let colors = [
            gpui::rgb(0xff1744),
            gpui::rgb(0x2979ff),
            gpui::rgb(0x00e676),
            gpui::rgb(0xffea00),
        ];
        let aurora_id = aurora(colors).shader().id();
        assert_eq!(aurora_id, aurora(colors).shader().id());
        assert_ne!(aurora_id, plasma(colors).shader().id());
        assert_ne!(aurora_id, color_orbs(colors).shader().id());
        assert!(album_glow_shader().uses_image());
    }
}
