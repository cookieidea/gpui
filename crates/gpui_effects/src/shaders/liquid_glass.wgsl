fn glass_safe_normalize(value: vec2<f32>) -> vec2<f32> {
    let magnitude = length(value);
    if (magnitude < 0.0001) {
        return vec2<f32>(0.0, -1.0);
    }
    return value / magnitude;
}

fn glass_saturate(color: vec3<f32>, amount: f32) -> vec3<f32> {
    let luminance = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luminance), color, amount);
}

fn glass_rounded_box_distance(
    position: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32,
) -> f32 {
    let corner = abs(position) - half_size + vec2<f32>(radius);
    return length(max(corner, vec2<f32>(0.0)))
        + min(max(corner.x, corner.y), 0.0)
        - radius;
}

fn glass_shape_distance(
    position: vec2<f32>,
    input: BackdropInput,
    params: BackdropParams,
) -> f32 {
    let geometry = params.slots[3];
    let interaction = params.slots[4];
    let padding = max(geometry.x, 0.0);
    let half_size = max(input.size * 0.5 - vec2<f32>(padding), vec2<f32>(1.0));
    let radius = clamp(geometry.y, 0.0, min(half_size.x, half_size.y));
    let thickness = max(geometry.z, 1.0);
    let pressure = interaction.x;
    let velocity = interaction.yz;
    let speed = clamp(interaction.w, 0.0, 1.0);
    let motion = max(params.slots[1].w, 0.0);

    var distance = glass_rounded_box_distance(position, half_size, radius);

    // A glass surface is never perfectly still. Two low-amplitude waves move
    // the contour and, more importantly, keep the surface normal changing
    // while content travels underneath it.
    let tau = 6.28318530718;
    let phase = input.time * tau;
    let ambient_ripple = sin(position.x * 0.021 + phase)
        * cos(position.y * 0.018 - phase * 2.0);
    distance += ambient_ripple * motion * thickness * 0.045;

    // Stretch the body along its motion axis. Expanding the leading and rear
    // edges by different amounts creates an actual silhouette deformation;
    // the previous signed offset mostly translated an unchanged rounded box.
    let motion_axis = glass_safe_normalize(velocity);
    let directional = dot(glass_safe_normalize(position), motion_axis);
    let axial = abs(directional);
    let leading = max(directional, 0.0);
    let trailing = max(-directional, 0.0);
    distance -= speed
        * thickness
        * (axial * 0.28 + leading * 0.28 + trailing * 0.10);
    distance += speed * thickness * (1.0 - axial) * 0.12;

    let pointer_position = (input.pointer - vec2<f32>(0.5)) * input.size;
    let pointer_delta = position - pointer_position;
    let pressure_radius = max(thickness * 3.6, 52.0);
    let pressure_field = exp(
        -dot(pointer_delta, pointer_delta)
        / max(pressure_radius * pressure_radius, 1.0),
    ) * input.pointer_active;

    // A pressed gel surface bulges around contact. The damped Rust-side spring
    // lets this term briefly cross zero on release, producing a soft rebound.
    distance -= pressure * pressure_field * thickness * 0.24;
    let rebound_wave = sin(
        length(pointer_delta) * 0.105
        - input.time * 6.28318530718 * 2.0,
    );
    distance -= rebound_wave
        * abs(pressure)
        * pressure_field
        * thickness
        * 0.055;
    return distance;
}

fn glass_shape_normal(
    position: vec2<f32>,
    input: BackdropInput,
    params: BackdropParams,
) -> vec2<f32> {
    let step = 1.25;
    let horizontal = glass_shape_distance(position + vec2<f32>(step, 0.0), input, params)
        - glass_shape_distance(position - vec2<f32>(step, 0.0), input, params);
    let vertical = glass_shape_distance(position + vec2<f32>(0.0, step), input, params)
        - glass_shape_distance(position - vec2<f32>(0.0, step), input, params);
    return glass_safe_normalize(vec2<f32>(horizontal, vertical));
}

fn backdrop_effect(input: BackdropInput, params: BackdropParams) -> vec4<f32> {
    let optics = params.slots[0];
    let surface = params.slots[1];
    let tint = params.slots[2];
    let geometry = params.slots[3];
    let interaction = params.slots[4];
    let edge_light = params.slots[5];

    let refraction_pixels = max(optics.x, 0.0);
    let dispersion_pixels = max(optics.y, 0.0);
    let raw_detail = clamp(optics.z, 0.0, 1.0);
    let saturation = max(optics.w, 0.0);
    let edge_width = clamp(surface.x, 0.001, 0.5);
    let highlight_strength = max(surface.y, 0.0);
    let pointer_strength = max(surface.z, 0.0);
    let motion = max(surface.w, 0.0);
    let padding = max(geometry.x, 0.0);
    let thickness = max(geometry.z, 1.0);
    let magnification = clamp(geometry.w, 0.0, 0.2);
    let pressure = interaction.x;
    let velocity = interaction.yz;
    let speed = clamp(interaction.w, 0.0, 1.0);

    let local_position = input.uv * input.size - input.size * 0.5;
    let half_size = max(input.size * 0.5 - vec2<f32>(padding), vec2<f32>(1.0));
    let distance = glass_shape_distance(local_position, input, params);
    let normal = glass_shape_normal(local_position, input, params);
    let coverage = smoothstep(1.35, -1.35, distance);
    let interior_depth = clamp(-distance / thickness, 0.0, 1.0);
    let edge = exp(-abs(distance) / max(thickness * (0.24 + edge_width), 1.0));
    let inner_rim = exp(
        -abs(distance + thickness * 0.28)
        / max(thickness * 0.12, 1.0),
    );

    let pointer_position = (input.pointer - vec2<f32>(0.5)) * input.size;
    let pointer_delta = local_position - pointer_position;
    let pointer_radius = max(thickness * 4.2, 62.0);
    let pointer_field = exp(
        -dot(pointer_delta, pointer_delta)
        / max(pointer_radius * pointer_radius, 1.0),
    ) * input.pointer_active;
    let pointer_normal = glass_safe_normalize(pointer_delta);

    let tau = 6.28318530718;
    let phase = input.time * tau;
    let surface_wave = sin(
        phase * 2.0
        + input.uv.x * tau * 1.7
        - input.uv.y * tau * 1.15,
    );

    // Estimate spatial detail in the current backdrop. This is deliberately
    // not presented as temporal motion detection: without a previous-frame
    // texture the shader cannot know velocity. It does, however, let moving
    // object contours catch and reveal the animated liquid normal field.
    let detail_probe = 2.5;
    let detail_x = sample_raw_backdrop(input, vec2<f32>(detail_probe, 0.0)).rgb
        - sample_raw_backdrop(input, vec2<f32>(-detail_probe, 0.0)).rgb;
    let detail_y = sample_raw_backdrop(input, vec2<f32>(0.0, detail_probe)).rgb
        - sample_raw_backdrop(input, vec2<f32>(0.0, -detail_probe)).rgb;
    let detail_energy = clamp(
        (length(detail_x) + length(detail_y)) * 1.35,
        0.0,
        1.0,
    );
    let flow_normal = vec2<f32>(
        sin(phase + input.uv.y * tau * 2.4)
            + cos(phase * 2.0 - input.uv.x * tau * 1.8),
        cos(phase + input.uv.x * tau * 2.1)
            + sin(phase * 2.0 + input.uv.y * tau * 1.6),
    ) * 0.5;
    let interior_flow = smoothstep(0.04, 0.42, interior_depth) * coverage;
    let flow_displacement = flow_normal
        * refraction_pixels
        * motion
        * (0.16 + detail_energy * 0.34)
        * interior_flow;

    // A broad traveling lens makes a moving object visibly change scale and
    // curvature rather than merely becoming blurred behind the panel.
    let travel_center = vec2<f32>(
        sin(phase) * half_size.x * 0.34,
        cos(phase * 2.0) * half_size.y * 0.24,
    );
    let travel_delta = local_position - travel_center;
    let travel_radius = max(min(half_size.x, half_size.y) * 0.52, 48.0);
    let travel_field = exp(
        -dot(travel_delta, travel_delta)
        / max(travel_radius * travel_radius, 1.0),
    ) * interior_flow;
    let traveling_lens = -travel_delta
        * travel_field
        * magnification
        * motion
        * 0.28;

    // Convex-lens sampling: the center gently magnifies while the curved edge
    // bends rays much more strongly. Pressure adds a localized concave lens.
    let normalized_center = local_position / max(half_size, vec2<f32>(1.0));
    let lens_displacement = -normalized_center
        * min(half_size.x, half_size.y)
        * magnification
        * (0.34 + interior_depth * 0.66);
    let edge_displacement = normal
        * refraction_pixels
        * (edge * 1.25 + (1.0 - interior_depth) * 0.12)
        * (1.0 + surface_wave * 0.07 * motion);
    let pressure_displacement = -pointer_delta
        * pressure
        * pointer_field
        * pointer_strength
        * 0.13;
    let approach_displacement = pointer_normal
        * pointer_field
        * pointer_strength
        * refraction_pixels
        * 0.22;
    let inertia_displacement = velocity
        * speed
        * thickness
        * (0.22 + edge * 0.36);
    let displacement = lens_displacement
        + edge_displacement
        + flow_displacement
        + traveling_lens
        + pressure_displacement
        + approach_displacement
        + inertia_displacement;

    let chromatic_direction = normal
        * dispersion_pixels
        * (0.18 + edge * 1.25 + abs(pressure) * pointer_field * 0.45);
    let red = sample_raw_backdrop(input, displacement + chromatic_direction);
    let green = sample_raw_backdrop(input, displacement);
    let blue = sample_raw_backdrop(input, displacement - chromatic_direction);
    let refracted = vec3<f32>(red.r, green.g, blue.b);
    let blurred = sample_blurred_backdrop(input, displacement * 0.16);

    let background_luminance = dot(blurred.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let adaptive_detail = clamp(
        raw_detail + (0.5 - abs(background_luminance - 0.5)) * 0.16,
        0.0,
        1.0,
    );
    var color = mix(blurred.rgb, refracted, adaptive_detail);
    color = glass_saturate(color, saturation);

    // Environment-aware transmission keeps dark backdrops luminous and reins
    // in the tint over bright content.
    let adaptive_tint = clamp(tint.a * mix(1.28, 0.68, background_luminance), 0.0, 1.0);
    color = mix(color, tint.rgb, adaptive_tint);
    let contrast = 1.0 + edge * 0.08 + (1.0 - interior_depth) * 0.035;
    color = (color - vec3<f32>(0.5)) * contrast + vec3<f32>(0.5);

    let light_direction = glass_safe_normalize(vec2<f32>(-0.72, -0.48));
    let facing_light = 0.5 + 0.5 * dot(-normal, light_direction);
    let facing_shadow = 0.5 + 0.5 * dot(normal, light_direction);
    let fresnel = pow(1.0 - interior_depth, 2.2);
    let moving_glint = 0.86
        + 0.14 * sin(phase + dot(input.uv, vec2<f32>(8.0, -5.5)));
    let caustic = edge
        * fresnel
        * highlight_strength
        * pow(facing_light, 2.5)
        * mix(1.0, moving_glint, motion);
    let concentrated_rim = inner_rim
        * highlight_strength
        * (0.18 + 0.28 * facing_light);
    let contact_glow = pointer_field
        * input.pointer_active
        * highlight_strength
        * (0.08 + abs(pressure) * 0.22 + speed * 0.12);
    let transmitted_color = mix(edge_light.rgb, blurred.rgb, 0.2);
    color += transmitted_color
        * (caustic + concentrated_rim + contact_glow)
        * edge_light.a;
    color -= blurred.rgb
        * edge
        * facing_shadow
        * (0.055 + speed * 0.035);

    let source_alpha = max(max(red.a, green.a), max(blue.a, blurred.a));
    return vec4<f32>(max(color, vec3<f32>(0.0)), source_alpha * coverage);
}
