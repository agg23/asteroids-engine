// Stamp beam to each step position for a given active time
// Vertex shader converts vector coordinates to the [-1, 1] GPU space
// Fragment shader renders a constant intensity along the vector

// TODO: What are these?
// Values are relative to the DISPLAY_RESOLUTION (1024)
// Gaussian sigma. Beam spot stddev
const SIGMA: f32 = 0.7;
// How far the Gaussian reaches from the center of the quad, at which point we clip to 0
const REACH: f32 = 3.0;

const PI = radians(180.0);

struct VsOut {
    @builtin(position) clip_position: vec4f,
    // The fragment shader normalized position, interpolated across the quad
    @location(0) local_position: vec2f,
    @location(1) energy: f32,
}

@vertex
fn vs_main(
    // 0 for starrt, 1 for dest
    @builtin(vertex_index) vertex_index: u32,
    // Layout declared in BeamStepInstance
    // @location(0) ticks: u64,
    @location(0) active_ticks: u32,
    @location(1) ticks_until_end_of_frame: u32,
    @location(2) dest: vec2<u32>,
    @location(3) intensity: u32,
    // @location(3) _padding: u32,
) -> VsOut {
    // The quad covering [-1, 1]
    var corners = array<vec2f, 6>(
        vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
        vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0),
    );
    // Get corner coresponding to this vertex
    let corner = corners[vertex_index];

    // Normalize from [0, 1024) to [-1, 1]
    // Receiving 1024 and mapping to a total range of 2, starting at -1
    // The + 0.5 centers the texel
    let normalized_position = (vec2f(dest) + 0.5) / DISPLAY_RESOLUTION * 2.0 - 1.0;

    // Reach on one side of the Gaussian in the normalized coordinate space
    let normalized_half_reach = REACH * SIGMA * (2.0 / DISPLAY_RESOLUTION);

    var out: VsOut;
    // Take our position and move towards the corresponding corner of the quad by our REACH
    // z = 0 (no depth), w = 1 (no perspective)
    out.clip_position = vec4f(normalized_position + corner * normalized_half_reach, 0.0, 1.0);
    // TODO: Why doesn't this have to be normalized?
    out.local_position = corner * REACH;
    out.energy = f32(intensity) / 15.0 * f32(active_ticks);

    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) f32 {
    let falloff = exp(-0.5 * dot(in.local_position, in.local_position)) / (2 * PI * SIGMA * SIGMA);
    let energy = in.energy * falloff;
    return energy;
}
