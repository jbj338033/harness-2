use futures::Stream;
use harness_llm_types::{ProviderError, StreamEvent};
use std::pin::Pin;

pub type BoxStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ProviderError>> + Send>>;
