fn album_glow_weight(point: vec2<f32>, center: vec2<f32>, softness: f32) -> f32 {
    let delta = point - center;
    return exp(-dot(delta, delta) * softness);
}

fn album_glow_color(input: EffectInput, uv: vec2<f32>) -> vec3<f32> {
    return sample_effect_image(input, clamp(uv, vec2<f32>(0.03), vec2<f32>(0.97))).rgb;
}

fn album_glow_hash(seed: f32) -> f32 {
    return fract(sin(seed * 91.173 + 17.17) * 43758.5453);
}

fn album_glow_center(seed: f32) -> vec2<f32> {
    return vec2<f32>(
        mix(0.14, 0.86, album_glow_hash(seed + 4.0)),
        mix(0.12, 0.88, album_glow_hash(seed + 5.0)),
    );
}

fn album_flow_clock(time: f32, seed: f32) -> f32 {
    return time
        + sin(time * 0.071 + seed * 1.7) * 2.4
        + sin(time * 0.031 - seed * 2.3) * 4.2;
}

fn album_flow_uv(
    input: EffectInput,
    uv: vec2<f32>,
    time: f32,
    motion: f32,
    flow_scale: f32,
    drift: f32,
    seed: f32,
) -> vec2<f32> {
    let aspect = input.size.x / max(input.size.y, 1.0);
    var metric_scale = vec2<f32>(1.0);
    if (aspect > 1.0) {
        metric_scale.x = aspect;
    } else {
        metric_scale.y = 1.0 / max(aspect, 0.0001);
    }

    let point = (uv - vec2<f32>(0.5)) * metric_scale;
    let clock = album_flow_clock(time, seed);
    let slow = vec2<f32>(
        sin(
            point.y * 2.31
                + clock * 0.071
                + sin(point.x * 1.73 - clock * 0.029 + seed) * 0.72
        ),
        cos(
            point.x * 2.03
                - clock * 0.063
                + sin(point.y * 1.91 + clock * 0.023 - seed) * 0.66
        ),
    );
    let weave = vec2<f32>(
        sin(
            (point.x + point.y) * 3.17
                - clock * 0.109
                + cos(point.y * 2.43 + seed) * 0.38
        ),
        cos(
            (point.x - point.y) * 2.71
                + clock * 0.127
                + sin(point.x * 2.19 - seed) * 0.33
        ),
    );
    let lively_span = 0.5 + 0.5 * sin(time * 0.019 + seed * 3.1);
    let eddy = vec2<f32>(
        sin(point.y * 5.27 + clock * 0.211 + sin(point.x * 3.11) * 0.27),
        cos(point.x * 4.83 - clock * 0.187 + sin(point.y * 3.43) * 0.24),
    );
    let local_flow = slow * 0.084
        + weave * 0.043
        + eddy * mix(0.009, 0.026, lively_span);
    let transport = vec2<f32>(
        sin(time * 0.023 + seed * 2.9),
        cos(time * 0.017 - seed * 2.1),
    ) * 0.052 * drift;

    return uv + (local_flow * flow_scale + transport) * motion / metric_scale;
}

fn effect(input: EffectInput, params: EffectParams) -> vec4<f32> {
    let blur = max(params.slots[0].x, 0.0);
    let saturation = max(params.slots[0].y, 0.0);
    let brightness = max(params.slots[0].z, 0.0);
    let motion = max(params.slots[0].w, 0.0);
    let flow_scale = max(params.slots[1].x, 0.0);
    let drift = max(params.slots[1].y, 0.0);
    let vignette_strength = clamp(params.slots[1].z, 0.0, 1.0);
    let seed = params.slots[1].w;
    let glow_strength = max(params.slots[2].x, 0.0);
    let diffusion = clamp(blur / 0.18, 0.0, 1.0);

    // Keep the previous background construction: nine equal-area samples,
    // Gaussian color mixing, the same saturation, brightness and shading.
    let palette = array<vec3<f32>, 9>(
        album_glow_color(input, vec2<f32>(0.16, 0.16)),
        album_glow_color(input, vec2<f32>(0.50, 0.14)),
        album_glow_color(input, vec2<f32>(0.84, 0.18)),
        album_glow_color(input, vec2<f32>(0.14, 0.50)),
        album_glow_color(input, vec2<f32>(0.50, 0.50)),
        album_glow_color(input, vec2<f32>(0.86, 0.50)),
        album_glow_color(input, vec2<f32>(0.18, 0.84)),
        album_glow_color(input, vec2<f32>(0.50, 0.86)),
        album_glow_color(input, vec2<f32>(0.82, 0.82)),
    );

    // Only movement changes: the old palette territories stay fixed and the
    // current continuous flow transports the complete blurred color field.
    let flowing_point = album_flow_uv(
        input,
        input.uv,
        input.time,
        motion,
        flow_scale,
        drift,
        seed,
    );
    var palette_color = vec3<f32>(0.0);
    var flow_color = vec3<f32>(0.0);
    var flow_total = 0.0;
    var local_total = 0.0;
    for (var index: u32 = 0u; index < 9u; index = index + 1u) {
        let palette_seed = 2.7 + f32(index) * 17.173;
        let softness = mix(8.0, 17.0, album_glow_hash(palette_seed + 7.0));
        let local = album_glow_weight(
            flowing_point,
            album_glow_center(palette_seed),
            softness,
        );
        let secondary = album_glow_weight(
            flowing_point,
            album_glow_center(palette_seed + 71.9),
            softness * 0.48,
        );
        let weight = 0.004 + local * 0.82 + secondary * 0.24;
        palette_color += palette[index];
        flow_color += palette[index] * weight;
        flow_total += weight;
        local_total += local + secondary * 0.5;
    }
    palette_color /= 9.0;
    var rgb = flow_color / max(flow_total, 0.0001);
    let glow_density = smoothstep(1.4, 5.8, local_total);
    rgb += mix(palette_color, rgb, 0.65)
        * glow_density
        * glow_strength
        * 0.10;

    let luminance = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    rgb = mix(vec3<f32>(luminance), rgb, saturation) * brightness;

    let centered = input.uv - vec2<f32>(0.5);
    let vignette_point = centered * vec2<f32>(0.85, 1.15);
    let vignette = 1.0 - smoothstep(0.18, 0.82, dot(vignette_point, vignette_point));
    let lower_shade = mix(1.0, 0.72, smoothstep(0.35, 1.0, input.uv.y));
    let light_center = album_flow_uv(
        input,
        album_glow_center(113.7),
        input.time,
        motion * 0.45,
        flow_scale,
        drift,
        seed,
    );
    let light_delta = (input.uv - light_center) * vec2<f32>(1.0, 1.35);
    let moving_light = exp(-dot(light_delta, light_delta) * 5.0);
    rgb *= mix(1.0 - vignette_strength, 1.0, vignette)
        * lower_shade
        * (0.94 + moving_light * 0.10 * motion);

    return vec4<f32>(max(rgb, vec3<f32>(0.0)), 1.0);
}
