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

// fog.w > 1.5 means the target is an sRGB image (the VR headset's swapchain), which encodes
// what we write: so decode our sRGB-authored colour to linear first, and it comes out as-is.
fn to_target(c: vec3<f32>) -> vec3<f32> {
    if (pc.fog.w > 1.5) {
        let x = max(c, vec3<f32>(0.0));
        return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
    }
    return c;
}

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
    let c = textureSample(atlas, samp, i.uv) * i.color;
    return vec4<f32>(to_target(c.rgb), c.a);
}
