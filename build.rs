// Compiles the WGSL shaders in shaders/ to SPIR-V at build time with naga,
// so no external shader compiler (glslc) is needed, and works out the version string.
use std::{env, fs, path::Path, process::Command};

fn main() {
    set_version();
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

/// Sets `HAUNTED_VERSION` for the game: `$HAUNTED_VERSION` if given (the release workflow
/// passes the git tag), else `git describe --tags` (e.g. "v1.0.0" or "v1.0.0-3-gabc1234"),
/// else the version in Cargo.toml.
fn set_version() {
    println!("cargo:rerun-if-env-changed=HAUNTED_VERSION");
    // New commits and tags change these, so the version stays current.
    for path in [".git/HEAD", ".git/refs", ".git/packed-refs"] {
        if Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let version = env::var("HAUNTED_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let out = Command::new("git").args(["describe", "--tags", "--always"]).output().ok()?;
            let v = String::from_utf8(out.stdout).ok()?.trim().to_string();
            (out.status.success() && !v.is_empty()).then_some(v)
        })
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=HAUNTED_VERSION={version}");
}
