struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(position, 0.0, 1.0);
    output.uv = uv;
    return output;
}

@group(0) @binding(0) var image_tile: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

struct DisplayAdjustment {
    tone_and_rg: vec4<f32>,
    blue_and_normalization_tone: vec4<f32>,
    normalization_rgb_and_padding: vec4<f32>,
};

@group(1) @binding(0) var<uniform> adjustment: DisplayAdjustment;

fn apply_adjustment(
    color: vec3<f32>,
    color_gain: vec3<f32>,
    gamma: f32,
    exposure_ev: f32,
) -> vec3<f32> {
    let balanced = color * color_gain;
    let luminance = dot(balanced, vec3<f32>(0.2126, 0.7152, 0.0722));
    let safe_luminance = max(luminance, 0.000001);
    let mapped_luminance = pow(safe_luminance, gamma) * exp2(exposure_ev);
    return balanced * (mapped_luminance / safe_luminance);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_tile, image_sampler, input.uv);
    let color_gain = vec3<f32>(
        adjustment.tone_and_rg.z,
        adjustment.tone_and_rg.w,
        adjustment.blue_and_normalization_tone.x,
    );
    let base_adjusted = apply_adjustment(
        sampled.rgb,
        color_gain,
        adjustment.tone_and_rg.y,
        adjustment.tone_and_rg.x,
    );
    let normalization_gamma = vec3<f32>(
        adjustment.blue_and_normalization_tone.z,
        adjustment.blue_and_normalization_tone.w,
        adjustment.normalization_rgb_and_padding.y,
    );
    let normalization_gain = vec3<f32>(
        adjustment.normalization_rgb_and_padding.x,
        adjustment.normalization_rgb_and_padding.z,
        adjustment.normalization_rgb_and_padding.w,
    );
    let safe_base = max(base_adjusted, vec3<f32>(0.000001));
    let color_offset = log2(max(normalization_gain, vec3<f32>(0.25)));
    let adjusted = pow(safe_base, normalization_gamma)
        * exp2(
            vec3<f32>(adjustment.blue_and_normalization_tone.y)
                + color_offset
                    * (vec3<f32>(1.0) - clamp(safe_base, vec3<f32>(0.0), vec3<f32>(1.0))),
        );
    return vec4<f32>(adjusted, sampled.a);
}
