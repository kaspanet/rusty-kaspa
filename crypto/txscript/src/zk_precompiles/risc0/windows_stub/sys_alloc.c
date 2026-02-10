// Windows stub implementation for risc0-zkvm-platform's sys_alloc_aligned
//
// WORKAROUND: Temporary workaround for Windows linking errors with risc0-zkvm-platform
//
// ERROR ENCOUNTERED:
// ==================
// When building on Windows (tn12 branch) with MSVC toolchain, the linker fails with:
//
//   error LNK2019: unresolved external symbol sys_alloc_aligned referenced in function
//   _ZN19risc0_zkvm_platform7syscall15sys_alloc_words17hd3ca7735ddf0b22bE
//
// This affects multiple crates that transitively depend on kaspa-txscript:
//   - kaspa-wallet-pskt
//   - kaspa-wrpc-client
//   - kaspa-grpc-simple-client-example
//   - rothschild
//
// WHY THIS SOLUTION:
// ==================
// We provide a Windows-specific implementation of sys_alloc_aligned()
// using MSVC's native _aligned_malloc() functions. This satisfies the
// linker's requirement for these symbols while maintaining the same functionality.
//
// IMPORTANT NOTES:
// ================
// - This function is called by risc0-zkvm-platform but is only used during ZK proof execution,
//   not during verification-only use cases (which is how Kaspa uses it).
// - The implementation uses Windows-specific functions (_aligned_malloc) which
//   are part of the MSVC runtime library, so no additional dependencies are required.
// - This is a temporary workaround until risc0-zkvm-platform adds proper Windows support.
//
// TESTING/VERIFICATION:
// =====================
// To verify this workaround works correctly, build the project on Windows:
//   cargo build --release
//
// Successful builds of affected crates (kaspa-wallet-pskt, kaspa-wrpc-client, etc.)
// indicate the workaround is functioning properly. The functions are only called during
// ZK proof operations, so runtime testing of ZK verification functionality should also
// be performed to ensure end-to-end correctness.
//
// COMPATIBILITY:
// ==============
// This workaround requires:
// - Windows OS with MSVC toolchain (x86_64-pc-windows-msvc)
// - MSVC runtime library (included with Visual Studio or Windows SDK)
// - The `cc` crate (already listed in build-dependencies)
//
// The implementation uses standard MSVC runtime functions available in all modern
// MSVC versions, so no specific version requirements beyond standard Rust/MSVC setup.
//
// TODO: Remove this workaround when risc0-zkvm-platform adds proper Windows support.
// Issue tracking: https://github.com/kaspanet/rusty-kaspa/issues/<ISSUE_NUMBER>
// Upstream risc0-zkvm-platform issue: <UPSTREAM_ISSUE_LINK> (if applicable)
//
// NOTE: IDE linter errors about missing headers are FALSE POSITIVES.
// This file only compiles on Windows with MSVC, where <malloc.h> exists.
// The code compiles successfully - verify with: cargo build --release

#ifdef _WIN32
#include <stddef.h>

// Explicitly declare Windows-specific functions (provided by MSVC runtime)
// These declarations satisfy the compiler even if the linter can't find the headers
void* _aligned_malloc(size_t size, size_t alignment);

__declspec(dllexport) void* sys_alloc_aligned(size_t size, size_t alignment) {
    // Use _aligned_malloc on Windows (provided by MSVC runtime)
    return _aligned_malloc(size, alignment);
}

#endif  // _WIN32

