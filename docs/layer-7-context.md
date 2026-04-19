# Layer 7: Context Management

## Overview

프롬프트 엔지니어링이 아니라 **컨텍스트 엔지니어링**. 시스템 프롬프트 텍스트보다 모델이 보는 전체 정보 환경의 설계가 중요.

## Core Principles

```
1. 정적 주입 최소화, 동적 탐색 최대화
   에이전트가 도구로 필요할 때 가져오게. 사전 로드는 최소.

2. 도구 설계 > 프롬프트 텍스트
   hashline처럼 도구 포맷이 성능을 10배 바꿀 수 있음.

3. 검증 = 외부 도구
   self-reflection 프롬프트 (LLM이 스스로 검토) 대신
   test/lint 실행 결과로 검증.

4. 메모리 예산 = 컨텍스트의 10% 이내
   과다 주입 시 "lost in the middle".

5. 긍정 지시 우선, 부정은 보조
   "Do Y" > "Don't X". 부정은 안전 관련만.

6. Extended thinking = 계획 단계만
   실행(편집, 명령)에선 낭비. 디버깅은 예외.

7. 워커에게 최소 컨텍스트
   역할에 필요한 것만. 전체 상태 불필요.
```

## Prompt Assembly Pipeline

순서: 안정적인 것 앞에, 변동적인 것 뒤에 (KV cache 최적화).

```
[1] 기본 지시 + 5원리 (불변)              ← cache breakpoint 1
[2] 도구 정의 (불변)                      ← cache breakpoint 2
[3] 프로젝트 메모리 (세션 내 불변)         ← cache breakpoint 3
[4] 역할 + 태스크 (에이전트 내 불변)       ← cache breakpoint 4
[5] 대화 히스토리 (append only)           (캐시 안 됨)
```

Anthropic 최대 4개 breakpoint. 각 지점까지의 prefix가 같으면 90% 할인.

멀티 에이전트 캐시 공유:
```
에이전트 A: [1][2][3] → 공유 → [4-A][5-A]
에이전트 B: [1][2][3] → 공유 → [4-B][5-B]
```

## System Prompt Structure

XML 태그 기반 (Anthropic 권장, Claude 학습 데이터 최적화).

```xml
<role>
  당신은 하네스 코딩 에이전트입니다.
</role>

<principles>
  <principle name="ephemeral">
    상태를 컨텍스트에 의존하지 마라. 파일시스템에 기록하라.
    다음 에이전트가 파일만 보고 이어갈 수 있어야 한다.
  </principle>
  
  <principle name="plan-as-contract">
    계획에 없는 작업은 하지 마라.
    결정된 것을 번복하지 마라.
    계획 밖 변경은 반드시 기록하라.
  </principle>
  
  <principle name="evidence">
    증거 없이 완료를 주장하지 마라.
    "should work", "probably fine" = FAILURE.
    테스트 통과, 빌드 성공만이 증거다.
  </principle>
  
  <principle name="quality-gate">
    test/lint 통과 전에 NEVER commit.
    실패한 코드는 쌓지 마라.
  </principle>
  
  <principle name="scope">
    계획에 명시된 파일만 수정하라.
    파괴적 명령을 실행하지 마라.
    불확실하면 질문하라.
  </principle>
</principles>

<workflow>
  1. 이해: 태스크 파악, 필요시 질문
  2. 분석: 코드베이스 읽기, 패턴 파악
  3. 계획: plan.md 작성 (파일, 함수, 패턴, 테스트 기준)
  4. 실행: 워커 spawn, fresh context, worktree 격리
  5. 검증: test, lint, 결과 리뷰
  6. 반복: 실패 시 Ralph Loop으로 재시도
  
  단순한 태스크는 1-2-3 건너뛰고 바로 실행.
</workflow>

<hard-gates>
  <gate>Edit 전에 반드시 Read 선행</gate>
  <gate>커밋 전에 반드시 test/lint 통과 확인</gate>
  <gate>plan 없이 복잡한 태스크 실행 금지</gate>
</hard-gates>
```

## Memory Injection

```
항상 주입 (10% 토큰 예산):
  global memory    — 사용자 선호, 전역 규칙
  project memory   — 기술 스택, 컨벤션

주입 안 함 (에이전트가 도구로 탐색):
  파일 내용        → read
  git 히스토리     → bash
  이슈 목록        → web_fetch
  코드 구조        → read(outline)
```

### Memory Selection

100개+ 메모리 축적 시:

```
1순위: FTS5 키워드 매칭 (빠름, 쉬움)
2순위: recency decay (최근 것에 가중치)
3순위: 매 세션 주입은 전체의 10% 이내
      초과 시 relevance 낮은 것부터 제외

FTS5 한계 (의미적 유사성 불가):
  "auth" ≠ "authentication" ≠ "login"
→ 메모리 100개+ 되면 임베딩 검색 추가 검토
```

## Skill Injection

Agent Skills (layer 9) 는 progressive disclosure 로 시스템 프롬프트에 들어간다.

**Tier 1 — catalog** 는 `memory` 블록 뒤, `role + task` 블록 앞에 `<available_skills>` XML 섹션으로 주입된다. 각 항목은 `{name, description, location}` 만 — body 는 로드하지 않음. ~100 토큰/skill.

```
[1] 기본 지시 + 5원리              cache BP1
[2] 도구 정의                     cache BP2
[3] 프로젝트 메모리                (3a)
    available_skills 카탈로그     (3b) ← 신규
                                  cache BP3
[4] 역할 + 태스크                  cache BP4
[5] 대화 히스토리 (append only)
```

Catalog 섹션에는 짧은 behavioral instruction 이 붙는다: "description 매칭 시 `activate_skill` tool 호출".

**Tier 2 — body** 는 `activate_skill` 도구가 호출될 때 대화 스레드에 직접 주입된다 (system role, `kind = 'skill_attachment'`). Catalog 와 달리 이 쪽은 메시지 영역에 들어가 캐시되지 않는다. 대신 V3 스키마의 `messages.kind` 컬럼 덕에 compaction 로직이 선택적으로 보호할 수 있다.

**Tier 3 — resources** (`scripts/`, `references/`, `assets/`) 는 body 가 참조할 때만 `read` 도구로 로드. eager 로드 금지.

자세한 것은 `docs/layer-9-skills.md`.

## Tool Description Design

도구 설명에 "언제 쓰고 언제 쓰지 말 것"을 명시:

```
좋은 예:
  name: "grep"
  description: "파일 내용 검색. ripgrep 기반.
    USE: 코드에서 특정 문자열/패턴 찾기
    DO NOT USE: bash에서 grep/rg 직접 실행 금지. 이 도구 사용."

나쁜 예:
  name: "grep"
  description: "Search file contents"
```

## Worker Context

워커가 받는 것:

```
[1] 기본 지시 + 5원리 (루트와 공유, 캐시 히트)
[2] 도구 정의 (루트와 공유, 캐시 히트)
[3] 프로젝트 메모리 (루트와 공유, 캐시 히트)
[4] 워커 전용:
    - 역할 프롬프트 (coder/reviewer/tester)
    - 태스크 (부모가 plan.md에서 해당 부분 전달)
    - worktree 경로
[5] 없음 (fresh context, 대화 히스토리 0)
```

부족하면 도구로 탐색 (read, grep, lsp).

## ChatOptions 구조

프로바이더별 고유 기능을 확장 가능하게 처리:

```rust
pub struct ChatOptions {
    // 공통
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
    
    // 프로바이더별 namespaced (enum 아님)
    pub provider: ProviderOptions,
}

#[derive(Default)]
pub struct ProviderOptions {
    pub anthropic: Option<AnthropicOptions>,
    pub openai:    Option<OpenAiOptions>,
    pub google:    Option<GoogleOptions>,
    pub extra:     serde_json::Map<String, Value>,  // escape hatch
}

pub struct AnthropicOptions {
    pub thinking: Option<ThinkingConfig>,
    pub cache_control: Option<CacheControl>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,  // 새 기능 passthrough
}

pub struct OpenAiOptions {
    pub reasoning_effort: Option<ReasoningEffort>,
    pub response_format: Option<ResponseFormat>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
```

이 패턴 채택 이유:
- enum이 아니므로 여러 프로바이더 옵션 동시 설정 가능 (라우터 친화)
- 프로바이더별 타입 안전성
- `#[serde(flatten)] extra`로 새 기능 즉시 사용 가능 (타입 추가는 나중에)
- Vercel AI SDK + rig 패턴 (프로덕션 검증)

## KV Cache Breakpoint 배치

```
cache_control 붙는 지점 (최대 4개):
  BP1: 기본 지시 끝
  BP2: 도구 정의 끝
  BP3: 프로젝트 메모리 끝
  BP4: 역할 + 태스크 끝
```

TTL 5분, 사용 시마다 갱신. 활발한 세션에서는 사실상 영구 유지.
