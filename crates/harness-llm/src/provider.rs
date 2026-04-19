use crate::stream::BoxStream;
use async_trait::async_trait;
use harness_llm_types::{ChatOptions, ChatRequest, ProviderError};

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;

    fn family(&self) -> &str;

    async fn list_models(&self) -> Result<Vec<String>, ProviderError>;

    async fn chat(
        &self,
        model: &str,
        request: ChatRequest,
        options: ChatOptions,
    ) -> Result<BoxStream, ProviderError>;
}
