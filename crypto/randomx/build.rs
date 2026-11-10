use std::env;

fn main() {
    println!("cargo:rerun-if-changed=RandomX");

    // The hash output is identical for every ARCH setting; ARCH only tunes compiler flags. `default` (no -march)
    // keeps the binary runnable on older CPUs than the build machine. Always build Release: a Debug RandomX is
    // many times slower and would make debug builds of the node unusable.
    let dst = cmake::Config::new("RandomX").define("ARCH", "default").profile("Release").build_target("randomx").build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-search=native={}/build", dst.display());
    println!("cargo:rustc-link-search=native={}/build/Release", dst.display());
    println!("cargo:rustc-link-lib=static=randomx");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    match target_os.as_str() {
        "openbsd" | "bitrig" | "netbsd" | "macos" | "ios" | "freebsd" => println!("cargo:rustc-link-lib=dylib=c++"),
        "windows" => {
            println!("cargo:rustc-link-lib=advapi32");
            if target_env == "gnu" {
                println!("cargo:rustc-link-lib=dylib=stdc++");
            }
        }
        _ => println!("cargo:rustc-link-lib=dylib=stdc++"),
    }
}
