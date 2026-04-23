// IMPLEMENTS: D-218, D-220, D-221, D-222, D-223, D-224, D-225, D-226, D-227, D-228, D-229, D-230
pub mod selection;
pub mod store;

pub use selection::{SelectionParams, render_xml, select_for_turn};
pub use store::{MemoryRecord, NewMemory, Scope, delete, insert, list_global, list_project};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("storage: {0}")]
    Storage(#[from] harness_storage::StorageError),

    #[error("invalid input: {0}")]
    Input(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;

#[must_use]
pub fn approx_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approx_tokens_is_roughly_right() {
        assert_eq!(approx_tokens(""), 0);
        assert!(approx_tokens("hello world").abs_diff(3) <= 1);
        let long = "a".repeat(400);
        let count = approx_tokens(&long);
        assert!((90..=110).contains(&count), "got {count}");
    }
}
