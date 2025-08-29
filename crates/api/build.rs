fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")
        .map_err(|e| format!("OUT_DIR environment variable not set: {}", e))?;

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(out_dir)
        .compile_protos(
            &["proto/v1/simulation.proto"], // Files to compile
            &["proto"],                     // Include paths
        )?;

    println!("cargo:rerun-if-changed=proto/v1/simulation.proto");

    Ok(())
}
