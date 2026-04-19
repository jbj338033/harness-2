# Layer 3: Session & Agent Model

## Overview

세션은 작업 단위. 에이전트는 세션 안의 실행 단위.
사용자가 `harness`를 실행하면 새 세션이 생기고, 루트 에이전트 1개가 시작된다.
이전 세션은 `harness -r` 또는 TUI에서 `/resume`으로 복귀.

## Core Model

```
session
  └─ agent (root) ← 사용자와 직접 대화
       ├─ agent (worker-1, worktree-a)
       ├─ agent (worker-2, worktree-b)
       └─ agent (reviewer)
            └─ agent (sub-reviewer)  ← 중첩 제한 없음
```

- solo = 에이전트 1개인 세션. 별도 모드 아님.
- team = 에이전트가 spawn한 세션. 별도 모드 아님.
- 모든 에이전트가 같은 규칙으로 동작: 일회용 + 파일시스템 상태.

## Agent Lifecycle

```
1. 태스크(프롬프트) 수신
2. 파일시스템/DB에서 현재 상태 파악
3. 단순하면 직접 실행
4. 복잡하면:
   a. 코드베이스 분석
   b. plan.md 작성 (파일, 함수, 패턴, 테스트 기준)
   c. 워커 spawn (각 워커가 plan의 자기 부분 실행)
5. 진행 상황 기록 (상태→SQLite, 산출물→파일시스템)
6. 품질 게이트 (test/lint 통과 후 커밋)
7. 완료 → status="done"
8. 미완료 + 종료 → 데몬이 같은 태스크로 재시작 (iteration++)
9. circuit breaker: 파일 변경 없이 3회 / 동일 에러 5회 → status="failed"
```

## Agent Primitives (도구)

```
spawn(role, task)    — 서브에이전트 생성 (fresh context, optional worktree)
wait(agents)         — 완료 대기
cancel(agent)        — 중단
```

기존 도구 (bash, edit, read 등)와 동일한 레벨로 제공.

## Context Strategy

- 오케스트레이터: 30-40% 컨텍스트 이내 유지 (계획/조율만)
- 워커: 태스크 단위 fresh context. 부모의 대화 히스토리 안 받음.
- 워커가 받는 것: system prompt + role prompt + task (plan.md에서) + project memory + cwd
- 컨텍스트 소진 감지: 60% 넘으면 경고. 에이전트가 판단 (마무리 or 재시작).
- 태스크를 잘 나누면 컨텍스트 소진 자체가 발생하지 않음.

## State Management

5원리 기반:

```
SQLite (구조화된 상태):
  에이전트 상태 (pending/running/done/failed)
  메시지 히스토리
  도구 호출 기록

파일시스템 (산출물):
  plan.md — 계획 (= 워커의 프롬프트)
  코드 변경 — git worktree 안에서
  result.md, SUMMARY.md — 워커 결과물
```

원리 2 (계획은 계약): plan.md가 워커의 scope를 정의.
원리 5 (선언된 범위): plan.md에 명시된 파일만 수정 가능. 데몬이 검증.

## Session Title

첫 번째 응답 완료 후 비동기로 저가 모델에 요청하여 자동 생성.
사용자가 `/title` 으로 언제든 덮어쓸 수 있음.

## Multi-Client

여러 클라이언트가 같은 세션에 동시 접속 가능.
- LLM 스트리밍은 broadcast 채널로 모든 클라이언트에 동시 전송.
- 입력 충돌: 큐 기반. 스트리밍 중 다른 클라이언트 입력은 큐에 넣고 현재 완료 후 처리.

## Database Schema

```sql
create table sessions (
    id text primary key,
    title text,
    cwd text not null,
    task text,                    -- 원본 태스크 (Ralph 재시작 시 재사용)
    created_at integer not null,
    updated_at integer not null
);

create table agents (
    id text primary key,
    session_id text not null references sessions(id),
    parent_id text references agents(id),
    role text not null,           -- "root", "coder", "reviewer", "tester", ...
    model text not null,
    status text not null,         -- "pending", "running", "done", "failed"
    worktree_path text,           -- git worktree (격리 실행 시)
    iteration integer not null default 1,  -- Ralph 반복 횟수
    created_at integer not null,
    completed_at integer
);

create table messages (
    id text primary key,
    agent_id text not null references agents(id),
    role text not null,           -- "user", "assistant", "system"
    content text,
    tokens_in integer,
    tokens_out integer,
    cost real,
    model text,
    created_at integer not null
);

create table tool_calls (
    id text primary key,
    message_id text not null references messages(id),
    name text not null,           -- "bash", "edit", "read", "spawn", ...
    input text not null,          -- JSON
    output text,                  -- 결과 (실행 전엔 null)
    exit_code integer,
    duration_ms integer,
    created_at integer not null
);

create table memory (
    id text primary key,
    project text,                 -- null이면 global
    content text not null,
    created_at integer not null
);

create virtual table messages_fts using fts5(
    content,
    content=messages,
    content_rowid=rowid
);
```

## Principle Enforcement

| 원리 | 시행 방법 |
|------|----------|
| 1. 일회용 에이전트 | 워커는 fresh context. 재시작 시 새 에이전트 (iteration++). |
| 2. 계획은 계약 | plan.md 기반 실행. 계획 밖 변경은 tool_calls에 기록. |
| 3. 증거 없으면 거짓 | 품질 게이트: test/lint 통과해야 커밋. "should work" 불가. |
| 4. 나쁜 작업 차단 | 게이트 실패 시 커밋 불가. 실패한 코드는 쌓이지 않음. |
| 5. 선언된 범위 | plan.md에 명시된 파일만 수정 허용. 파괴적 명령 차단. worktree 격리. |

## Resource Controls

- 비용 예산: 세션당 상한 설정 가능. 초과 시 중단.
- 동시 에이전트 수: 기본 5, 설정 가능. 초과 시 큐잉.
- TUI: 에이전트 트리 전체 표시. 상태, 비용, 진행 상황.
- 취소: 아무 에이전트나 즉시 취소 가능.
