// Reads from the phosphor texture and renders it

// Scale brightness so it "fits" in range
const EXPOSURE: f32 = 1.0;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
) -> @builtin(position) vec4f {
    // One triangle covering the whole screen (-1,-1), (3,-1), (-1,3)
    let corner = vec2f(f32(vertex_index / 2u), f32(vertex_index % 2u)) * 4.0 - 1.0;
    return vec4f(corner, 0.0, 1.0);
}

@group(0) @binding(0) var energy_texture: texture_2d<f32>;

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

    // Perform tonemapping
    // The probability of a Poisson distributed electron emission hitting a single eye "element" is:
    let probabilty_of_impact = 1.0 - exp(-EXPOSURE * energy);

    // Apply gamma normalization (at 2.2)
    let output = pow(probabilty_of_impact, 1.0/2.2);

    return vec4f(output, output, output, 1.0);
}
