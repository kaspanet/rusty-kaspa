use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src/asm");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_arch == "x86_64" {
        let mut builder = cc::Build::new();
        builder.flag("-c");
        match target_os.as_str() {
            "macos" => builder.file("src/asm/keccakf1600_x86-64-osx.s"),
            "linux" => builder.file("src/asm/keccakf1600_x86-64-elf.s"),
            "windows" if target_env == "gnu" => builder.file("src/asm/keccakf1600_x86-64-mingw64.s"),
            "windows" if target_env == "msvc" => builder.file("src/asm/keccakf1600_x86-64-msvc.asm"),
            _ => unimplemented!("Unsupported OS"),
        };
        builder.compile("libkeccak.a");
    }
    Ok(())
}
