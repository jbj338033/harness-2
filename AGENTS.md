# AGENTS.md — harness

Entry point for AI coding agents (Codex · Cursor · Aider · Copilot · Claude Code · any [agents.md](https://agents.md) compatible reader).

> **Precedence:** this file is the universal agent contract. Tool-specific files (`CLAUDE.md`, `.cursor/rules/*.mdc`, `.github/copilot-instructions.md`) add to this, they do not override it.

## What harness is

A Rust workspace — a persistent, multi-agent AI coding daemon. A daemon that outlives sessions, remembers across projects, and pulls maximum performance from LLM models.

- Product README: [`README.md`](./README.md)
- Implementation rules (Rust-specific, stricter): [`CLAUDE.md`](./CLAUDE.md)
- Design workspace (455 decisions, 58 research papers): [`.harness-design/AGENTS.md`](./.harness-design/AGENTS.md) → [`.harness-design/SPECS.md`](./.harness-design/SPECS.md)

## Commands

```sh
cargo build --workspace
cargo test  --workspace
cargo fmt   --all
cargo clippy --workspace --all-targets -- -D warnings

cargo run -p harnessd                       # daemon
cargo run -p harness-tui --bin harness      # TUI against a running daemon
```

All four must pass. Zero warnings is the bar.

## Non-negotiable rules (from `CLAUDE.md`)

1. No `#[allow(...)]` anywhere — fix the lint root cause
2. No `todo!()`, `unimplemented!()`, or placeholder `Ok(())` bodies
3. No `_`-prefixed params unless a trait signature forces one
4. No narrowing `as` casts — use `TryFrom` / `try_into()`
5. No parallel versions (`foo_v2`, `foo_new`) — edit the existing function

Plus: no comments except `// SAFETY:` on `unsafe`, one-line invariant, `///` on public items.

## Code style

- `cargo fmt` is the source of truth.
- Errors: lowercase first word, no trailing period.
- `pub` is opt-in — export the minimum, use re-exports in `lib.rs` / `mod.rs`.
- Platform-specific code lives in its own module gated with `#[cfg(...)]`.
- Provider-specific options flow through namespaced struct pattern (`ProviderOptions { anthropic, openai, google, ollama, extra }`), not a tagged enum.

## Where to find things

| If you are... | Read |
|---|---|
| Building a feature in Rust | [`CLAUDE.md`](./CLAUDE.md) → crate `README.md` → relevant `crates/*/src/` |
| Adding a tool | `CLAUDE.md` "Adding a tool" section |
| Adding an RPC method | `CLAUDE.md` "Adding an RPC method" section |
| Consulting a design decision | [`.harness-design/DECISIONS.md`](./.harness-design/DECISIONS.md) (append-only SSoT, 455 decisions) |
| Reading the spec | [`.harness-design/SPECS.md`](./.harness-design/SPECS.md) (25 sections, v1 target) |
| Researching a design axis | [`.harness-design/INDEX.md`](./.harness-design/INDEX.md) → `RESEARCH/R{N}.md` |
| Understanding identity / 3 primitives | [`.harness-design/SPECS.md`](./.harness-design/SPECS.md) §1–§5 |

## Design invariants (MUST NOT violate)

From `.harness-design/DECISIONS.md`:

1. **Events strict append-only** (D-031, D-181). Projections are mutable but always recomputable from events fold.
2. **Main thread single** (D-007, D-135). Subagents query-only; Worker wave requires `files_modified` disjoint proof (D-045).
3. **No own formats** (D-004, D-179). Ingest existing: SKILL.md · AGENTS.md · CLAUDE.md · `.claude/settings.json` · MCP · ACP · Cursor `environment.json` · Codex `rollout.jsonl` · Agent Trace.
4. **Compile-time approval gate** (D-323). `Action<Unapproved/Approved>` typestate — unapproved Act = build failure.
5. **Branding** (D-424). Never use "분신" / "bunshin"; use "agent" / "에이전트".

## Git

- Commit messages in English, imperative mood, lowercase first word. Format: `type: description`.
- One logical change per commit. Never skip hooks. Never commit secrets.
- `$HOME/.harness/` lives outside the repo.
- Never push without an explicit request.

## Don't

- Don't touch the five principles without explicit approval
- Don't edit workspace `Cargo.toml` pins casually
- Don't modify RPC namespaces already in production
- Don't invent new formats when a standard exists (D-004, D-179)
- Don't treat superseded decisions as active — check `status: superseded by D-NNN`

---

**This file follows the [agents.md](https://agents.md) convention.** For Claude-specific guidance including the full "Adding a tool / Adding an RPC method" playbooks, read [`CLAUDE.md`](./CLAUDE.md).
