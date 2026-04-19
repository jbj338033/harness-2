# Layer 6: Tools

## Overview

에이전트가 사용하는 도구. 네이티브 15종 + MCP 확장.

모든 도구는 같은 trait를 구현:

```rust
trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput>;
}
```

LLM 입장에서 네이티브 도구와 MCP 도구는 구분 없이 같은 도구 목록.

## Native Tools (15종)

### 파일

| 도구 | 설명 |
|------|------|
| **read** | 파일 읽기. hashline ID 포함 반환. `outline` 옵션으로 tree-sitter 구조 요약. `symbol` 옵션으로 특정 함수/클래스 추출. |
| **edit** | 파일 편집. hashline 기반 (기본) + exact string replacement (폴백). 입력 형태를 자동 감지. |
| **write** | 신규 파일 생성. 기존 파일은 edit 사용. |
| **glob** | 파일 패턴 검색. |
| **grep** | 내용 검색 (ripgrep 기반). |

### 셸

| 도구 | 설명 |
|------|------|
| **bash** | 셸 명령 실행. 루트 에이전트 = 허용적. 스폰된 워커 = 샌드박스 (worktree 제한). |

워커 샌드박스:
- Linux: `landlock` (pre_exec에서 적용, worktree + 빌드 캐시만 쓰기 허용)
- macOS: `sandbox-exec` (프로파일 기반 경로 제한)
- 파괴적 명령 패턴 차단 (rm -rf /, mkfs, dd 등)

### 에이전트

| 도구 | 설명 |
|------|------|
| **spawn** | 서브에이전트 생성 (fresh context, optional worktree) |
| **wait** | 서브에이전트 완료 대기 |
| **cancel** | 서브에이전트 중단 |

### 브라우저

| 도구 | 설명 |
|------|------|
| **browser** | CDP 직접 연결 (Rust, chromiumoxide). accessibility snapshot + refs로 토큰 93% 절감. |

동작:
- 기존 Chrome 있으면 → CDP 연결 (로그인/세션 유지)
- 없으면 → headless 실행 + 프로필 로드
- 상주 데몬이 CDP 클라이언트 → 별도 프로세스 불필요
- snapshot: `button "로그인" [ref=e1], textbox "이메일" [ref=e2]`
- 액션: navigate, click(ref), fill(ref, text), screenshot, evaluate(js), network, performance

### 화면

| 도구 | 설명 |
|------|------|
| **computer_use** | 스크린샷 + 마우스/키보드 제어 (xcap + enigo) |

액션:
- 캡처: screenshot, zoom(region)
- 클릭: left/right/middle/double/triple_click
- 드래그: drag, mouse_down, mouse_up
- 이동: mouse_move, scroll
- 입력: type, key, hold_key
- 제어: wait

토큰 최적화: 저해상도 전체 + zoom 고해상도 관심 영역. JPEG 압축 70-80.

### 웹

| 도구 | 설명 |
|------|------|
| **web_search** | 웹 검색 |
| **web_fetch** | HTTP 요청 + 콘텐츠 추출 |

### 코드

| 도구 | 설명 |
|------|------|
| **lsp** | Language Server Protocol 통합 |

기능: go-to-definition, find-references, rename (프로젝트 전체), diagnostics (타입 에러, 린트)
grep보다 정확 (의미 기반 심볼 검색).

## MCP 확장

`rmcp` 크레이트로 외부 MCP 서버 연결. stdio 트랜스포트.

```
harness config set mcp.servers.postgres "npx @anthropic/mcp-postgres"
→ 데몬이 MCP 서버 스폰 → 도구 자동 등록
```

네이티브 vs MCP 판단 기준:
- 매 세션 사용 + 지연시간 중요 → 네이티브
- 프로젝트별 다름 + 외부 의존 → MCP

## Hashline Edit 상세

파일을 읽을 때 각 줄에 content hash 부여:

```
1:a3|function hello() {
2:f1|  return "world";
3:0e|}
```

편집 시 hash로 위치 특정:

```json
{"anchors": [{"line": "2:f1", "content": "  return \"hello\";"}]}
```

exact string replacement 폴백:

```json
{"old_text": "return \"world\"", "new_text": "return \"hello\""}
```

입력 형태를 자동 감지. hashline이 기본, exact string도 받아줌.

이점:
- 기존 텍스트 재현 불필요 → 토큰 절약
- 해시 불일치 시 명확한 거부 → silent corruption 방지
- 15개 LLM에서 5-14%p 성능 향상 (The Harness Problem, 2026)
- 출력 토큰 61% 감소 (재시도 루프 제거)

## Approval Gate

민감한 작업(파괴적 shell 명령, 외부 네트워크 등)은 `ApprovalGate` 를 통과해야 실행된다.

### 흐름

1. 도구가 위험한 패턴을 감지 → `broadcaster.publish(approval.request { id, description, pattern })` notification 발행.
2. TUI 가 `pending_approval` 을 세팅하고 `y`/`n`/`a`/`g` 키로 결정 입력.
3. TUI → `v1.approval.respond { request_id, decision, pattern?, session_id? }` RPC 전송.
4. daemon 의 `approval_respond` 핸들러:
   - decision 이 `allow_session` / `allow_global` 이면 `approvals` 테이블에 persist.
   - `Daemon.pending_approvals[request_id]` 에 대기 중인 tool 이 있으면 `oneshot::Sender` 로 decision 전달.
5. 도구는 수신한 decision 에 따라 실행을 재개하거나 중단.

### Pattern 의미

- `pattern` 은 현재 **literal substring** 매칭 (정규식 아님). `approvals::matches` 는 `command.contains(pattern)` 으로 검사한다. 필요해지면 regex / glob 로 확장.

### 상태

- **response 경로 (TUI → daemon)**: 구현 완료 (`v1.approval.respond`).
- **request 경로 (tool → TUI)**: 구조(`pending_approvals` 맵, notification 형식)는 있지만 실제 도구가 이걸 쏘는 경로는 미구현. 후속 phase 에서 shell tool 부터 wiring.

## Principle Enforcement (원리 5)

| 구분 | 시행 |
|------|------|
| 루트 에이전트 | 허용적 (사용자가 보고 있음) |
| 워커 에이전트 | 샌드박스 (worktree + 빌드 캐시만 쓰기 허용) |
| 파괴적 명령 | 정적 패턴 차단 (rm -rf /, mkfs 등) |
| 파일 범위 | plan.md에 명시된 파일만 edit 허용 (워커) |
| 외부 시스템 | 명시적 허가 없이 불가 |

## Rust Crates

```
chromiumoxide    — CDP 브라우저 제어
xcap             — 스크린 캡처
enigo            — 마우스/키보드 제어
rmcp             — MCP 클라이언트
tree-sitter      — AST 파싱 (read outline/symbol)
grep-searcher    — ripgrep 라이브러리
landlock         — Linux 샌드박스
```
