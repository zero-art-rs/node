//! Crate for testing the node

#[cfg(test)]
#[cfg(feature = "integration_tests")]
pub(crate) mod integration_tests;

#[cfg(test)]
pub(crate) mod test_api;

#[cfg(test)]
pub(crate) mod user_test_model;

#[cfg(test)]
pub(crate) mod client_test_wrapper;

#[cfg(test)]
pub(crate) mod utils;

/// Backend url for testing.
#[cfg(test)]
const BACKEND_URL: &str = "http://localhost:8080";
/// Centrifugo url for testing.
#[cfg(test)]
const CENTRIFUGO_URL: &str = "http://localhost:8000";

/// used for tests, which can be repeated.
#[cfg(test)]
const TEST_REPEATS: usize = 4;
/// Default nonce used in tests.
#[cfg(test)]
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
/// Used to denote the size of the group for some tests.
#[cfg(test)]
const GROUP_SIZE: u64 = 10;
