fn main() {
    let proto_file1 = "./proto/messages.proto";
    let proto_file2 = "./proto/kaspadrpc.proto";

    println!("cargo:rerun-if-changed={}, {}", proto_file1, proto_file2);

    tonic_build::configure()
        .build_server(true)
        .compile(&[proto_file1], &["./proto/", "."])
        .unwrap_or_else(|e| panic!("protobuf compile error: {}", e));
}
