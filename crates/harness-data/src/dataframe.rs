// IMPLEMENTS: D-264
//! Dataframe runtime selector. Default is **pandas** because Posit's
//! DS-1000 evaluation showed LLM-generated pandas code is materially
//! more reliable than Polars at the same task today. Polars is
//! opt-in for performance-critical pipelines that explicitly request
//! it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataframeRuntime {
    #[default]
    Pandas,
    Polars,
}

#[must_use]
pub fn default_runtime() -> DataframeRuntime {
    DataframeRuntime::Pandas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_pandas() {
        assert_eq!(DataframeRuntime::default(), DataframeRuntime::Pandas);
        assert_eq!(default_runtime(), DataframeRuntime::Pandas);
    }
}
