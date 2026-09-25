use std::io::Result;

fn main() -> Result<()> {
    let proto_kaspawalletd = "./proto/kaspawalletd.proto";
    let proto_protoserialization = "./proto/wallet.proto";
    let proto_dir = "./proto";

    println!("cargo:rerun-if-changed={}", proto_kaspawalletd);

    tonic_prost_build::configure().build_server(true).build_client(true).compile_protos(&[proto_kaspawalletd], &[proto_dir])?;
    tonic_prost_build::configure().compile_protos(&[proto_protoserialization], &[proto_dir])?;

    Ok(())
}
