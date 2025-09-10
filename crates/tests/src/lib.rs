//! Crate for testing the node

pub(crate) mod integration_tests;
pub(crate) mod test_api;
pub(crate) mod user_test_model;

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
