// 3D voxel world shader: textured blocks with baked (Minecraft-style) light,
// a warm lantern carried by the ride cart, and distance fog.
struct PC {
    viewproj: mat4x4<f32>,
    cam: vec4<f32>,     // xyz camera position, w = time
    fog: vec4<f32>,     // rgb fog colour
    params: vec4<f32>,  // x fog start, y fog end, z lantern strength, w lantern radius
};
var<push_constant> pc: PC;

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,   // rgb = baked light * tint, a = lantern response (2+ = sky, unfogged)
};
struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = pc.viewproj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let tex = textureSample(atlas, samp, i.uv);
    if (tex.a < 0.5) {
        discard;
    }
    if (i.color.a > 1.5) {
        return vec4<f32>(tex.rgb * i.color.rgb, 1.0);
    }
    let d = distance(i.world, pc.cam.xyz);
    let k = clamp(1.0 - d / pc.params.w, 0.0, 1.0);
    let lamp = pc.params.z * k * k * i.color.a;
    var rgb = tex.rgb * (i.color.rgb + vec3<f32>(1.0, 0.78, 0.5) * lamp);
    let f = smoothstep(pc.params.x, pc.params.y, d);
    rgb = mix(rgb, pc.fog.rgb, f);
    return vec4<f32>(rgb, 1.0);
}

// Soft alpha-blended mist puffs. `uv` holds the quad's own 0..1 coordinates (not the
// atlas) so the falloff is smooth; colour.a is the puff's opacity.
@fragment
fn fs_mist(i: VOut) -> @location(0) vec4<f32> {
    let q = i.uv * 2.0 - 1.0;
    var a = clamp(1.0 - dot(q, q), 0.0, 1.0);
    a = a * a;
    // Thin out right in front of the camera so riding through a puff doesn't pop.
    let d = distance(i.world, pc.cam.xyz);
    a = a * smoothstep(1.0, 6.0, d) * i.color.a;
    let rgb = mix(i.color.rgb, pc.fog.rgb, smoothstep(pc.params.x, pc.params.y, d));
    return vec4<f32>(rgb, a);
}
