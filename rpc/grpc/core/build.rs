fn main() {
    let protowire_files = &["./proto/messages.proto", "./proto/rpc.proto"];
    let dirs = &["./proto"];

    tonic_build::configure()
        .build_server(true)
        .build_client(true)

        // In case we want protowire.rs to be explicitly integrated in the crate code,
        // uncomment this line and reflect the change in src/lib.rs
        //.out_dir("./src")

        .compile_protos(&protowire_files[0..1], dirs)
        .unwrap_or_else(|e| panic!("protobuf compile error: {e}"));

    // recompile protobufs only if any of the proto files changes.
    for file in protowire_files {
        println!("cargo:rerun-if-changed={file}");
    }
}
