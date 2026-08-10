fn frosted_safe_normalize(value: vec2<f32>) -> vec2<f32> {
    let magnitude = length(value);
    if (magnitude < 0.0001) {
        return vec2<f32>(0.0, -1.0);
    }
    return value / magnitude;
}

fn frosted_rounded_box_distance(
    position: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32,
) -> f32 {
    let corner = abs(position) - half_size + vec2<f32>(radius);
    return length(max(corner, vec2<f32>(0.0)))
        + min(max(corner.x, corner.y), 0.0)
        - radius;
}

fn frosted_shape_distance(
    position: vec2<f32>,
    input: BackdropInput,
    params: BackdropParams,
) -> f32 {
    let geometry = params.slots[3];
    let padding = max(geometry.x, 0.0);
    let half_size = max(input.size * 0.5 - vec2<f32>(padding), vec2<f32>(1.0));
    let radius = clamp(geometry.y, 0.0, min(half_size.x, half_size.y));
    return frosted_rounded_box_distance(position, half_size, radius);
}

fn frosted_shape_normal(
    position: vec2<f32>,
    input: BackdropInput,
    params: BackdropParams,
) -> vec2<f32> {
    let step = 0.75;
    let horizontal = frosted_shape_distance(position + vec2<f32>(step, 0.0), input, params)
        - frosted_shape_distance(position - vec2<f32>(step, 0.0), input, params);
    let vertical = frosted_shape_distance(position + vec2<f32>(0.0, step), input, params)
        - frosted_shape_distance(position - vec2<f32>(0.0, step), input, params);
    return frosted_safe_normalize(vec2<f32>(horizontal, vertical));
}

fn frosted_saturate(color: vec3<f32>, amount: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luminance), color, amount);
}

fn backdrop_effect(input: BackdropInput, params: BackdropParams) -> vec4<f32> {
    let optics = params.slots[0];
    let surface = params.slots[1];
    let tint = params.slots[2];
    let edge_light = params.slots[5];

    let raw_detail = clamp(optics.z, 0.0, 1.0);
    let saturation = max(optics.w, 0.0);
    let edge_width = clamp(surface.x, 0.001, 0.5);
    let highlight_strength = max(surface.y, 0.0);

    let local_position = input.uv * input.size - input.size * 0.5;
    let distance = frosted_shape_distance(local_position, input, params);
    let normal = frosted_shape_normal(local_position, input, params);
    let coverage = smoothstep(1.15, -1.15, distance);
    let inside_distance = max(-distance, 0.0);

    let raw = sample_raw_backdrop(input, vec2<f32>(0.0));
    let blurred = sample_blurred_backdrop(input, vec2<f32>(0.0));
    var color = mix(blurred.rgb, raw.rgb, raw_detail);
    color = frosted_saturate(color, saturation);

    // Static, low-frequency density variation prevents a perfectly uniform
    // blur sheet without introducing animated liquid motion.
    let density = 0.5 + 0.5 * sin(input.uv.x * 8.7 + input.uv.y * 5.3);
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let tint_amount = clamp(
        tint.a * mix(1.20, 0.82, luminance) * mix(0.94, 1.06, density),
        0.0,
        1.0,
    );
    color = mix(color, tint.rgb, tint_amount);

    let rim_width = max(0.8 + edge_width * 5.0, 0.8);
    let outer_rim = exp(-abs(distance + 0.65) / rim_width);
    let inner_rim = exp(-abs(inside_distance - 4.5) / max(rim_width * 1.8, 1.0));
    let light_direction = frosted_safe_normalize(vec2<f32>(-0.24, -1.0));
    let facing_light = clamp(dot(normal, light_direction), 0.0, 1.0);
    let facing_away = clamp(dot(-normal, light_direction), 0.0, 1.0);
    let highlight = outer_rim
        * highlight_strength
        * (0.10 + pow(facing_light, 2.6) * 0.48);
    color += edge_light.rgb * edge_light.a * (highlight + inner_rim * 0.035);
    color *= 1.0
        - outer_rim
            * facing_away
            * highlight_strength
            * edge_light.a
            * 0.035;

    let source_alpha = max(raw.a, blurred.a);
    return vec4<f32>(max(color, vec3<f32>(0.0)), source_alpha * coverage);
}
