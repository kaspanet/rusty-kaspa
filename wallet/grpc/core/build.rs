use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
    let proto_file = "./proto/kaspawalletd.proto";
    let proto_dir = PathBuf::from("./proto");
    tonic_build::configure().build_server(true).build_client(true).compile_protos(&[proto_file], &[proto_dir])?;
    println!("cargo:rerun-if-changed={}", proto_file);
    Ok(())
}
