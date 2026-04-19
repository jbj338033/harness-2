# Layer 9: Agent Skills

## Overview

Agent Skills — [agentskills.io](https://agentskills.io) — 는 절차적 지식을 `SKILL.md` 디렉토리로 패키징하는 오픈 표준. Claude Code, OpenCode, Cursor, VS Code, GitHub Copilot, OpenAI Codex 등 20+ agent 가 동일 포맷을 공유한다.

harness 는 이 표준을 준수하는 소비자 (consumer) 이다. 즉 SKILL.md 를 읽고 로드할 뿐, 새 SKILL.md 포맷을 만들지 않는다.

## Directory layout (spec)

```
skill-name/
├── SKILL.md          required: frontmatter + body
├── scripts/          optional: executable helpers
├── references/       optional: docs loaded on demand
└── assets/           optional: templates / binary files
```

`SKILL.md` 는 YAML frontmatter + Markdown body:

```markdown
---
name: pdf-processing
description: Extract PDF text, fill forms, merge files. Use when handling PDFs.
license: Apache-2.0
allowed-tools: Bash(git:*) Read
---

# Usage
1. read with pdfplumber...
```

필수 필드: `name` (≤64, a-z0-9-), `description` (≤1024).
선택: `license`, `compatibility`, `allowed-tools`, arbitrary `metadata`.

## Discovery

harness 는 **6 개 경로**를 스캔한다. 앞이 우선 (project > user, Native > Std > Claude):

```
<cwd>/.harness/skills/    project,  Native
<cwd>/.agents/skills/     project,  Std
<cwd>/.claude/skills/     project,  Claude
~/.harness/skills/        user,     Native
~/.agents/skills/         user,     Std
~/.claude/skills/         user,     Claude
```

규칙:
- 하위에 `SKILL.md` 있는 디렉토리만 skill 로 인식.
- 동일 `name` 충돌 시 먼저 본 것을 유지하고 warn 로그.
- 최대 깊이 4, 루트 당 최대 2000 entry (표준 권장).
- `.git`, `node_modules`, `target` skip.

### Lenient parsing

`SKILL.md` 파싱은 관대하게:

- 프론트매터 YAML 파싱 실패 → 값에 따옴표를 둘러 한 번 재시도 (Unquoted colon 대응).
- `name` 이 spec 위반(대문자, 하이픈 앞뒤, 64 초과) → **warn + 로드**.
- 디렉토리명 ≠ `name` → warn + 로드.
- `description` 없음 → **드롭** (디스커버리에 필수).

## Progressive disclosure

세 단계로 토큰 비용을 점진적으로 지불:

| Tier | 시점 | 비용 |
|---|---|---|
| 1. Catalog (name + description) | 세션 시작 | ~100 토큰/skill |
| 2. Body | `activate_skill` 호출 | ≤5k 토큰/skill 권장 |
| 3. Resources (scripts/references/assets) | body 가 참조할 때 | lazy |

### System prompt 주입 — tier 1

`harness-context::assemble()` 는 시스템 프롬프트에 `<available_skills>` 블록을 삽입한다. 태그 이름은 Claude Code 의 de-facto convention (`anthropics/claude-code` issue #16072):

```xml
<available_skills>
  <skill>
    <name>pdf-processing</name>
    <description>Extract PDF text, fill forms, merge files.</description>
    <location>/Users/u/.agents/skills/pdf-processing/SKILL.md</location>
  </skill>
  ...
</available_skills>
```

한 문단의 behavioral instruction 이 뒤따른다: "description 매칭 시 `activate_skill` tool 호출".

### Activation — tier 2

`activate_skill` 은 harness 가 제공하는 내장 도구. 입력:

```json
{ "name": "pdf-processing" }
```

이 tool 이 수행하는 일:
1. `Catalog::get(name)` 으로 skill 조회.
2. `harness_skills::activate(skill)` → `(body, directory, resources)`.
3. `messages` 에 row 삽입:
   - `role = system`
   - `kind = 'skill_attachment'`
   - `content = <skill_content name="...">...\n<skill_resources>...</skill_resources></skill_content>`
4. Tool result 로 `"skill activated: {name}"` 반환 (본문은 이미 대화 스레드에 들어감).

**Dedupe**: 동일 agent 에서 같은 skill 재활성화 시 "already active" 로 no-op. DB 의 `idx_messages_kind` 인덱스가 빠른 확인을 제공.

### Resources — tier 3

`scripts/`, `references/`, `assets/` 아래 파일은 activation 결과의 `resources` 배열에 **경로만** 포함된다. 실제 읽기는 body 가 참조할 때 LLM 의 `read` 도구로.

## RPC surface (v1)

`docs/layer-2-protocol.md` 의 v1 네임스페이스:

- **`v1.skill.list`** → `{ skills: [{ name, description, location, scope, layout }] }`.
- **`v1.skill.activate { name }`** → `{ name, body, directory, resources: [string] }`. frontmatter stripped.

`activate_skill` 내장 도구는 내부적으로 `v1.skill.activate` 경로와 같은 함수를 호출하지만, 도구 경로는 본문을 **메시지로 영속화**하고 RPC 경로는 단순히 반환만 한다. TUI 슬래시 (`/skill-name`) 는 도구 경로를 사용한다.

## Storage — `messages.kind`

`messages` 테이블은 V3 migration 에서 `kind TEXT NOT NULL DEFAULT 'chat'` 을 갖는다. 현재 값:

| kind | 생성 주체 | 의미 |
|---|---|---|
| `chat` | 기본 | 일반 user/assistant/system 턴 |
| `skill_attachment` | `activate_skill` | SKILL.md 본문 주입 |

미래 확장: `plan_attachment`, `async_agent_attachment`. Claude Code 의 `attachment.type` 패턴과 동일한 의미론이되 harness 는 **in-memory state 가 아닌 DB 자체**를 source of truth 로 삼는다 (원칙 1).

## Compaction policy

harness 에 context compaction 은 아직 없지만, 도입 시 다음 정책을 채택한다 (Claude Code 수치 그대로):

- summary 생성은 `WHERE kind = 'chat'` 행만 대상. `skill_attachment` 는 요약에서 제외.
- summary 직후 활성 skill 을 재주입. per-skill 최대 5K 토큰 (head 우선 truncation). 총 skill 예산 25K 토큰. 초과 시 오래된 것부터 drop.

`harness-skills::POST_COMPACT_MAX_TOKENS_PER_SKILL` / `POST_COMPACT_SKILLS_TOKEN_BUDGET` 상수로 노출 예정.

## Security model

harness 는 project-level skill 도 기본 로드한다 (trust 게이트 없음). 이유:

- skill 본문은 **실행 권한이 없는 지시문**이다. 본문 자체로는 시스템을 건드릴 수 없다.
- skill 이 LLM 에게 도구 호출을 지시하더라도 harness 의 **기존 approval gate + 원칙 5 범위 검증**이 작동한다.
- skill 의 `allowed-tools` frontmatter 는 spec 상 **pre-approved** 선언. 그 밖의 도구 호출은 평소와 동일한 승인 경로를 거친다.

즉 trust 게이트는 기존 보안 계층과 중복이고 UX 마찰만 늘린다.

## Cross-client compatibility

`.agents/skills/` 와 `.claude/skills/` 를 동시 스캔하므로:

- 사용자가 Claude Code 로 설치한 skill 이 harness 에서도 작동.
- harness 에서 새 skill 을 작성해 `.agents/skills/` 에 두면 OpenCode, Cursor 등도 사용 가능.

세 경로에 동일 이름이 있으면 Native > Std > Claude 순으로 우선.
