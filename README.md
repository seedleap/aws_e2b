# aws_e2b

A command-line tool for building e2b templates and pushing base images to Amazon Elastic Container Registry (ECR).

## Features
- Build templates through the e2b application programming interface.
- Use a local Dockerfile, an existing ECR image, or a default image as the base.
- When a local Dockerfile is provided, `docker build` runs in its directory so `COPY` instructions can access local files.
- `docker build` runs with `--platform linux/amd64` to ensure x86 compatibility.
- Push the base image to Amazon ECR.
- Notify the API after the image is pushed and poll for the build status.

## Installation
From source:
```bash
cargo install --path .
```

From Git:
```bash
cargo install --git https://github.com/seedleap/aws_e2b
```

## Usage example
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

## Forwarding other commands
`aws_e2b` implements only `template build`. All other subcommands are forwarded to the official `e2b` command-line interface. During forwarding, `E2B_DOMAIN` and `E2B_ACCESS_TOKEN` are set automatically if they exist in environment variables or user configuration.
```bash
aws_e2b template list  # Equivalent to e2b template list
```
The `auth` subcommand is not supported and will not be forwarded.

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
# template_id = "j4iitty8yuz06tfnm5du" # Build using an existing template ID

[docker]
# dockerfile = "./Dockerfile"
# ecr-image = "123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag"
# dockerimage = "e2bdev/code-interpreter:latest"
```

User configuration `~/.aws_e2b/config.toml`:
```toml
[aws]
aws_region = "us-east-1"

[e2b]
e2b_domain = "e2b.dev"
e2b_access_token = "YOUR_TOKEN" # or set environment variable E2B_ACCESS_TOKEN
```

## Argument priority
- Memory, CPU, start command, ready command, alias: command line > `aws_e2b.toml` > defaults
- template_id: command line > `aws_e2b.toml` > create new template
- AWS region: environment variable `AWS_REGION` > `~/.aws_e2b/config.toml` `[aws].aws_region`
- e2b domain: environment variable `E2B_DOMAIN` > `~/.aws_e2b/config.toml` `[e2b].e2b_domain`
- Access token: environment variable `E2B_ACCESS_TOKEN` > `~/.aws_e2b/config.toml` `[e2b].e2b_access_token`

## License
MIT or Apache-2.0

## Development
GitHub Actions runs formatting, Clippy, build, and tests. To reproduce locally:
```bash
rustup component add clippy rustfmt
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --verbose
cargo test --verbose
```
