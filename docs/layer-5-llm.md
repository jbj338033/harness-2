# Layer 5: LLM Integration

## Overview

멀티 프로바이더 LLM 통합. 직접 추상화 (Provider trait).
프로바이더 고유 기능 (Anthropic 캐싱, OpenAI JSON mode 등)을 타입 안전하게 노출.

## Provider Trait

```rust
trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest, options: ChatOptions) -> Result<ChatStream>;
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;
}
```

### ChatRequest

도구 호출은 chat의 일부. 별도 메서드 불필요.

```rust
struct ChatRequest {
    system: Option<String>,
    messages: Vec<Message>,
    tools: Vec<ToolDef>,
}

struct Message {
    role: Role,
    content: Vec<ContentBlock>,
}

enum Role { User, Assistant, System }

enum ContentBlock {
    Text(String),
    Image { data: Vec<u8>, media_type: String },
    ToolCall { id: String, name: String, input: Value },
    ToolResult { id: String, output: String },
}
```

멀티모달 (이미지 등)은 ContentBlock variant로 처리. 별도 API 불필요.

### ChatOptions

프로바이더 고유 기능을 `Option<T>`로 flat하게 나열.
지원 안 하는 프로바이더는 무시.

```rust
struct ChatOptions {
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    top_p: Option<f32>,
    stop_sequences: Option<Vec<String>>,
    response_format: Option<ResponseFormat>,   // OpenAI JSON mode
    reasoning_effort: Option<ReasoningEffort>,  // OpenAI
    cache_control: Option<CacheControl>,        // Anthropic prompt caching
}
```

### ChatStream

tokio Stream. 이벤트 기반 스트리밍.

```rust
type ChatStream = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>;

enum StreamEvent {
    Delta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, chunk: String },
    Done { usage: Usage },
    Error(ProviderError),
}

struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    cache_read_tokens: Option<u32>,
    cache_creation_tokens: Option<u32>,
    cost: Option<f64>,
}
```

## Error Handling

```rust
enum ProviderError {
    RateLimit { retry_after: Option<Duration> },
    ContextTooLong { max: usize, actual: usize },
    ServerError { status: u16, message: String },
    AuthError,
    InvalidRequest(String),
    StreamInterrupted,
    Network(reqwest::Error),
}
```

- RateLimit → ProviderPool이 키 전환 + backoff
- ContextTooLong → 호출자가 truncate 결정
- ServerError → 최대 3회 exponential backoff
- AuthError → 키 무효, 다음 키 시도

## Provider Implementations

```rust
struct AnthropicProvider { /* POST /v1/messages */ }
struct OpenAiProvider { /* POST /v1/chat/completions */ }
struct GoogleProvider { /* POST /v1/models/{model}:generateContent */ }
struct OllamaProvider { /* POST /api/chat */ }
```

각 구현이 ChatRequest/ChatOptions를 자기 API 포맷으로 변환.
ChatStream으로 통일된 응답 반환.

## Key Balancing (ProviderPool)

키 밸런싱은 Provider trait 밖, 별도 ProviderPool이 처리.

```
ProviderPool
  ├─ provider-1 (key-1) ← 현재
  ├─ provider-2 (key-2)
  └─ provider-3 (key-3)

chat 요청
  → pool이 사용 가능한 키 선택 (rate limit 상태 기반)
  → Provider.chat 호출
  → 429 반환 → 다음 키로 전환 + backoff
  → 전부 소진 → 다른 프로바이더로 fallback (사용자 설정에 따라)
```

Rate limit 상태는 응답 헤더 (`x-ratelimit-remaining`, `retry-after`)에서 갱신.
메모리에서만 관리, 영속화하지 않음.

## Token Counting

Provider trait에 포함하지 않음.
응답의 `Usage`에서 사용량 수신.
사전 카운팅이 필요하면 로컬 토크나이저 (tiktoken 등)로 별도 처리.

## Embedding (향후)

현재 불필요. 벡터 검색 추가 시 별도 trait:

```rust
trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>>;
}
```

## Model Registry

바이너리에 내장된 모델 목록 (context_window, 가격 등) + Ollama는 런타임 API 조회.
`harness model add`로 커스텀 모델 추가 가능.
