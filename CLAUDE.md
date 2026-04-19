# CLAUDE.md

House rules for anyone (human or agent) editing this repo.

## Scope

`harness` is a Rust workspace — a persistent, multi-agent AI coding daemon. Read [`README.md`](README.md) for the product. The five non-negotiable principles below govern every design choice.

## Commands

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings

cargo run -p harnessd                       # daemon
cargo run -p harness-tui --bin harness      # TUI against a running daemon
```

All four must pass. Zero warnings is the bar.

## Non-negotiable rules

**No workarounds.** Fix the root cause. Specifically:

- **No `#[allow(...)]` anywhere.** No crate-level, file-level, or inline. Fix the code so the lint doesn't trigger. The project uses default clippy (correctness + suspicious + style). `clippy::pedantic` is not enabled — it's an opt-in style group and we don't use it.
- **No `#[allow(dead_code)]`.** Delete the dead code.
- **No `let _ =` to swallow `Result`.** Handle it, propagate it with `?`, or consume it explicitly — `.ok()` if you genuinely don't care, `.unwrap()` / `.expect("reason")` only where failure is impossible.
- **No `_`-prefixed parameters** except where a trait signature forces one. If you control the signature, remove the parameter.
- **No `todo!()`, `unimplemented!()`, or placeholder `Ok(())` bodies.** Either finish it or don't start it.
- **No `as` casts** for narrowing integer conversions. Use `TryFrom` / `try_into()` and handle the error. Bitmasks first (`(v & 0xFF) as u8`) or integer math instead of float is usually the right answer.

**No comments, with three exceptions:**

1. `// SAFETY:` on `unsafe` blocks — required.
2. A single short line on a hidden invariant, ordering constraint, or workaround tied to a real bug.
3. `///` on a public item when the name alone genuinely isn't enough.

Never write a comment that restates what the code does. Never leave a commit / PR / issue reference in a comment — that belongs in git history.

**No stubs, no parallel versions.** Edit the existing function. Don't add `foo_v2`, `foo_new`, or shim wrappers.

**No speculative flexibility.** A bug fix doesn't need a refactor. Three similar lines beat a premature abstraction. Only validate at system boundaries (user input, external APIs).

## Code style

- `cargo fmt` is the source of truth.
- Errors: lowercase first word, no trailing period.
- `pub` is opt-in — export the minimum, use re-exports in `lib.rs` / `mod.rs`.
- Platform-specific code lives in its own module, gated with `#[cfg(...)]`.
- Provider-specific options flow through the namespaced struct pattern (`ProviderOptions { anthropic, openai, google, ollama, extra }`) — not a tagged enum.

## Testing

- Tests go in `#[cfg(test)] mod tests` inside the same file unless they need fixtures.
- Prefer `tempfile::NamedTempFile` / `TempDir` over touching `~/.harness`.
- Tools that hit external services get a `wiremock` test. The pattern lives in [`crates/harness-llm-anthropic/src/lib.rs`](crates/harness-llm-anthropic/src/lib.rs).

## Adding an RPC method

1. Add a handler in `crates/harnessd/src/rpc/<namespace>.rs`.
2. Register it in `crates/harnessd/src/rpc/mod.rs::build_router`.
3. Add a client call in `harness-cli` or the TUI if users need to reach it.

## Adding a tool

1. Implement `harness_tools::Tool` in its own crate (`harness-tools-<name>`).
2. Register it in `crates/harnessd/src/tools.rs::build`.
3. Tool descriptions follow the `USE:` / `DO NOT USE:` convention.
4. Gate anything that touches external systems behind `harness_tools::ApprovalGate`.

## Turn loop

[`crates/harness-agent/src/turn.rs`](crates/harness-agent/src/turn.rs) owns the chat → stream → tool-dispatch → re-chat cycle. Anything that changes persistence ordering (user message, placeholder assistant message, `tool_calls` rows, final `Done`) must keep crash recovery working — a daemon killed mid-turn has to leave a state the next boot can reason about.

## Git

- Commit messages in English, imperative mood, lowercase first word. Format: `type: description`.
- One logical change per commit.
- Never skip hooks or commit secrets. `$HOME/.harness/` lives outside the repo.
- Never push without an explicit request.

## Don't touch without a reason

- The five principles.
- The workspace `Cargo.toml` pins.
- RPC namespaces already in production.
