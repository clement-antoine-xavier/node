use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Prefer a `PROTOC` already present in the environment; otherwise fall back to
    // the vendored binary so builds are hermetic on machines and CI without protoc.
    if env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        // SAFETY: `build.rs` runs as a single-threaded process before anything
        // else reads the environment, so there is no data race here.
        unsafe { env::set_var("PROTOC", protoc) };
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let descriptor_path = out_dir.join("node_descriptor.bin");

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(descriptor_path)
        .compile_protos(&["proto/node.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto/node.proto");
    Ok(())
}
