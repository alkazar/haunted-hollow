// 2D HUD shader: crosshairs, pixel font text and panels, alpha blended.
struct PC {
    viewproj: mat4x4<f32>,
    cam: vec4<f32>,
    fog: vec4<f32>,
    params: vec4<f32>,
};
var<push_constant> pc: PC;

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = pc.viewproj * vec4<f32>(v.pos.xy, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas, samp, i.uv);
    return tex * i.color;
}
