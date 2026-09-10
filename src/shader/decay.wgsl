// Decay all energy in the phosphor
// Rate of long tail
const A: f32 = 0.05;
// Rate of initial falloff
const B: f32 = 0.3;

// 1_512_000 / 246.09
const NMI_PERIOD_TICKS: i32 = 6144;
const TICKS_PER_FRAME: i32 = NMI_PERIOD_TICKS * 4;
const TICKS_PER_MS: i32 = 1512;

@group(0) @binding(0) var state_in: texture_2d<f32>;
// @group(0) @binding(1) var<uniform> uniforms: SharedUniforms;

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

    let elapsed_ms = f32(TICKS_PER_FRAME) / f32(TICKS_PER_MS);

    // dE/dt = −A * E - B * E^2
    let tail_exp = exp(-A * elapsed_ms);
    let decayed_energy = A * energy * tail_exp / (A + B * energy * (1.0 - tail_exp));

    return vec4f(decayed_energy, 0.0, 0.0, 1.0);
}
