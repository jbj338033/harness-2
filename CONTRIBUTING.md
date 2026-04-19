# Contributing

Thanks for your interest in harness.

## Development setup

```sh
rustup toolchain install stable
cargo build --workspace
```

See [`CLAUDE.md`](CLAUDE.md) for house rules (no workarounds, no comment noise, provider-specific options via namespaced structs, etc.).

## Quality gates

Every PR must pass these four, matching what CI enforces:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check          # requires: cargo install cargo-deny --locked
```

## Commit style

`type: description` — lowercase, imperative, one line. Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `chore`, `ci`, `style`. Breaking changes use `!:` (e.g. `feat!: rename RPC namespace`).

Only `feat`, `fix`, `perf`, and breaking changes land in `CHANGELOG.md`. Keep noise commits (`chore`, `test`, `ci`, `docs`, `refactor`, `style`) as their own units so they stay filtered out.

## Pull requests

- One logical change per PR.
- Link any related issue.
- Update docs if behavior changes.
- Do not skip hooks or commit secrets.

## Release procedure (maintainers)

```sh
# 1. Bump workspace.package.version in Cargo.toml
# 2. Regenerate CHANGELOG.md
cargo install git-cliff --locked            # one-time
git cliff --tag vX.Y.Z --output CHANGELOG.md

# 3. Commit, tag, push
git commit -am "release: vX.Y.Z"
git tag vX.Y.Z
git push --follow-tags
```

The `release.yml` workflow builds the four-target matrix and creates the GitHub Release with tarballs, `SHA256SUMS`, and the extracted changelog section.
