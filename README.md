# aws_e2b

一个 CLI 工具，用于在 AWS 上自托管 e2b 时创建模板并将基础镜像推送到 AWS ECR。

## 功能
- 通过 e2b API 创建模板
- 构建或选择基础镜像（Dockerfile、ECR 镜像或默认镜像）
- 构建本地 Dockerfile 时会在其目录执行 `docker build`，确保 `COPY` 指令能够访问文件
- 将基础镜像推送到 AWS ECR
- 通知构建完成并轮询构建状态

## 安装
从源码安装：
```bash
cargo install --path .
```

## 使用
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

或者使用已有的 ECR 镜像：
```bash
aws_e2b template create --config ./aws_e2b.toml --ecr-image 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-image:tag
```

## 配置
- 模板配置文件：`aws_e2b.toml`
- 用户配置：`~/.aws_e2b/config.toml`

示例 `aws_e2b.toml`：
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

用户配置 `~/.aws_e2b/config.toml`：
```toml
[aws]
aws_region = "us-east-1"

[e2b]
e2b_domain = "e2b.dev"
e2b_access_token = "YOUR_TOKEN" # 或者设置环境变量 E2B_ACCESS_TOKEN
```

## 优先级
- 内存/CPU/启动命令/就绪命令/别名：CLI > `aws_e2b.toml` > 默认值
- AWS Region：环境变量 `AWS_REGION` > `~/.aws_e2b/config.toml` 中 `[aws].aws_region`
- e2b Domain：环境变量 `E2B_DOMAIN` > `~/.aws_e2b/config.toml` 中 `[e2b].e2b_domain`
- Access Token：环境变量 `E2B_ACCESS_TOKEN` > `~/.aws_e2b/config.toml` 中 `[e2b].e2b_access_token`

## 许可证
MIT 或 Apache-2.0

