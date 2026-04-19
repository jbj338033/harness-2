# Layer 0: Daemon Lifecycle

## Overview

harness daemon은 호스트에서 영구 실행되는 단일 프로세스로, 모든 세션과 클라이언트를 관리한다.

## Architecture

```
~/.harness/
  harness.db          # 설정, 키, 세션, 메시지 전부
  harness.db.bak      # 일일 자동 백업
  harness.sock        # Unix socket (로컬 CLI용)
```

- 데몬 1개, 세션 여러 개
- 모든 데이터는 `~/.harness/harness.db` (SQLite, WAL mode)에 저장
- 별도 config 파일 없음. 설정도 DB에 저장
- 설정 변경은 TUI(`/config`) 또는 CLI(`harness config set`)로

## Installation

`install.sh`가 수행하는 작업:

1. 바이너리 설치
2. `~/.harness/` 디렉토리 생성
3. `harness.db` 초기화 (스키마 마이그레이션)
4. OS 서비스 등록 (macOS: launchd, Linux: systemd)
5. 데몬 시작
6. `harness` CLI를 PATH에 추가

설치 후 데몬은 영구 실행. 부팅 시 자동 시작, 크래시 시 자동 재시작.

## Crash Recovery

### Streaming Response 보존

LLM 스트리밍 응답을 메모리 버퍼에 누적하다가 500ms마다 SQLite에 flush.

- 응답 완료 시 최종 write
- 크래시 시 마지막 flush 이후 토큰만 유실 (최대 500ms분)

### Client 재연결

- 클라이언트는 연결 끊김 감지 시 자동 재연결 (exponential backoff)
- 재연결 성공 시 데몬이 전송:
  - 현재 세션 ID
  - 마지막 N턴 대화 (DB에서)
  - 중단된 부분 응답 (있으면)
  - 세션 상태 (idle / streaming 중이었는지)

### Rate Limit 카운터

영속화하지 않음. 재시작 후 API 응답 헤더에서 재구축.

## Logging

데몬은 stderr로 출력. OS가 캡처:

- macOS: `log show --predicate 'process == "harness"'`
- Linux: `journalctl -u harness -f`

Rust `tracing` 크레이트로 구조화된 텍스트 포맷.
로테이션, 보관 기간 등은 OS 로그 시스템에 위임.

## Update

### 수동

```
harness update
```

1. 새 바이너리 다운로드
2. 현재 데몬에 graceful shutdown 시그널
3. 진행 중인 스트리밍이 있으면 flush 후 종료
4. launchd/systemd가 새 바이너리로 자동 재시작
5. 클라이언트 자동 재연결

### 자동

- 하루 1회 새 버전 체크
- idle 상태일 때만 자동 적용
- idle 정의: 연결된 클라이언트 0 + 진행 중인 LLM 호출 0
- idle이 아니면 알림만: "harness v0.3.2 available"

## Model Registry

모델 목록은 DB에 저장하지 않는다. 메모리에서 관리:

1. **내장 모델**: 바이너리에 하드코딩 (anthropic, openai, google 등). `harness update`로 갱신.
2. **로컬 모델**: Ollama 등 로컬 프로바이더는 런타임에 API(`/api/tags`)로 조회.
3. **커스텀 모델**: `harness model add`로 수동 추가. config 테이블에 저장.

```
데몬 시작 시:
  내장 목록 로드 → Ollama 등 로컬 조회 → config에서 커스텀 로드 → 메모리에 합산
```

기본 모델은 config 테이블에 저장: `model.default = "gemini-3.1-pro"`

## Database Schema (Layer 0 범위)

```sql
-- 설정 (key-value)
create table config (
    key text primary key,
    value text not null
);

-- 프로바이더 인증 (평문 + 파일 퍼미션 600으로 보호)
create table credentials (
    id text primary key,
    provider text not null,
    kind text not null,        -- "api_key", "oauth"
    value text not null,
    label text,
    created_at integer not null
);

-- 등록된 디바이스 (ed25519 인증)
create table devices (
    id text primary key,
    name text not null,        -- 클라이언트가 자동 설정 ("iPhone 15" 등)
    public_key blob not null,  -- ed25519 공개키
    last_seen_at integer,
    created_at integer not null
);
```

## Device Pairing (ed25519)

### 페어링 플로우

```
서버 (TUI/CLI):
  $ harness pair
  → QR 코드 표시 (server_url + one-time pairing code)
  → 텍스트로도 표시: server: ..., code: a8f3-k2x1
  → 클라이언트 연결 대기

클라이언트 (폰/웹/리모트CLI):
  1. QR 스캔 또는 서버주소 + 코드 직접 입력
  2. 로컬에서 ed25519 키 쌍 생성
  3. 공개키 + 페어링 코드 + 디바이스 이름 전송
  4. 서버가 코드 확인 → 공개키 DB 저장 → 페어링 완료
  5. 페어링 코드 즉시 만료 (1회용)

리모트 CLI:
  $ harness connect 192.168.0.10:8384 --code a8f3-k2x1
  → 로컬에 ed25519 키 쌍 생성
  → 공개키 + 코드 전송 → 페어링 완료
```

### 인증 플로우 (페어링 이후)

```
1. 클라이언트 → WS 연결
2. 서버 → 랜덤 nonce 전송
3. 클라이언트 → 프라이빗 키로 nonce 서명 → 서명 전송
4. 서버 → DB에서 공개키 조회 → 서명 검증
5. 통과 → 세션 연결
```

### 디바이스 관리

```
harness device list              # 등록된 디바이스 목록
harness device revoke "iphone"   # 특정 디바이스 차단 (공개키 삭제)
```

## DB Backup

하루 1회 `harness.db` → `harness.db.bak` 자동 복사.
SQLite online backup API 사용 (WAL 안전).
