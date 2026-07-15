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
    values: vec4<f32>,
};

@group(1) @binding(0) var<uniform> adjustment: DisplayAdjustment;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_tile, image_sampler, input.uv);
    let luminance = dot(sampled.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let safe_luminance = max(luminance, 0.000001);
    let mapped_luminance = pow(safe_luminance, adjustment.values.y) * exp2(adjustment.values.x);
    let adjusted = sampled.rgb * (mapped_luminance / safe_luminance);
    return vec4<f32>(adjusted, sampled.a);
}
