# Contributing

Contributions are welcome. Keep changes focused, add tests for behavior, and run the Docker quality gate documented in [README.md](README.md) before submitting a change.

Unless explicitly stated otherwise, by submitting a contribution you agree to license it under the project's GNU Affero General Public License, version 3 or later (`AGPL-3.0-or-later`). Do not submit code or assets that you do not have the right to contribute under compatible terms.

Performance-sensitive changes should include measurements against a representative workload. The release-1 acceptance workload is four simultaneously visible 100 MP images on the baseline system in [DESIGN.md](DESIGN.md).

## Docker quality checks

The development image contains Rust, Clippy, rustfmt, and `cargo-llvm-cov`; no host Rust installation is required.

```sh
docker compose build dev
docker compose run --rm dev cargo fmt --all --check
docker compose run --rm dev cargo test --workspace
docker compose run --rm dev cargo clippy --workspace --all-targets -- -D warnings
docker compose run --rm dev cargo llvm-cov --workspace --all-targets --summary-only
```
