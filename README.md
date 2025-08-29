# aws_e2b

Command-line tool for interacting with e2b templates and sandboxes in a self-hosted AWS environment.

## Features
- Build templates through the e2b API.
- Use a local Dockerfile, an existing ECR image, or a default image as the base.
- When a Dockerfile is provided, `docker build` runs in its directory so `COPY` instructions can access local files.
- `docker build` runs with `--platform linux/amd64` to ensure x86 compatibility.
- Push the base image to Amazon ECR.
- Notify the e2b API and poll for the build status.

## Installation
From source:
```bash
cargo install --path .
```

From Git:
```bash
cargo install --git https://github.com/seedleap/aws_e2b
```

## Usage examples
Run `aws_e2b --help` to view all available commands and options.

Build a template:
```bash
aws_e2b template build \
  --config ./aws_e2b.toml \
  --memory-mb 4096 \
  --cpu-count 4 \
  --start-cmd "your-start-cmd" \
  --ready-cmd "your-ready-cmd" \
  --alias my-template \
  --docker-file ./Dockerfile
```

Use an existing ECR image:
```bash
aws_e2b template build --config ./aws_e2b.toml --ecr-image 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag
```

List templates for a team:
```bash
aws_e2b template list --team YOUR_TEAM_ID
```
If `--team` is omitted, the team identifier is read from `[e2b].e2b_team_id` in `~/.aws_e2b/config.toml`.

## Command forwarding rules
- `template build` and `template list` are implemented by this tool.
- `sandbox` subcommands are forwarded to the official `e2b` CLI.
- `aws_e2b` verifies that the official `e2b` CLI is installed and instructs installation from <https://e2b.dev/docs/cli> when it is missing.
- All other commands are unsupported.

## Configuration files
- Template configuration: `aws_e2b.toml`
- User configuration: `~/.aws_e2b/config.toml`

Example `aws_e2b.toml`:
```toml
[e2b]
memory_mb = 4096
cpu_count = 4
# start_cmd = "/root/.jupyter/start-up.sh"
# ready_cmd = "curl -sf http://127.0.0.1:8888/health"
# alias = "ci-python"
# template_id = "j4iitty8yuz06tfnm5du" # build using an existing template ID

[docker]
# dockerfile = "./Dockerfile"
# ecr-image = "123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag"
# base-image = "e2bdev/code-interpreter:latest"
```

User configuration `~/.aws_e2b/config.toml`:
```toml
[aws]
aws_region = "us-east-1"

[e2b]
e2b_domain = "e2b.dev"
e2b_access_token = "YOUR_TOKEN" # or set environment variable E2B_ACCESS_TOKEN
e2b_api_key = "YOUR_API_KEY"    # or set environment variable E2B_API_KEY
e2b_team_id = "YOUR_TEAM_ID"    # overridden by the --team argument
```

## Parameter precedence
- Memory, CPU, `start_cmd`, `ready_cmd`, `alias`: CLI > `aws_e2b.toml` > default value
- `template_id`: CLI > `aws_e2b.toml` > create new template
- AWS region: environment variable `AWS_REGION` > user config `[aws].aws_region`
- e2b domain: environment variable `E2B_DOMAIN` > user config `[e2b].e2b_domain`
- access token: environment variable `E2B_ACCESS_TOKEN` > user config `[e2b].e2b_access_token`
- API key: environment variable `E2B_API_KEY` > user config `[e2b].e2b_api_key`
- team identifier: `--team` > user config `[e2b].e2b_team_id`

## License
MIT or Apache-2.0

## Development
GitHub Actions runs format checks, Clippy, build, and tests. Reproduce locally:
```bash
rustup component add clippy rustfmt
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --verbose
cargo test --verbose
```
