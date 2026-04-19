# Layer 4: Storage

## Overview

단일 SQLite 파일 (`~/.harness/harness.db`)에 모든 구조화된 데이터를 저장한다.
스키마는 Layer 3에서 정의. 여기선 운영 방식을 다룬다.

## SQLite Configuration

```sql
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
```

- WAL: 동시 읽기 무제한, 쓰기 직렬화 자동
- busy_timeout: 쓰기 경합 시 5초까지 재시도

## Write Pattern

전용 writer 스레드 + mpsc 채널:

```
에이전트 1 ──┐
에이전트 2 ──┼── mpsc::channel ──→ writer 스레드 ──→ SQLite
에이전트 3 ──┘
```

- 모든 DB write가 단일 스레드에서 직렬 처리
- tokio 태스크에서 rusqlite 직접 호출하지 않음 (executor 차단 방지)
- 읽기는 별도 읽기 전용 연결로 자유롭게

## FTS5

messages 테이블에 전문 검색 인덱스:

```sql
create virtual table messages_fts using fts5(
    content,
    content=messages,
    content_rowid=rowid
);
```

트리거 기반 자동 동기화. 메시지 INSERT 시 FTS 인덱스 자동 갱신.

## Migration

`rusqlite_migration` 사용. SQLite의 `user_version` (파일 헤더 정수)으로 버전 추적.
별도 테이블 불필요.

```rust
let migrations = Migrations::new(vec![
    M::up("CREATE TABLE sessions (...)"),
    M::up("CREATE TABLE agents (...)"),
    M::up("CREATE TABLE messages (...)"),
    M::up("CREATE TABLE tool_calls (...)"),
    M::up("CREATE TABLE memory (...)"),
    M::up("CREATE TABLE config (...)"),
    M::up("CREATE TABLE credentials (...)"),
    M::up("CREATE TABLE devices (...)"),
]);
migrations.to_latest(&mut conn)?;
```

스키마 변경 시 새 `M::up(...)` 추가. 데몬 시작 시 자동 적용.

## Backup

하루 1회 `harness.db` → `harness.db.bak`.
SQLite online backup API 사용 (WAL 모드에서도 안전).

## Streaming Flush

LLM 스트리밍 응답은 메모리 버퍼에 누적 후 500ms마다 DB에 flush.
응답 완료 시 최종 write. 크래시 시 최대 500ms분 유실.

## Rust Crates

```
rusqlite (bundled)        — SQLite 바인딩 + SQLite 소스 포함
rusqlite_migration        — user_version 기반 마이그레이션
```
