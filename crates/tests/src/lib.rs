//! Crate for testing the node

// #[cfg(feature = "integration_tests")]
// #[cfg(feature = "integration_tests")]
pub(crate) mod flow_tests;
pub(crate) mod user_test_model;
pub(crate) mod utils;
mod sender;

pub(crate) fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .try_init();
}
