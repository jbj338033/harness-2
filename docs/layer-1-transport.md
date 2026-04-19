# Layer 1: Transport

## Overview

클라이언트와 데몬 간 바이트가 흐르는 경로. 두 가지 트랜스포트를 제공한다.

```
로컬 CLI (TUI)  ─── Unix socket ───┐
                                    ├──→ 데몬 코어
원격 클라이언트  ─── WebSocket ────┘
```

두 트랜스포트 모두 내부적으로 같은 핸들러로 합류하여 동일한 JSON-RPC 메시지로 처리.

## Unix Socket

```
경로: ~/.harness/harness.sock
퍼미션: 700 (소유자만 접근)
용도: 로컬 TUI 전용
연결: 지속 (TUI가 떠있는 동안 유지)
암호화: 불필요 (같은 머신)
```

## WebSocket

```
바인딩: 0.0.0.0:8384
프로토콜: wss:// (자체 서명 TLS)
용도: 원격 클라이언트 (폰, 웹, 리모트 CLI)
```

### TLS

설치 시 자체 서명 인증서를 자동 생성.

- CLI / 모바일 앱: cert pinning (페어링 시 받은 fingerprint로 검증). 경고 없음.
- 브라우저: 최초 1회 `https://서버:8384` 접속 → 경고 → 예외 수락 → 이후 WSS 정상.

도메인을 설정하면 Let's Encrypt 자동 발급으로 모든 클라이언트에서 경고 없이 동작:
```
harness config set domain myharness.example.com
→ 데몬이 ACME로 인증서 자동 발급/갱신
```

### 인증

ed25519 challenge-response. 페어링된 디바이스만 접속 가능.

### 페어링 QR

```
{ server, code, fingerprint }
```

fingerprint는 TLS 인증서의 SHA-256 해시. CLI/모바일 앱이 cert pinning에 사용.

### 포트 충돌

8384가 이미 사용 중이면 데몬 시작 실패 + 에러 로그.
`harness config set network.ws_port 8385`로 변경.

### Heartbeat

WS 프로토콜 내장 ping/pong 사용. 별도 구현 없음.

```
ping_interval: 60s
ping_timeout: 10s
```

## Rust 구현

```
axum          — HTTP/WSS 서버
tokio         — async runtime
rustls        — TLS
tokio::net::UnixListener — Unix socket

두 listener를 tokio::select!로 동시 구동.
내부 AppState를 Arc로 공유.
```
