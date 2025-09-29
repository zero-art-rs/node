//! Crate for testing the node

#[cfg(feature = "integration_tests")]
pub(crate) mod integration_tests;
pub(crate) mod test_api;
pub(crate) mod user_test_model;
pub(crate) mod utils;

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";

// used for tests, which can be repeated
const TEST_REPEATS: usize = 4;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
// Used to denote the size of the group for some tests
const GROUP_SIZE: u64 = 10;