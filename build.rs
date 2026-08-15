//! Compile the runtime shim once, and tell the crate where the object landed.
//!
//! Building it here rather than on demand means there is exactly one `shim.o` per build, and the
//! driver, the golden-program tests, and the differential harness all link that same object — which
//! is what makes a comparison against `clang` apples to apples.
//!
//! `-Werror` is deliberate: the shim is small enough that a warning in it is a defect, and a build
//! failure is a louder way to say so than a line of output nobody reads.

// The compiler's no-panic rule is about user input reaching a pass, which has a diagnostic channel
// to report through. A build script has none: failing loudly is the only way it can report at all.
#![allow(clippy::expect_used, clippy::panic)]

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Where the shim's source lives, relative to the crate root.
const SHIM_SOURCE: &str = "runtime/shim.c";

/// Compile `runtime/shim.c` into `$OUT_DIR/shim.o` and export its path as `MYCC_SHIM_OBJECT`.
fn main() {
    println!("cargo::rerun-if-changed={SHIM_SOURCE}");
    println!("cargo::rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo always sets OUT_DIR"));
    let object = out_dir.join("shim.o");

    let output = Command::new("clang")
        .args(["-std=c99", "-O0", "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(SHIM_SOURCE)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "could not run clang to build {SHIM_SOURCE}: {error}\n\
                 Xcode Command Line Tools are required; run `xcode-select --install`."
            )
        });

    assert!(
        output.status.success(),
        "clang failed to build {SHIM_SOURCE}:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    println!("cargo::rustc-env=MYCC_SHIM_OBJECT={}", object.display());
}
