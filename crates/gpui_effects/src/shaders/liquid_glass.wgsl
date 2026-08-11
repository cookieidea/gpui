// Rounded liquid-glass backdrop displacement.

fn liquid_smooth_step(edge_0: f32, edge_1: f32, value: f32) -> f32 {
    let amount = clamp((value - edge_0) / (edge_1 - edge_0), 0.0, 1.0);
    return amount * amount * (3.0 - 2.0 * amount);
}

fn liquid_rounded_rect_sdf(
    position: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32,
) -> f32 {
    let corner = abs(position) - half_size + vec2<f32>(radius);
    return min(max(corner.x, corner.y), 0.0)
        + length(max(corner, vec2<f32>(0.0)))
        - radius;
}

fn liquid_saturate(color: vec3<f32>, amount: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luminance), color, amount);
}

fn backdrop_effect(input: BackdropInput, params: BackdropParams) -> vec4<f32> {
    let optics = params.slots[0];
    let surface = params.slots[1];
    let tint = params.slots[2];
    let geometry = params.slots[3];
    let edge_light = params.slots[5];

    let refraction_pixels = max(optics.x, 0.0);
    let dispersion_pixels = max(optics.y, 0.0);
    let raw_detail = clamp(optics.z, 0.0, 1.0);
    let saturation = max(optics.w, 0.0);
    let edge_width = clamp(surface.x, 0.001, 0.5);
    let highlight_strength = max(surface.y, 0.0);
    let contrast = max(surface.w, 0.0);
    let padding = max(geometry.x, 0.0);
    let thickness = max(geometry.z, 1.0);
    let magnification = max(geometry.w, 0.0);

    let local_position = input.uv * input.size - input.size * 0.5;
    let half_size = max(input.size * 0.5 - vec2<f32>(padding), vec2<f32>(1.0));
    let panel_size = half_size * 2.0;
    let centered_uv = local_position / panel_size;
    let radius = clamp(geometry.y, 0.0, min(half_size.x, half_size.y));
    let shape_distance = liquid_rounded_rect_sdf(local_position, half_size, radius);
    let coverage = smoothstep(1.15, -1.15, shape_distance);

    // Shape a broad central lens and a stronger rounded edge, then convert the
    // source-coordinate delta into pixels for GPUI's backdrop sampler.
    let lens_distance = liquid_rounded_rect_sdf(
        centered_uv,
        vec2<f32>(0.3, 0.2),
        0.6,
    );
    let displacement = liquid_smooth_step(0.8, 0.0, lens_distance - 0.15);
    let scaled = liquid_smooth_step(0.0, 1.0, displacement);
    let source_position = centered_uv * scaled * panel_size;
    let lens_strength = refraction_pixels
        / thickness
        * (0.72 + magnification * 3.0);
    let displacement_pixels = (source_position - local_position) * lens_strength;

    let radial_direction = normalize(centered_uv + vec2<f32>(0.0001));
    let chromatic = radial_direction * dispersion_pixels;
    let red = sample_raw_backdrop(input, displacement_pixels + chromatic);
    let green = sample_raw_backdrop(input, displacement_pixels);
    let blue = sample_raw_backdrop(input, displacement_pixels - chromatic);
    let refracted = vec3<f32>(red.r, green.g, blue.b);
    let blurred = sample_blurred_backdrop(input, displacement_pixels * 0.12);

    var color = mix(blurred.rgb, refracted, raw_detail);
    color = liquid_saturate(color, saturation);
    color = (color - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5);
    color *= 1.05;
    color = mix(color, tint.rgb, clamp(tint.a, 0.0, 1.0));

    // Add an inset highlight and lower inner shadow. The renderer performs the
    // actual rounded-rectangle clipping.
    let interior_depth = max(-shape_distance, 0.0);
    let rim_width = 8.0 + edge_width * 32.0;
    let rim = 1.0 - smoothstep(0.0, rim_width, interior_depth);
    let panel_uv = centered_uv + vec2<f32>(0.5);
    let upper_light = rim
        * (1.0 - panel_uv.y)
        * highlight_strength
        * edge_light.a;
    let lower_shadow = rim * panel_uv.y * 0.10 * edge_light.a;
    color += edge_light.rgb * upper_light;
    color -= color * lower_shadow;

    let source_alpha = max(max(red.a, green.a), max(blue.a, blurred.a));
    return vec4<f32>(max(color, vec3<f32>(0.0)), source_alpha * coverage);
}
