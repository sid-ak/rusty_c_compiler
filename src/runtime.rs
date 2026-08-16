//! The runtime every compiled program links against.
//!
//! Three fixed-arity output functions, written in C and compiled once by the build script. What
//! they are and why they are not `printf` is in `docs/decisions/0006-fixed-arity-runtime-shim.md`;
//! the source is `runtime/shim.c`.

use std::path::Path;

/// The compiled shim object, built by the build script into Cargo's output directory.
///
/// One object per build, linked by the driver and by both test harnesses, so every binary in a
/// differential comparison contains the identical runtime.
pub const SHIM_OBJECT: &str = env!("RUSTYCC_SHIM_OBJECT");

/// The path to [`SHIM_OBJECT`].
pub fn shim_object() -> &'static Path {
    Path::new(SHIM_OBJECT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The build script produced the object, so anything linking against it has a file to link.
    #[test]
    fn the_shim_object_exists() {
        assert!(
            shim_object().is_file(),
            "expected the build script to have compiled {SHIM_OBJECT}"
        );
    }
}
