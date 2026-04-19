# Layer 8: TUI

## Overview

경량 inline TUI. 풀스크린으로 터미널을 점유하지 않고, 일반 출력처럼 scroll-back 보존.

## Rendering

```
라이브러리: ratatui v0.29+
모드:      inline (scrolling-regions feature)
근거:     Claude Code(Ink)와 동일 접근. 풀스크린은 scroll-back 손실.
```

ratatui v0.29의 scrolling-regions로 풀스크린 아닌 inline 렌더링 가능. 터미널의 기존 출력이 그대로 보존되고, 대화와 상태만 동적으로 갱신.

## Streaming

```
배치 flush:     16-33ms 간격 (토큰마다 redraw 하면 느림)
프레임 캡:      60fps
syntax highlight: 줄 단위. 스트리밍 중에는 plain, 줄 완료 시 highlight.
코드 블록:      ``` 닫힘 시 full re-highlight.
```

syntect 상태 캐싱으로 증분 하이라이트. tree-sitter는 옵션 (ratatui-code-editor 참고).

## Visual Language

### 글리프 (BMP 범위만)

```
●  진행 중
○  대기
✓  성공
✗  실패
▸▾ 펼침/접힘
→  도구 호출
```

Nerd Font 의존 금지. ASCII fallback 제공:

```
[*] 진행 중
[ ] 대기
[v] 성공
[x] 실패
> 펼침
- 도구 호출
```

### 색상

```
NO_COLOR 환경변수 존중 (설정 시 색 비활성화)
색 + 글리프 + 텍스트 병용 (색맹 접근성)
diff: +/- 문자 + 색 (색 단독 금지)
```

## Layout

기본 화면:

```
  (터미널의 기존 출력 그대로 보존)
  
  > 이 컴포넌트 리팩터링해줘
  
  ● src/components/Button.tsx 분석 중
  ● 3개 개선 포인트 발견
  
  Button 컴포넌트를 다음과 같이 리팩터링하겠습니다:
  
  1. Props 타입을 인터페이스로 분리
  2. 스타일을 className prop으로 외부화
  3. memo로 감싸 리렌더링 최적화
  
  → editing src/components/Button.tsx
  
  @@ -12,5 +12,8 @@
    export interface ButtonProps {
  -   variant: string;
  +   variant: 'primary' | 'secondary';
  +   className?: string;
  
  → running npm test
  ✓ 12 passed
  
  리팩터링 완료. 테스트 전부 통과했습니다.
  
  > _
  ────── tokens: 12.4k │ $0.03 │ 14s ──────
```

## Status Bar

```
솔로:
──── tokens: 12.4k │ $0.03 │ 14s ────

멀티 에이전트:
─ agents: ⏳2 ✓1 │ tokens: 45.2k │ $0.12 │ 2m 18s ─

실패 포함:
─ agents: ⏳1 ✓1 ✗1 │ ... ─
```

## Keybindings

```
입력:
  Enter         전송
  Shift+Enter   줄바꿈 (멀티라인)
  Ctrl+C        현재 스트리밍 중단
  Esc           입력 취소 / 모드 벗어남
  Tab           슬래시 자동완성 (공통 접두사 확장, 단일 후보면 완성)

탐색:
  Ctrl+L        화면 지우기 (대화는 보존)
  Ctrl+P/N      입력 히스토리 (이전/다음)
  Ctrl+R        히스토리 검색 (예정)
  PgUp/PgDn     대화 스크롤 (예정)
  Ctrl+O        선택 메시지 복사 (Codex 패턴, 예정)

오버레이:
  Ctrl+A        에이전트 트리
  Ctrl+H        세션 히스토리
  Ctrl+,        설정
  Ctrl+/        도움말

종료:
  Ctrl+D        세션 종료
```

슬래시 명령과 단축키는 겹치지 않음 (슬래시는 discoverable, 단축키는 빈번한 동작).

## CLI flags

Claude Code · opencode 와 동일:

```
harness                   새 세션 (기본)
harness --continue   -c   현재 cwd 의 가장 최근 세션 resume
harness --resume [id] -r  세션 id 지정 resume. id 없으면 picker overlay
```

## Slash Commands

단일 소스는 `crates/harness-tui/src/commands.rs::COMMANDS` — `/help` 와 자동완성이 여기서 파생된다.

```
/clear             새 세션 시작 (대화 지우기 + session.create)
/resume [id]       세션 resume. id 없으면 picker
/list              세션 목록
/title <text>      세션 제목 변경
/model [id]        모델 조회/전환
/config            설정 조회/변경
/creds …           credential 관리
/pair              페어링 코드 생성
/devices           디바이스 목록
/revoke <id>       디바이스 해제
/agents            에이전트 트리 오버레이
/cancel            현재 턴 취소
/ping /status      데몬 프로브
/help /quit
```

`/<skill-name>` — discovered Agent Skill 을 곧바로 활성화. `v1.skill.list` 카탈로그에 있는 이름이 자동완성 후보에 `◆` 글리프로 뜬다. 빌트인 명령과 충돌하면 빌트인이 우선.

## Slash Autocomplete

`/` 로 시작하면 입력창 바로 위에 inline 팝업:

```
▸ /clear     Start a new session (clears the conversation)
▸ /resume    Resume a session (picker when no id)
◆ /pdf-processing   activate skill
▸_
```

- 글리프: `▸` 빌트인, `◆` skill.
- `Tab` : 공통 접두사까지 확장. 단일 후보면 완성 + 공백.
- `Esc` : 팝업 닫기(입력은 유지).
- 최대 8개 후보.

## Overlays

일회성 동작만 오버레이 (파일 피커, 명령 팔레트, 승인).
영구 정보는 inline + 토글 가능.

### 에이전트 트리 (Ctrl+A)

```
┌─ agents ────────────────────── [esc to close] ─┐
│                                                 │
│  ● root (orchestrator)                32s      │
│    task: "이 기능 전체 구현"                    │
│                                                 │
│    ├─ ✓ coder-1      auth 구현      2m 12s     │
│    │    worktree: .harness/wt/coder-1          │
│    │                                            │
│    ├─ ✓ api-builder  routes 추가    1m 48s     │
│    │                                            │
│    └─ ⏳ tester      cargo test     45s        │
│                                                 │
│  [↑↓ navigate] [enter details] [c cancel]       │
└─────────────────────────────────────────────────┘
```

### 파일 탐색 (Ctrl+F)

```
┌─ files ───────────────── [esc to close] ─┐
│  > auth                                   │
│                                           │
│  ▸ src/middleware/auth.rs    (modified)   │
│    src/middleware/rate_limit.rs           │
│    tests/auth_test.rs                     │
│                                           │
│  [↑↓] [enter]                             │
└───────────────────────────────────────────┘
```

### 설정 (/config 또는 Ctrl+,)

```
┌─ settings ───────────────── [esc to close] ─┐
│                                              │
│  Model                                       │
│  ▸ default       gemini-3.1-pro    ▾        │
│                                              │
│  Credentials                                 │
│    anthropic     sk-ant-...k3x2    [edit]   │
│    openai        sk-...f8j1        [edit]   │
│                  [+ add provider]            │
│                                              │
│  Devices                                     │
│    macbook      last seen 2m ago             │
│    iphone       last seen 15m ago            │
│                 [+ pair device]              │
│                                              │
│  [↑↓ navigate] [enter edit]                  │
└──────────────────────────────────────────────┘
```

## Approval UX

Inline Y/N (Codex 패턴):

```
● 실행 요청: rm -rf ./node_modules
  이 명령은 node_modules 디렉토리를 삭제합니다.
  
  [y] 허용  [n] 거부  [a] 이번 세션 동안 같은 패턴 허용
  
> _
```

모달 다이얼로그 없음. 입력 필드에서 키 하나로 결정.
"Always allow for this session" 옵션으로 반복 승인 부담 감소.

## Diff Display

터미널 폭에 따라 자동 분기:

```
< 130 col: unified (기본)
  @@ -12,5 +12,8 @@
    context line
  - removed
  + added

>= 130 col: side-by-side (선택)
  12 │ context          │ 12 │ context
  13 │ removed          │ 13 │ added
```

변경 블록에만 syntax highlight. `+`/`-` 문자 + 색 (색 단독 금지).

## Error Display

인라인 + 액션:

```
✗ worker-2 실패: cargo build 에러
  error[E0432]: unresolved import
   --> src/main.rs:3:5
  
  [r] 재시도  [c] 취소  [l] 로그 보기
```

## Performance

```
ratatui scrolling-regions → flicker 없음
프레임 캡 60fps
스트리밍 배치 flush 16-33ms
syntect 상태 캐싱
dirty region diffing (ratatui 내장)
```

병목은 rendering이 아니라 syntax highlight 재계산. 증분 하이라이트 필수.

## Rust Crates

```
ratatui v0.29+   — TUI 프레임워크 (scrolling-regions)
crossterm        — 터미널 백엔드
syntect          — syntax highlight
tree-sitter      — 선택적 증분 파싱
```
