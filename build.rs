// Compiles the WGSL shaders in shaders/ to SPIR-V at build time with naga,
// so no external shader compiler (glslc) is needed.
use std::{env, fs, path::Path};

fn main() {
    let out = env::var("OUT_DIR").unwrap();
    for name in ["world", "overlay"] {
        let src_path = format!("shaders/{name}.wgsl");
        println!("cargo:rerun-if-changed={src_path}");
        let src = fs::read_to_string(&src_path).expect("read shader");
        let module = naga::front::wgsl::parse_str(&src)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&src)));
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::PUSH_CONSTANT,
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let words = naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
            .expect("spv");
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        fs::write(Path::new(&out).join(format!("{name}.spv")), bytes).unwrap();
    }
}
