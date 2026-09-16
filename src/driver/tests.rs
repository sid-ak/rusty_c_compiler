//! Unit tests for the driver.

use super::*;

/// A toolchain that is present reports itself as present.
#[test]
fn preflight_finds_the_toolchain() {
    assert!(
        preflight().is_ok(),
        "clang should be on the path for this suite"
    );
}
