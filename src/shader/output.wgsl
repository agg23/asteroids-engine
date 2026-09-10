// Reads from the phosphor texture and renders it

// This portion of the brightness range will be matched 1:1, the rest of the range will be expanded/compressed
const EXACT_BRIGHTNESS_FRACTION = 0.7;

// The brightness of a single beam tick, where 1.0 is SDR white
const BRIGHTNESS_PER_BEAM_TICK: f32 = 2.0;

// Phosphor emission chromacity in sRGB, normalized to max channel brightness at 1.0
// Asteroids uses 9300k as its whitepoint, which is at chromacity 0.283, 0.298. Mapping from chromacity to sRGB results in vec3f(0.839193, 1.014020, 1.335080). Normalizing by field produces:
// const PHOSPHOR_TINT: vec3f = vec3f(0.629, 0.760, 1.000);
const PHOSPHOR_TINT: vec3f = vec3f(0.35, 0.55, 1.0);

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> @builtin(position) vec4f {
    // One triangle covering the whole screen (-1,-1), (3,-1), (-1,3)
    let corner = vec2f(f32(vertex_index / 2u), f32(vertex_index % 2u)) * 4.0 - 1.0;
    return vec4f(corner, 0.0, 1.0);
}

struct SharedUniforms {
    hdr_headroom: f32,
}

@group(0) @binding(0) var energy_texture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> uniforms: SharedUniforms;

@fragment
fn fs_main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    // Texture is supersampled. Apply a box blur, averaging each surrounding "pixel"'s energy level, then apply the tonemapping
    // Get supersampled position representing this output position
    let supersampled_position = vec2i(position.xy) * SUPERSAMPLE_MULTIPLIER;

    var energy = 0.0;
    for (var y = 0; y < SUPERSAMPLE_MULTIPLIER; y++) {
        for (var x = 0; x < SUPERSAMPLE_MULTIPLIER; x++) {
            // Read phosphor texture at 2D coord. Everything is in the red channel, so read it only
            energy += textureLoad(energy_texture, supersampled_position + vec2i(x, y), 0).r;
        }
    }

    // Average energy based on supersampling ratio
    energy = energy / f32(SUPERSAMPLE_MULTIPLIER * SUPERSAMPLE_MULTIPLIER);

    let color = remap_into_hdr(energy, uniforms.hdr_headroom) * PHOSPHOR_TINT;

    return vec4f(color, 1.0);
}

// Apply tonemapping and rescale brightness into any available HDR space
fn remap_into_hdr(energy: f32, hdr_headroom: f32) -> f32 {
    let max_brightness = max(hdr_headroom, 1.0);
    // Up until this point, brightness is 1:1
    let exact_brightness_end_point = EXACT_BRIGHTNESS_FRACTION * max_brightness;
    let scaled_max_range = max_brightness - exact_brightness_end_point;

    let base_brightness = energy * BRIGHTNESS_PER_BEAM_TICK;

    let excess_brightness = max(base_brightness - exact_brightness_end_point, 0.0);
    return min(base_brightness, exact_brightness_end_point) + scaled_max_range * (1.0 - exp(-excess_brightness / scaled_max_range));
}
