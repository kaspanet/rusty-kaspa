fn main() {
    let protowire_main_file = "./proto/messages.proto";

    tonic_build::configure()
        .build_server(true)
        .build_client(true)

        // In case we want protowire.rs to be explicitly integrated in the crate code,
        // uncomment this line and reflect the change in src/lib.rs
        //.out_dir("./src")

        .compile(&[protowire_main_file], &["./proto/", "."])
        .unwrap_or_else(|e| panic!("protobuf compile error: {e}"));
}
