fn main() {
    let iface_files = &["messages.proto", "p2p.proto", "rpc.proto"];
    let dirs = &["./proto"];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(iface_files, dirs)
        .unwrap_or_else(|e| panic!("protobuf compilation failed, error: {e}"));
    // recompile protobufs only if any of the proto files changes.
    for file in iface_files {
        println!("cargo:rerun-if-changed={file}");
    }
}
