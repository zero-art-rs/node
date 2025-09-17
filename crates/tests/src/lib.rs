//! Crate for testing the node

pub(crate) mod test_api;
pub(crate) mod user_test_model;

#[cfg(feature = "integration_tests")]
pub(crate) mod integration_tests;
#[cfg(feature = "integration_tests")]
pub(crate) mod user_integration_test_model;
pub(crate) mod utils;

pub(crate) fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    /// template
    pub fn add(left: u64, right: u64) -> u64 {
        left + right
    }

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
