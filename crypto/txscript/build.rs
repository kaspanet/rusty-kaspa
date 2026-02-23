#[cfg(windows)]
fn main() {
    // WORKAROUND: Compile Windows stub for risc0-zkvm-platform's sys_alloc_aligned
    //
    // This build script compiles a C stub that provides the missing sys_alloc_aligned()
    // and sys_free_aligned() symbols required by risc0-zkvm-platform on Windows.
    //
    // The stub is only compiled on Windows platforms and links against the MSVC runtime
    // to provide aligned memory allocation functions. This allows the crate to build
    // successfully on Windows while risc0-zkvm-platform lacks native Windows support.
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
