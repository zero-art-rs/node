//! Crate for testing the node

mod test_api;
mod user_test_model;

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
