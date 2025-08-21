# AGENTS

This repository maintains the `aws_e2b` CLI, which helps users interact with e2b that is self-hosted on AWS.

## Contribution Guidelines

- All documentation and code comments must be written in English with clear wording. Do not use abbreviations.
- Variable and function names should be explicit and avoid shorthand.
- Follow Rust community best practices for code style.
- Before committing, run the following commands to stay consistent with GitHub CI:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
- All commits must pass GitHub CI.
- Update related documentation with every code change.
