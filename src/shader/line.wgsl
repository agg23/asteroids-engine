// Rough line shader
// Vertex shader converts vector coordinates to the [-1, 1] GPU space
// Fragment shader renders a constant intensity along the vector

struct VsOut {
    @builtin(position) clip_position: vec4f,
    @location(0) intensity: f32,
}

@vertex
fn vs_main(
    // Layout declared in gpu.rs
    // DVG space: [0, 1024)
    @location(0) position: vec2<u32>,
    // 4 bit
    @location(1) intensity: u32,
) -> VsOut {
    var out: VsOut;

    // Normalize [0, 1024) to fit into [-1, +1]
    let normalized_position = vec2f(position) / 1024.0 * 2.0 - 1.0;

    // z = 0 (no depth), w = 1 (no perspective)
    out.clip_position = vec4f(normalized_position, 0.0, 1.0);
    out.intensity = f32(intensity) / 16;

    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    return vec4f(in.intensity, in.intensity, in.intensity, 1.0);
}
