# aws_e2b

A CLI to create e2b templates and push base images to AWS ECR for self-hosted e2b.

## Features
- Create templates via e2b API
- Build or select base images (Dockerfile, ECR image, or default image)
- When building a local Dockerfile, run `docker build` in the Dockerfile directory to ensure `COPY` instructions can access files
- Push base images to AWS ECR
- Report build completion and poll build status

## Install
From source:
```bash
cargo install --path .
```

## Usage
```bash
aws_e2b template create \
  --config ./aws_e2b.toml \
  --memory-mb 4096 \
  --cpu-count 4 \
  --start-cmd "your-start-cmd" \
  --ready-cmd "your-ready-cmd" \
  --alias my-template \
  --docker-file ./Dockerfile
```

Or use an existing ECR image:
```bash
aws_e2b template create --config ./aws_e2b.toml --ecr-image 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag
```

## Configuration
- Template configuration file: `aws_e2b.toml`
- User configuration: `~/.aws_e2b/config.toml`

Example `aws_e2b.toml`:
```toml
[e2b]
memory_mb = 4096
cpu_count = 4
# start_cmd = "/root/.jupyter/start-up.sh"
# ready_cmd = "curl -sf http://127.0.0.1:8888/health"
# alias = "ci-python"

[docker]
# dockerfile = "./Dockerfile"
# ecr-image = "123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag"
# dockerimage = "e2bdev/code-interpreter:latest"
```

User config `~/.aws_e2b/config.toml`:
```toml
[aws]
aws_region = "us-east-1"

[e2b]
e2b_domain = "e2b.dev"
e2b_access_token = "YOUR_TOKEN" # or set env E2B_ACCESS_TOKEN
```

## Precedence
- Memory/CPU/start/ready/alias: CLI > `aws_e2b.toml` > defaults
- AWS Region: env `AWS_REGION` > `~/.aws_e2b/config.toml` `[aws].aws_region`
- e2b Domain: env `E2B_DOMAIN` > `~/.aws_e2b/config.toml` `[e2b].e2b_domain`
- Access Token: env `E2B_ACCESS_TOKEN` > `~/.aws_e2b/config.toml` `[e2b].e2b_access_token`

## License
MIT or Apache-2.0
