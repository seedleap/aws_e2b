use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{anyhow, Context, Result};
use aws_config::meta::region::RegionProviderChain;
use aws_config::Region;
use aws_sdk_ecr as ecr;
use aws_sdk_sts as sts;
use log::info;

use crate::args::BuildArgs;
use crate::aws_utils::{create_ecr_repo_if_needed, fetch_aws_account_id, get_ecr_auth};
use crate::config::{load_e2b_toml, read_user_config};
use crate::docker_utils::{build_temp_image, pull_docker_image, push_image, tag_image};
use crate::e2b_api::{build_template, notify_build_complete, poll_build_status_until_done};

/// Default configuration
const DEFAULT_MEMORY_MB: u32 = 4096;
const DEFAULT_CPU_COUNT: u32 = 4;
const DEFAULT_IMAGE: &str = "e2bdev/code-interpreter:latest";

/// Build method
#[derive(Debug, Clone, PartialEq, Eq)]
enum BuildType {
    Default,
    Dockerfile,
    EcrImage,
}

/// Add a "Bearer" prefix to the access token if it is missing
fn format_bearer_token(token: &str) -> String {
    let trimmed = token.trim();
    let has_prefix = trimmed
        .get(0..7)
        .map(|p| p.eq_ignore_ascii_case("bearer "))
        .unwrap_or(false);
    if has_prefix {
        trimmed.to_string()
    } else {
        format!("Bearer {}", trimmed)
    }
}

/// Core logic for the `template build` subcommand
pub async fn run_template_build(args: BuildArgs) -> Result<()> {
    // Load optional aws_e2b.toml
    let (e2b_cfg, e2b_dir) = load_e2b_toml(args.config_path.as_deref())?;

    // Extract template-related parameters from configuration
    let t_memory_mb = e2b_cfg.e2b.as_ref().and_then(|s| s.memory_mb);
    let t_cpu_count = e2b_cfg.e2b.as_ref().and_then(|s| s.cpu_count);
    let t_start_cmd = e2b_cfg.e2b.as_ref().and_then(|s| s.start_cmd.clone());
    let t_ready_cmd = e2b_cfg.e2b.as_ref().and_then(|s| s.ready_cmd.clone());
    let t_alias = e2b_cfg.e2b.as_ref().and_then(|s| s.alias.clone());
    let t_template_id = e2b_cfg.e2b.as_ref().and_then(|s| s.template_id.clone());

    let t_dockerfile = e2b_cfg.docker.as_ref().and_then(|s| s.dockerfile.clone());
    let t_ecr_image = e2b_cfg.docker.as_ref().and_then(|s| s.ecr_image.clone());
    let t_docker_image = e2b_cfg.docker.as_ref().and_then(|s| s.docker_image.clone());

    // Parameter priority: command line > aws_e2b.toml > defaults
    let resolved_memory_mb = args
        .e2b
        .memory_mb
        .or(t_memory_mb)
        .unwrap_or(DEFAULT_MEMORY_MB);
    let resolved_cpu = args
        .e2b
        .cpu_count
        .or(t_cpu_count)
        .unwrap_or(DEFAULT_CPU_COUNT);
    let resolved_start_cmd = args.e2b.start_cmd.clone().or(t_start_cmd);
    let resolved_ready_cmd = args.e2b.ready_cmd.clone().or(t_ready_cmd);
    let resolved_alias = args.e2b.alias.clone().or(t_alias);
    let resolved_template_id = args.e2b.template_id.clone().or(t_template_id);

    let (build_type, dockerfile_content, base_image_opt, dockerfile_path) = resolve_build_input(
        &args,
        t_dockerfile.as_deref(),
        t_ecr_image.as_deref(),
        t_docker_image.as_deref(),
        e2b_dir.as_deref(),
    )?;

    // Read user-level configuration ~/.aws_e2b/config.toml
    let user_cfg = read_user_config().ok().flatten();

    // AWS region priority: environment variable > user configuration
    let user_aws_region = user_cfg
        .as_ref()
        .and_then(|c| c.aws.as_ref().and_then(|a| a.aws_region.clone()));
    let aws_region = env::var("AWS_REGION").ok().or(user_aws_region).ok_or_else(|| {
        anyhow!(
            "Missing AWS region: set AWS_REGION or configure [aws].aws_region in ~/.aws_e2b/config.toml"
        )
    })?;

    // e2b domain priority: environment variable > user configuration
    let user_e2b_domain = user_cfg
        .as_ref()
        .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_domain.clone()));
    let e2b_domain = env::var("E2B_DOMAIN").ok().or(user_e2b_domain).ok_or_else(|| {
        anyhow!(
            "Missing e2b domain: set E2B_DOMAIN or configure [e2b].e2b_domain in ~/.aws_e2b/config.toml"
        )
    })?;

    // Access token priority: environment variable > user configuration
    let user_token = user_cfg
        .as_ref()
        .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_access_token.clone()));
    let raw_access_token = env::var("E2B_ACCESS_TOKEN").ok().or(user_token).ok_or_else(|| {
        anyhow!(
            "Missing e2b access token: set E2B_ACCESS_TOKEN or configure [e2b].e2b_access_token in ~/.aws_e2b/config.toml"
        )
    })?;
    let e2b_access_token = format_bearer_token(&raw_access_token);

    if let Some(ref tid) = resolved_template_id {
        info!("Using existing template ID: {}", tid);
    }

    let (build_id, template_id) = build_template(
        &e2b_domain,
        &e2b_access_token,
        &dockerfile_content,
        resolved_memory_mb,
        resolved_cpu,
        resolved_start_cmd.clone(),
        resolved_ready_cmd.clone(),
        resolved_alias.clone(),
        resolved_template_id.clone(),
    )
    .await?;
    info!("buildID: {}", build_id);
    info!("templateID: {}", template_id);

    // Initialize AWS SDK
    let region = Region::new(aws_region.clone());
    let region_provider = RegionProviderChain::first_try(region);
    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;
    let sts_client = sts::Client::new(&shared_config);
    let ecr_client = ecr::Client::new(&shared_config);

    let aws_account_id = fetch_aws_account_id(&sts_client).await?;
    info!("AWS Account ID: {}", aws_account_id);

    let (registry, docker_creds) = get_ecr_auth(&ecr_client).await?;

    create_ecr_repo_if_needed(&ecr_client, &template_id).await?;

    // Prepare the base image
    let base_image = match build_type {
        BuildType::Dockerfile => {
            info!("Base image source: local build from Dockerfile");
            let path = dockerfile_path.ok_or_else(|| anyhow!("missing Dockerfile path"))?;
            build_temp_image(&path).await?
        }
        BuildType::EcrImage => {
            let img = base_image_opt.expect("ECR image must be provided");
            info!("Base image source: ECR image {}", img);
            pull_docker_image(&img, Some(&docker_creds)).await?;
            img
        }
        BuildType::Default => {
            let chosen = args
                .docker
                .base_image
                .clone()
                .or(t_docker_image)
                .unwrap_or_else(|| DEFAULT_IMAGE.to_string());
            info!("Base image: {}", chosen);
            pull_docker_image(&chosen, None).await?;
            chosen
        }
    };

    let ecr_target_tag = format!(
        "{}/e2bdev/base/{}:{}",
        registry.trim_start_matches("https://"),
        template_id,
        build_id
    );

    tag_image(&base_image, &ecr_target_tag).await?;
    push_image(&ecr_target_tag, &docker_creds).await?;
    info!("Pushed base image to ECR: {}", ecr_target_tag);

    notify_build_complete(&e2b_domain, &e2b_access_token, &template_id, &build_id).await?;

    poll_build_status_until_done(&e2b_domain, &e2b_access_token, &template_id, &build_id).await?;
    info!("Build completed");

    Ok(())
}

/// Determine build method based on command line arguments and configuration
fn resolve_build_input(
    args: &BuildArgs,
    toml_dockerfile_path: Option<&str>,
    toml_ecr_image: Option<&str>,
    toml_docker_image: Option<&str>,
    toml_base_dir: Option<&Path>,
) -> Result<(BuildType, String, Option<String>, Option<PathBuf>)> {
    match (&args.docker.docker_file, &args.docker.ecr_image) {
        (Some(_), Some(_)) => Err(anyhow!(
            "The `--docker-file` and `--ecr-image` options cannot be used together",
        )),
        (Some(path), None) => {
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read Dockerfile: {}", path.display()))?;
            info!("Building with provided Dockerfile content");
            Ok((BuildType::Dockerfile, content, None, Some(path.clone())))
        }
        (None, Some(image)) => {
            info!("Building with provided ECR image: {}", image);
            Ok((
                BuildType::EcrImage,
                format!("FROM {}", image),
                Some(image.clone()),
                None,
            ))
        }
        (None, None) => {
            if let Some(image) = toml_ecr_image {
                info!("Using ecr-image from aws_e2b.toml: {}", image);
                return Ok((
                    BuildType::EcrImage,
                    format!("FROM {}", image),
                    Some(image.to_string()),
                    None,
                ));
            }
            if let Some(dockerfile_path) = toml_dockerfile_path {
                let raw = Path::new(dockerfile_path);
                let path = if raw.is_relative() {
                    if let Some(base) = toml_base_dir {
                        base.join(raw)
                    } else {
                        raw.to_path_buf()
                    }
                } else {
                    raw.to_path_buf()
                };
                let content = fs::read_to_string(&path).with_context(|| {
                    format!(
                        "failed to read Dockerfile from configuration: {}",
                        path.display()
                    )
                })?;
                info!("Reading Dockerfile from aws_e2b.toml: {}", path.display());
                return Ok((BuildType::Dockerfile, content, None, Some(path)));
            }
            if let Some(img) = toml_docker_image {
                info!(
                    "Using dockerimage from aws_e2b.toml as default base image: {}",
                    img
                );
                let dockerfile = format!("FROM {}", img);
                return Ok((BuildType::Default, dockerfile, None, None));
            }
            let default_dockerfile = format!("FROM {}", DEFAULT_IMAGE);
            info!("Using default base image: {}", DEFAULT_IMAGE);
            Ok((BuildType::Default, default_dockerfile, None, None))
        }
    }
}
