//! Unit tests for locating the runtime shim object the build script compiles.

use super::*;

/// The build script produced the object, so anything linking against it has a file to link.
#[test]
fn the_shim_object_exists() {
    assert!(
        shim_object().is_file(),
        "expected the build script to have compiled {SHIM_OBJECT}"
    );
}
