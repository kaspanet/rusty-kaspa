//! Test utilities for kaspa_p2p_lib, only available with the "test-utils" feature.
#![allow(dead_code)]

#[cfg(feature = "test-utils")]
pub use crate::core::hub::HubTestExt;
#[cfg(feature = "test-utils")]
pub use crate::core::router::RouterTestExt;
