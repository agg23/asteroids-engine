// Rough line shader
// Vertex shader converts vector coordinates to the [-1, 1] GPU space
// Fragment shader renders a constant intensity along the vector

struct VsOut {
    @builtin(position) clip_position: vec4f,
    @location(0) intensity: f32,
}

@vertex
fn vs_main(
    // 0 for starrt, 1 for dest
    @builtin(vertex_index) vertex_index: u32,
    // Layout declared in LineInstance
    @location(0) start: vec2<u32>,
    @location(1) dest: vec2<u32>,
    @location(2) intensity: u32,
    @location(3) flags: u32,
) -> VsOut {
    var out: VsOut;

    let forward_vector = vec2<f32>((dest - start) & vec2<u32>(0xFFF));
    let backwards_vector = vec2<f32>((start - dest) & vec2<u32>(0xFFF));

    let negative_x = (flags & 0x1) != 0;
    let negative_y = (flags & 0x2) != 0;

    let negative = vec2<bool>(negative_x, negative_y);

    let delta = select(forward_vector, -backwards_vector, negative);
    // TODO: We shouldn't really need this
    let is_point = delta == vec2<f32>(0.0);
    let delta_with_point = select(delta, vec2<f32>(1.0, 0.0), is_point);

    // TODO: This shouldn't happen in practice, but this lets `start` be out of bounds
    let startf = vec2<f32>(start);
    let normalized_start = select(startf, startf - 4096.0, startf > vec2<f32>(2048.0));
    let position = select(normalized_start, normalized_start + delta_with_point, vertex_index == 1);

    // Normalize [0, 1024) to fit into [-1, +1]
    let normalized_position = vec2f(position) / 1024.0 * 2.0 - 1.0;

    // z = 0 (no depth), w = 1 (no perspective)
    out.clip_position = vec4f(normalized_position, 0.0, 1.0);
    out.intensity = f32(intensity) / 15.0;

    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    if (in.intensity == 0.0) {
        discard;
    }

    return vec4f(in.intensity, in.intensity, in.intensity, 1.0);
}
