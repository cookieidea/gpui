struct GlobalParams {
    viewport_size: vec2<f32>,
    premultiplied_alpha: u32,
    pad: u32,
}

struct Bounds {
    origin: vec2<f32>,
    size: vec2<f32>,
}

struct BackdropParams {
    slots: array<vec4<f32>, 8>,
}

struct BackdropInstance {
    bounds: Bounds,
    content_mask: Bounds,
    corner_radii: vec4<f32>,
    blur_radius: f32,
    opacity: f32,
    time: f32,
    pointer_active: f32,
    direction: vec2<f32>,
    pointer: vec2<f32>,
    uniforms: BackdropParams,
}

@group(0) @binding(0) var<uniform> globals: GlobalParams;
@group(1) @binding(0) var<storage, read> b_backdrops: array<BackdropInstance>;
@group(1) @binding(1) var t_backdrop: texture_2d<f32>;

fn sample_backdrop(uv: vec2<f32>) -> vec4<f32> {
    let dimensions = vec2<f32>(textureDimensions(t_backdrop));
    let position = clamp(
        uv * dimensions - vec2<f32>(0.5),
        vec2<f32>(0.0),
        dimensions - vec2<f32>(1.0),
    );
    let low = vec2<i32>(floor(position));
    let high = min(low + vec2<i32>(1), vec2<i32>(dimensions) - vec2<i32>(1));
    let factor = fract(position);
    let top = mix(
        textureLoad(t_backdrop, low, 0),
        textureLoad(t_backdrop, vec2<i32>(high.x, low.y), 0),
        factor.x,
    );
    let bottom = mix(
        textureLoad(t_backdrop, vec2<i32>(low.x, high.y), 0),
        textureLoad(t_backdrop, high, 0),
        factor.x,
    );
    return mix(top, bottom, factor.y);
}

struct BackdropVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) instance_id: u32,
}

fn unit_vertex(vertex_id: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex_id & 1u), 0.5 * f32(vertex_id & 2u));
}

fn to_device_position(unit: vec2<f32>, bounds: Bounds) -> vec4<f32> {
    let position = unit * bounds.size + bounds.origin;
    let device = position / globals.viewport_size * vec2<f32>(2.0, -2.0)
        + vec2<f32>(-1.0, 1.0);
    return vec4<f32>(device, 0.0, 1.0);
}

@vertex
fn vs_backdrop(
    @builtin(vertex_index) vertex_id: u32,
    @builtin(instance_index) instance_id: u32,
) -> BackdropVarying {
    var out: BackdropVarying;
    out.position = to_device_position(unit_vertex(vertex_id), b_backdrops[instance_id].bounds);
    out.instance_id = instance_id;
    return out;
}

@fragment
fn fs_blur(input: BackdropVarying) -> @location(0) vec4<f32> {
    let instance = b_backdrops[input.instance_id];
    let uv = input.position.xy / max(instance.content_mask.size, vec2<f32>(1.0));
    let step = instance.direction * instance.blur_radius
        / max(globals.viewport_size, vec2<f32>(1.0)) * 0.25;

    var color = sample_backdrop(uv) * 0.227027;
    color += sample_backdrop(uv + step * 1.0) * 0.1945946;
    color += sample_backdrop(uv - step * 1.0) * 0.1945946;
    color += sample_backdrop(uv + step * 2.0) * 0.1216216;
    color += sample_backdrop(uv - step * 2.0) * 0.1216216;
    color += sample_backdrop(uv + step * 3.0) * 0.054054;
    color += sample_backdrop(uv - step * 3.0) * 0.054054;
    color += sample_backdrop(uv + step * 4.0) * 0.016216;
    color += sample_backdrop(uv - step * 4.0) * 0.016216;
    return color;
}
