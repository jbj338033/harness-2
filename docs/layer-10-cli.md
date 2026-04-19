# Layer 10: CLI

`harness` 는 단일 바이너리다. 인자 없이 실행하면 TUI 로 진입하고, 첫 positional 이 서브커맨드면 CLI 모드로 전환된다. `harness-cli` 같은 별도 바이너리는 없다.

## 진입 분기

```
harness                        → TUI (신규 세션)
harness -c                     → TUI (cwd 최근 세션 resume)
harness -r [id]                → TUI (id 지정 resume / picker)
harness <subcommand> [args]    → CLI 모드, `harness_cli::run` 로 위임
```

`harness-tui/src/main.rs::parse_args` 가 `harness_cli::is_subcommand(&arg)` 로 첫 토큰을 검사해 분기한다.

## 서브커맨드 카탈로그

별칭은 파서에서 정규화된다: `ls`→`list`, `rm|delete|del`→`remove`, `show`→`get`.

### auth
| | |
|--|--|
| `auth login [provider]` | inline CLI picker (`dialoguer`). API key 또는 Codex OAuth |
| `auth add <provider> <value>` | 비대화형 API key 등록 (스크립트 친화) |
| `auth list` / `ls` | 저장된 credential (키 마스킹) |
| `auth remove <id>` / `rm`/`delete` | credential 삭제. `kind=oauth` 면 `harness-auth::oauth::openai::revoke` 호출 후 DB 삭제 |

지원 provider: `anthropic` · `openai` (API key + Codex OAuth) · `google` (gemini) · `ollama` (endpoint URL 만, credential 불필요).

### model
| | |
|--|--|
| `model list` / `ls` | 등록된 모델 + context window |
| `model use <id>` / `set`/`default` | `config.default_model` 세팅 |
| `model current` / `get`/`show` | 현재 default |

### config
| | |
|--|--|
| `config list` / `ls` | 전체 key=value |
| `config get <key>` | 단건 조회 |
| `config set <key> <value>` | 세팅 |
| `config unset <key>` | 삭제 |

### skill
| | |
|--|--|
| `skill list` / `ls` | discovered Agent Skills (6경로 스캔 결과) |
| `skill info <name>` / `get`/`show` | frontmatter + SKILL.md body + resources |

### mcp
| | |
|--|--|
| `mcp list` / `ls` | 등록된 MCP 서버 |
| `mcp add <name> -- <command> [args…]` | 서버 등록 (config 테이블에 JSON 저장) |
| `mcp remove <name>` / `rm` | 삭제 |

### devices · pair · connect
| | |
|--|--|
| `pair` | pairing code 발급 (daemon 필수) |
| `connect <wss-url> <code> <name>` | 원격 daemon 에 이 기기 등록 |
| `devices list` / `ls` | 페어링된 기기 |
| `devices revoke <id>` / `rm`/`remove` | 기기 해제 |

### 기타
| | |
|--|--|
| `status` | daemon + paired devices 요약 |
| `doctor` | daemon socket / protocol / credentials / skills / MCP 5축 check. 첫 실패 시 exit 1 |
| `setup` | 인터랙티브 초기 설정 (wizard 재사용 예정) |
| `--help` · `--version` | 도움말 · 버전 |

## auth 플로우

`harness auth login`은 채팅 TUI를 띄우지 않는다 — `harness-cli/src/auth_login.rs` 의 `dialoguer` 기반 inline 프롬프트가 호출 셸에서 직접 동작한다.

1. `Select` 로 provider 선택 (Anthropic / OpenAI / Gemini / Ollama / Skip). `harness auth login <provider>` 로 호출하면 해당 항목에 커서가 프리셀렉트.
2. provider 별:
   - **Anthropic / Gemini** → `Password` 마스킹 입력 → `v1.auth.credentials.add { provider, kind:"api_key", value }`.
   - **OpenAI** → 두번째 `Select` 로 "API key" 또는 "Codex OAuth (ChatGPT plan)" 분기.
     - API key → 위와 동일.
     - Codex OAuth → 아래 §Codex OAuth 흐름.
   - **Ollama** → `Input` 으로 endpoint URL (`http://localhost:11434` 프리필) → `v1.config.set { key:"ollama.endpoint", value }`.
   - **Skip** (또는 Esc / Ctrl+C) → 그냥 종료.
3. 채팅 TUI 진입 시 credentials 리스트가 비어 있으면 시스템 라인으로 `harness auth login` 실행을 안내한다.

## Codex OAuth

`harness auth login` → OpenAI → "Codex OAuth" 선택 시:

1. `harness_auth::oauth::pkce::gen_pkce()` 로 PKCE triple 생성.
2. `harness_auth::oauth::loopback::spawn_loopback()` 으로 `127.0.0.1:<random>` 콜백 서버 기동.
3. `authorize_url(pkce, redirect_uri)` 출력 + `open::that(url)` 로 브라우저 자동 열기 (실패 시 URL 수동 복붙 안내).
4. 사용자 승인 후 `CallbackResult::Ok { code, state }` 수신, `state` 가 `pkce.state` 와 다르면 거부.
5. `oauth::openai::exchange_code(code, verifier, redirect)` → `TokenBundle`.
6. `serde_json::to_string(&bundle)` 한 결과를 `v1.auth.credentials.add { provider:"openai", kind:"oauth", value }` 로 저장.
7. `harnessd` 의 `providers.rs` 가 `("openai","oauth")` 분기에서 `OpenAiProvider::new_oauth(...)` 로 풀에 자동 등록.

OAuth 모듈: `harness-auth/src/oauth/{pkce,loopback,openai,device_code}.rs`.

- Client ID: `app_EMoamEEZ73f0CkXaXp7hrann` (OpenAI 공식 Codex 클라이언트와 동일).
- Endpoint: `https://auth.openai.com/oauth/{authorize,token,revoke}`.
- ChatGPT plan 요청은 `https://chatgpt.com/backend-api`, API key 는 `https://api.openai.com/v1/...` — `harness-llm-openai` 가 credential `kind` 로 라우팅.
- Refresh race: writer task 가 단일 스레드고 SQLite writer 는 one-at-a-time 이라 `credentials.replace_value` 는 이미 직렬화. `openai/codex` #52037 / #10332 회피.
- Device-code flow (`oauth/device_code.rs`) — 헤드리스 환경용. 현재 `auth login` UI 에는 미노출, 후속.

## 개발 노트

- `harness-cli` crate 는 **lib only** (`[lib]`, `[[bin]]` 없음). `harness-tui` 가 의존해 `harness_cli::run(args)` 로 위임.
- `install.sh` 는 `harness` + `harnessd` 두 바이너리만 빌드 / 설치한다. 구 `harness-cli` 바이너리 언급은 전부 제거됐다.
