// Decay all energy in the phosphor
const DECAY: f32 = 0.5;

@group(0) @binding(0) var state_in: texture_2d<f32>;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> @builtin(position) vec4f {
    // One triangle covering the whole screen (-1,-1), (3,-1), (-1,3)
    let corner = vec2f(f32(vertex_index / 2u), f32(vertex_index % 2u)) * 4.0 - 1.0;
    return vec4f(corner, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let energy = textureLoad(state_in, vec2i(position.xy), 0).r;
    return vec4f(energy * DECAY, 0.0, 0.0, 1.0);
}
