use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
    let proto_kaspawalletd = "./proto/kaspawalletd.proto";
    let proto_protoserialization = "./proto/wallet.proto";

    println!("cargo:rerun-if-changed={}", proto_kaspawalletd);

    let proto_dir = PathBuf::from("./proto");
    tonic_build::configure().build_server(true).build_client(true).compile_protos(&[proto_kaspawalletd], &[&proto_dir])?;
    tonic_build::configure().compile_protos(&[proto_protoserialization], &[proto_dir])?;

    Ok(())
}
