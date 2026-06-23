#[cfg(windows)]
fn main() {
    // WORKAROUND: Compile Windows stub for risc0-zkvm-platform's sys_alloc_aligned.
    //
    // Verifier-only RISC0 builds may still declare sys_alloc_aligned as an external
    // symbol on Windows. The optional `risc0-platform-exports-syscalls` feature is
    // enabled by kaspa-txscript-zk-sdk, where risc0-zkvm enables
    // risc0-zkvm-platform/export-syscalls and exports sys_alloc_aligned itself.
    // In that graph, compiling this shim would create duplicate MSVC symbols.
    if std::env::var_os("CARGO_FEATURE_RISC0_PLATFORM_EXPORTS_SYSCALLS").is_some() {
        return;
    }

    // This build script compiles a C stub that provides the missing sys_alloc_aligned()
    // symbol required by risc0-zkvm-platform on Windows.
    //
    // The stub is only compiled on Windows platforms and links against the MSVC runtime
    // to provide aligned memory allocation functions. This allows the crate to build
    // successfully on Windows when the RISC0 platform does not export the host symbol.
    //
    // See src/zk_precompiles/risc0/windows_stub/sys_alloc.c for implementation details.
    //
    // The compiled stub is automatically linked into the final binary, providing the
    // required symbols at link time. This approach was chosen over conditional compilation
    // because risc0-zkvm-platform is a required dependency (transitively via risc0-* crates)
    // and cannot be disabled on Windows without breaking the build entirely.
    //
    // TODO: Remove when risc0-zkvm-platform adds proper Windows support.
    cc::Build::new().file("src/zk_precompiles/risc0/windows_stub/sys_alloc.c").compile("risc0_zkvm_platform_stub");
}

#[cfg(not(windows))]
fn main() {}
