<p align="center">
  <img src="assets/banner.svg" alt="HARNESS" width="560" />
</p>

<p align="center">
  A persistent, multi-agent AI coding daemon.
</p>

<p align="center">
  <a href="https://github.com/jbj338033/harness/actions/workflows/ci.yml"><img alt="ci" src="https://img.shields.io/github/actions/workflow/status/jbj338033/harness/ci.yml?branch=main&amp;style=flat-square&amp;label=ci&amp;color=b7a7eb"></a>
  <a href="https://github.com/jbj338033/harness/releases/latest"><img alt="release" src="https://img.shields.io/github/v/release/jbj338033/harness?style=flat-square&amp;color=b7a7eb"></a>
  <a href="LICENSE"><img alt="license" src="https://img.shields.io/badge/license-MIT-b7a7eb?style=flat-square"></a>
</p>

---

Harness runs as a long-lived host process that serves clients (terminal, web, phone) over one JSON-RPC surface. Sessions outlive any single window — close the TUI, come back tomorrow from your phone, pick up mid-turn. Workers run in isolated git worktrees so the orchestrator spawns them in parallel without stepping on each other.

Provider-agnostic (Anthropic, OpenAI, Google, Ollama) with a first-class tool belt: content-hash file edits, sandboxed shell, web fetch, CDP browser, LSP, accessibility-aware screen control, and an MCP client for out-of-tree tools.

## Install

On the host machine (runs the daemon):

```sh
curl -sSL https://raw.githubusercontent.com/jbj338033/harness/main/install.sh | sh
```

Installs both binaries in `/usr/local/bin` and registers a launchd plist (macOS) or systemd unit (Linux).

On a client-only device (connects to a remote host):

```sh
curl -sSL https://raw.githubusercontent.com/jbj338033/harness/main/install.sh | sh -s -- --client-only
```

Installs the `harness` CLI only — no daemon, no service.

## Quick start

```sh
harness auth login          # pick a provider, paste a key or OAuth
harness                     # open the inline TUI
harness doctor              # end-to-end health check
```

Pair another device:

```sh
harness pair                                    # on the host
harness connect wss://<host>:8384 <code> <name> # on the new device
```

## Contributing

```sh
cargo fmt --all
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Keep changes focused, include a test, and match the house style in [`CLAUDE.md`](CLAUDE.md).

## License

MIT
