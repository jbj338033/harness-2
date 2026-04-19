# Layer 2: Protocol

## Overview

JSON-RPC 2.0 over Unix socket / WebSocket. MCP, LSP와 동일한 프로토콜.
필요 시 경량 바이너리 프로토콜을 나중에 추가할 수 있다.

## Versioning

메서드 이름에 `v{n}.` prefix 로 명시적 버전. LSP `initialize` capability 방식과 동일 철학이되 prefix-on-method 로 구현이 단순.

### 구조

```
pre-negotiation (unversioned)  ping, status, negotiate
v1.* (active)                  v1.session.*, v1.chat.*, v1.skill.*, v1.config.*,
                                v1.auth.*, v1.device.*
future: v2.*, v3.*
```

- 클라이언트는 접속 직후 `negotiate { client_versions: [...] }` 로 지원 버전 제시.
- 서버는 **양쪽이 지원하는 최고 버전**을 `{ selected: N, server_versions: [...] }` 로 응답.
- 공통 버전이 없으면 `error { code: -32010 (VersionMismatch) }`.
- 이후 모든 method 호출은 `v{selected}.` prefix 사용.
- 여러 버전을 한 연결에 섞어 쓰지 않음 (negotiate 가 connection-sticky).

### Bump 기준

기존 버전을 **같은 자리에서** 건드리는 breaking change 는 금지. 새 버전을 추가한다.

Bump 가 필요한 경우:
- 필드 이름 변경, 삭제
- 선택(optional) 필드를 필수(required) 로 전환
- 필드 의미 변경 (예: `timestamp` 가 unix ms → ISO8601)
- method 삭제

Bump 가 **불필요한** 경우 (backward-compatible):
- 새 optional 필드 추가
- 새 method 추가 (기존 method 의 의미 변경 없음)
- 응답에 새 필드 추가
- 구현 최적화

### 구버전 유지

새 major 버전이 등장해도 구버전은 **최소 한 개 메이저 리비전 동안** 함께 제공한다. `SUPPORTED_VERSIONS = &[1, 2]` 같은 상태가 정상이며, 라우터는 `v1.chat.send` 와 `v2.chat.send` 를 모두 route 로 등록한다. 구버전 제거는 별도 릴리스 노트에 고지 후 다음 메이저에서.

### 상수

`harness_proto::SUPPORTED_VERSIONS: &[u32]` 가 단일 소스. 클라이언트·서버 모두 이 상수를 참조.

## Message Format

### 요청 (클라이언트 → 서버)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "chat.send",
  "params": {
    "session_id": "abc",
    "message": "버그 고쳐줘"
  }
}
```

### 응답 (서버 → 클라이언트)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "status": "done",
    "tokens_in": 142,
    "tokens_out": 89,
    "cost": 0.003
  }
}
```

### Notification (서버 → 클라이언트, 스트리밍용)

id가 없는 메시지. JSON-RPC 스펙에 정의된 표준 패턴.

```json
{"jsonrpc":"2.0","method":"stream.delta","params":{"session_id":"abc","content":"분석해보겠습니다"}}
```

### 에러

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32600,
    "message": "invalid session_id"
  }
}
```

## Streaming

LLM 스트리밍은 notification으로 처리:

```
클라이언트 → chat.send (id: 1)

서버 → stream.delta    (토큰 청크, 여러 번)
서버 → stream.tool_call (도구 호출 시작)
서버 → stream.tool_result (도구 실행 결과)
서버 → stream.delta    (이어서 토큰)
서버 → stream.done     (스트리밍 종료)

서버 → result (id: 1, 최종 통계)
```

토큰은 개별 전송이 아닌 청크 단위. LLM API 응답이 오는 단위 그대로 전달하여 오버헤드 최소화.

## Methods

메서드 목록은 상위 레이어에서 구체화. 네이밍 컨벤션은 `v{n}.domain.action` (pre-negotiation 은 무버전):

```
ping, status, negotiate        pre-negotiation (unversioned)

v1.session.*                   세션 관리
v1.chat.*                      대화
v1.skill.*                     Agent Skills (layer-9)
v1.approval.*                  도구 승인 응답 (layer-6)
v1.mcp.*                       MCP 서버 등록 (layer-6)
v1.config.*                    설정
v1.auth.*                      인증/credential
v1.device.*                    디바이스 관리

stream.*                       스트리밍 notification (connection-sticky,
                                negotiate 로 버전 고정된 후 항상 해당 버전의 의미)
approval.request               도구가 승인을 요청 (클라이언트가 v1.approval.respond 로 회신)
```

### pre-negotiation

- **`ping`** → `{ pong, version, protocol_versions }`. 연결 기본 정보.
- **`status`** → 데몬 구동 통계.
- **`negotiate`** → `NegotiateParams { client_versions: [u32] }` → `NegotiateResult { selected, server_versions }`.

### v1.skill.*

- **`v1.skill.list`** → `{ skills: [{ name, description, location, scope, layout }] }`.
- **`v1.skill.activate`** → `{ name: string }` → `{ name, body, directory, resources: [string] }`.

전체 스펙은 `docs/layer-9-skills.md` 참조.

### v1.approval.*

- **`v1.approval.respond`** → `{ request_id, decision: "allow"|"deny"|"allow_session"|"allow_global", pattern?, session_id? }`.
  - `allow_session` / `allow_global` 은 `approvals` 테이블에 persist.
  - 메모리 `pending_approvals[request_id]` 에 대기 중인 tool 이 있으면 `decision` 으로 재개.
  - 발행 측(도구가 `approval.request` notification 을 쏘는 경로)은 점진적으로 추가 중.

### v1.mcp.*

MCP 서버 등록은 `config` 테이블의 `mcp.server.<name>` 키에 JSON body 로 저장된다. 별도 테이블 없음.

- **`v1.mcp.add`** → `{ name, command, args?: [string], env?: {string: string} }` → `{ added }`.
- **`v1.mcp.list`** → `{ servers: [{ name, command, args, env }] }`.
- **`v1.mcp.remove`** → `{ name }` → `{ removed }`.

`harnessd` boot-time 에서 이 prefix 를 읽어 `harness_mcp::Supervisor` 로 넘기는 작업은 미구현 (STATUS.md 참조).

## Binary Data

이미지, 파일 등 바이너리는 파일 경로로 전달하는 것을 기본으로 한다.
직접 전송이 필요한 경우 base64 인코딩.
