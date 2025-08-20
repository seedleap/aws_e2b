use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use bollard::auth::DockerCredentials;
use clap::{Parser, Subcommand};
use log::{error, info, warn};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use xshell::{cmd, Shell};

use aws_config::meta::region::RegionProviderChain;
use aws_config::Region;
use aws_sdk_ecr as ecr;
use aws_sdk_sts as sts;

// Default values
const DEFAULT_MEMORY_MB: u32 = 4096;
const DEFAULT_CPU_COUNT: u32 = 4;
const DEFAULT_IMAGE: &str = "e2bdev/code-interpreter:latest";

#[derive(Parser, Debug)]
#[command(name = "aws_e2b", version, about = "CLI for self-hosting e2b on AWS")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Template related commands
    Template(TemplateCli),
}

#[derive(Parser, Debug)]
struct TemplateCli {
    #[command(subcommand)]
    command: TemplateCommands,
}

#[derive(Subcommand, Debug)]
enum TemplateCommands {
    /// Create a template
    Create(CreateArgs),
}

#[derive(Parser, Debug)]
struct CreateArgs {
    /// Path to aws_e2b.toml (optional). If not provided, the tool looks for ./aws_e2b.toml.
    #[arg(long = "config")]
    config_path: Option<PathBuf>,

    #[command(flatten)]
    e2b: E2bArgs,

    #[command(flatten)]
    docker: DockerArgs,
}

#[derive(Parser, Debug, Clone)]
struct E2bArgs {
    /// Override memory (MB)
    #[arg(long = "memory-mb", help_heading = "E2B")]
    memory_mb: Option<u32>,

    /// Override CPU cores
    #[arg(long = "cpu-count", help_heading = "E2B")]
    cpu_count: Option<u32>,

    /// Optional: startup command passed to server
    #[arg(long = "start-cmd", help_heading = "E2B")]
    start_cmd: Option<String>,

    /// Optional: readiness check command passed to server
    #[arg(long = "ready-cmd", help_heading = "E2B")]
    ready_cmd: Option<String>,

    /// Optional: template alias
    #[arg(long = "alias", help_heading = "E2B")]
    alias: Option<String>,
}

#[derive(Parser, Debug, Clone)]
struct DockerArgs {
    /// Path to a Dockerfile; its content will be used directly
    #[arg(long = "docker-file", help_heading = "DOCKER")]
    docker_file: Option<PathBuf>,

    /// Existing ECR image URI to use as base image
    #[arg(long = "ecr-image", help_heading = "DOCKER")]
    ecr_image: Option<String>,

    /// Base image to use when neither docker-file nor ecr-image is provided
    #[arg(long = "base-image", help_heading = "DOCKER")]
    base_image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CreateType {
    Default,
    Dockerfile,
    EcrImage,
}

#[derive(Debug, Default, Deserialize)]
struct E2bSection {
    #[serde(default)]
    memory_mb: Option<u32>,
    #[serde(default)]
    cpu_count: Option<u32>,
    #[serde(default)]
    start_cmd: Option<String>,
    #[serde(default)]
    ready_cmd: Option<String>,
    #[serde(default)]
    alias: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DockerSection {
    #[serde(default)]
    dockerfile: Option<String>,
    #[serde(default, rename = "ecr-image", alias = "ecr_image")]
    ecr_image: Option<String>,
    #[serde(
        default,
        rename = "dockerimage",
        alias = "docker_image",
        alias = "image"
    )]
    docker_image: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct E2bConfigToml {
    #[serde(default)]
    e2b: Option<E2bSection>,
    #[serde(default)]
    docker: Option<DockerSection>,
}

#[derive(Serialize)]
struct CreateTemplateRequest<'a> {
    dockerfile: &'a str,
    #[serde(rename = "memoryMB")]
    memory_mb: u32,
    #[serde(rename = "cpuCount")]
    cpu_count: u32,
    #[serde(rename = "startCmd", skip_serializing_if = "Option::is_none")]
    start_cmd: Option<&'a str>,
    #[serde(rename = "readyCmd", skip_serializing_if = "Option::is_none")]
    ready_cmd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<&'a str>,
}

#[derive(Deserialize)]
struct CreateTemplateResponse {
    #[serde(rename = "buildID")]
    build_id: String,
    #[serde(rename = "templateID")]
    template_id: String,
}

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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format(|fmt, record| {
            use std::io::Write;
            let ts = chrono::Local::now().format("%H:%M:%S");
            match record.level() {
                log::Level::Error | log::Level::Warn => {
                    writeln!(fmt, "{} [{}] {}", ts, record.level(), record.args())
                }
                _ => writeln!(fmt, "{} {}", ts, record.args()),
            }
        })
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Template(t) => match t.command {
            TemplateCommands::Create(args) => run_template_create(args).await,
        },
    }
}

async fn run_template_create(args: CreateArgs) -> Result<()> {
    // Load aws_e2b.toml (template-related only), optional
    let (e2b_cfg, e2b_dir) = load_e2b_toml(args.config_path.as_deref())?;

    // Extract template-related configuration
    let t_memory_mb = e2b_cfg.e2b.as_ref().and_then(|s| s.memory_mb);
    let t_cpu_count = e2b_cfg.e2b.as_ref().and_then(|s| s.cpu_count);
    let t_start_cmd = e2b_cfg.e2b.as_ref().and_then(|s| s.start_cmd.clone());
    let t_ready_cmd = e2b_cfg.e2b.as_ref().and_then(|s| s.ready_cmd.clone());
    let t_alias = e2b_cfg.e2b.as_ref().and_then(|s| s.alias.clone());

    let t_dockerfile = e2b_cfg.docker.as_ref().and_then(|s| s.dockerfile.clone());
    let t_ecr_image = e2b_cfg.docker.as_ref().and_then(|s| s.ecr_image.clone());
    let t_docker_image = e2b_cfg.docker.as_ref().and_then(|s| s.docker_image.clone());

    // Resolve values with precedence: CLI > aws_e2b.toml > defaults
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

    let (create_type, dockerfile_content, base_image_opt, dockerfile_path) = resolve_create_input(
        &args,
        t_dockerfile.as_deref(),
        t_ecr_image.as_deref(),
        t_docker_image.as_deref(),
        e2b_dir.as_deref(),
    )?;

    // Read user config from ~/.aws_e2b/config.toml
    let user_cfg = read_user_config().ok().flatten();

    // AWS region: ENV > user config ([aws])
    let user_aws_region = user_cfg
        .as_ref()
        .and_then(|c| c.aws.as_ref().and_then(|a| a.aws_region.clone()));
    let aws_region = env::var("AWS_REGION").ok().or(user_aws_region)
        .ok_or_else(|| anyhow!(
            "Missing AWS region: set AWS_REGION or configure [aws].aws_region in ~/.aws_e2b/config.toml"
        ))?;

    // e2b domain: ENV > user config ([e2b])
    let user_e2b_domain = user_cfg
        .as_ref()
        .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_domain.clone()));
    let e2b_domain = env::var("E2B_DOMAIN").ok().or(user_e2b_domain)
        .ok_or_else(|| anyhow!(
            "Missing e2b domain: set E2B_DOMAIN or configure [e2b].e2b_domain in ~/.aws_e2b/config.toml"
        ))?;

    // Access token: ENV > user config ([e2b])
    let user_token = user_cfg
        .as_ref()
        .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_access_token.clone()));
    let raw_access_token = env::var("E2B_ACCESS_TOKEN")
        .ok()
        .or(user_token)
        .ok_or_else(|| anyhow!(
            "Missing e2b access token: set E2B_ACCESS_TOKEN or configure [e2b].e2b_access_token in ~/.aws_e2b/config.toml"
        ))?;
    let e2b_access_token = format_bearer_token(&raw_access_token);

    let (build_id, template_id) = create_template(
        &e2b_domain,
        &e2b_access_token,
        &dockerfile_content,
        resolved_memory_mb,
        resolved_cpu,
        resolved_start_cmd.clone(),
        resolved_ready_cmd.clone(),
        resolved_alias.clone(),
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

    // Acquire ECR authorization (for push/pull)
    let (registry, docker_creds) = get_ecr_auth(&ecr_client).await?;

    // Ensure ECR repository exists
    create_ecr_repo_if_needed(&ecr_client, &template_id).await?;

    // Prepare base image
    let base_image = match create_type {
        CreateType::Dockerfile => {
            info!("Base image source: dockerfile (built locally)");
            let path = dockerfile_path.ok_or_else(|| anyhow!("Dockerfile path is required"))?;
            build_temp_image(&path).await?
        }
        CreateType::EcrImage => {
            let img = base_image_opt.expect("ECR image must be provided");
            info!("Base image source: ecr-image, value: {}", img);
            pull_docker_image(&img, Some(&docker_creds)).await?;
            img
        }
        CreateType::Default => {
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
    info!("Build completed!");

    Ok(())
}

fn load_e2b_toml(config_path: Option<&Path>) -> Result<(E2bConfigToml, Option<PathBuf>)> {
    // Prefer explicit --config path
    if let Some(p) = config_path {
        if p.exists() {
            return parse_e2b_toml_file(p).map(|cfg| (cfg, p.parent().map(|d| d.to_path_buf())));
        }
        return Err(anyhow!(
            "Specified config file does not exist: {}",
            p.display()
        ));
    }

    // Look for aws_e2b.toml in current working directory
    let cwd = std::env::current_dir()?;
    let path = cwd.join("aws_e2b.toml");
    if path.exists() {
        let cfg = parse_e2b_toml_file(&path)?;
        return Ok((cfg, path.parent().map(|d| d.to_path_buf())));
    }

    Ok((E2bConfigToml::default(), None))
}

fn parse_e2b_toml_file(path: &Path) -> Result<E2bConfigToml> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read TOML: {}", path.display()))?;
    let cfg: E2bConfigToml = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse TOML: {}", path.display()))?;
    Ok(cfg)
}

fn resolve_create_input(
    args: &CreateArgs,
    toml_dockerfile_path: Option<&str>,
    toml_ecr_image: Option<&str>,
    toml_docker_image: Option<&str>,
    toml_base_dir: Option<&Path>,
) -> Result<(CreateType, String, Option<String>, Option<PathBuf>)> {
    match (&args.docker.docker_file, &args.docker.ecr_image) {
        (Some(_), Some(_)) => Err(anyhow!(
            "`--docker-file` and `--ecr-image` cannot be used together"
        )),
        (Some(path), None) => {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read Dockerfile: {}", path.display()))?;
            info!("Using provided Dockerfile content to create template");
            Ok((CreateType::Dockerfile, content, None, Some(path.clone())))
        }
        (None, Some(image)) => {
            info!("Using provided ECR image to create template: {}", image);
            Ok((
                CreateType::EcrImage,
                format!("FROM {}", image),
                Some(image.clone()),
                None,
            ))
        }
        (None, None) => {
            // ecr image from TOML
            if let Some(image) = toml_ecr_image {
                info!("Using ecr-image from aws_e2b.toml: {}", image);
                return Ok((
                    CreateType::EcrImage,
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
                        "Failed to read dockerfile from TOML path: {}",
                        path.display()
                    )
                })?;
                info!(
                    "Using Dockerfile path from aws_e2b.toml: {}",
                    path.display()
                );
                return Ok((CreateType::Dockerfile, content, None, Some(path)));
            }
            // dockerimage from TOML as default base image
            if let Some(img) = toml_docker_image {
                info!(
                    "Using dockerimage from aws_e2b.toml as default base image: {}",
                    img
                );
                let dockerfile = format!("FROM {}", img);
                return Ok((CreateType::Default, dockerfile, None, None));
            }
            let default_dockerfile = format!("FROM {}", DEFAULT_IMAGE);
            info!(
                "Using default base image to create template: {}",
                DEFAULT_IMAGE
            );
            Ok((CreateType::Default, default_dockerfile, None, None))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct UserConfig {
    #[serde(default)]
    aws: Option<UserAwsSection>,
    #[serde(default)]
    e2b: Option<UserE2bSection>,
}

#[derive(Debug, Default, Deserialize)]
struct UserAwsSection {
    #[serde(default)]
    aws_region: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct UserE2bSection {
    #[serde(default, rename = "e2b_domain")]
    e2b_domain: Option<String>,
    #[serde(default, rename = "e2b_access_token")]
    e2b_access_token: Option<String>,
}

fn read_user_config() -> Result<Option<UserConfig>> {
    let home = env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Ok(None);
    }
    let path = Path::new(&home).join(".aws_e2b").join("config.toml");
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read user config: {}", path.display()))?;
    let cfg: UserConfig = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse user config: {}", path.display()))?;
    Ok(Some(cfg))
}

#[allow(clippy::too_many_arguments)] // Many parameters in the API call; keep them explicit
async fn create_template(
    e2b_domain: &str,
    access_token: &str,
    dockerfile: &str,
    memory_mb: u32,
    cpu_count: u32,
    start_cmd: Option<String>,
    ready_cmd: Option<String>,
    alias: Option<String>,
) -> Result<(String, String)> {
    let url = format!("https://api.{}/templates", e2b_domain);
    info!("Calling API to create template: {}", url);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(access_token)?);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let client = reqwest::Client::new();
    let body = CreateTemplateRequest {
        dockerfile,
        memory_mb,
        cpu_count,
        start_cmd: start_cmd.as_deref(),
        ready_cmd: ready_cmd.as_deref(),
        alias: alias.as_deref(),
    };
    let resp = client.post(url).headers(headers).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        error!("Create template failed HTTP {}: {}", status, text);
        return Err(anyhow!("Create template failed HTTP {}", status));
    }
    let parsed: CreateTemplateResponse = serde_json::from_str(&text)?;
    Ok((parsed.build_id, parsed.template_id))
}

async fn fetch_aws_account_id(sts_client: &sts::Client) -> Result<String> {
    let out = sts_client.get_caller_identity().send().await?;
    Ok(out.account().unwrap_or_default().to_string())
}

async fn get_ecr_auth(ecr_client: &ecr::Client) -> Result<(String, DockerCredentials)> {
    let resp = ecr_client.get_authorization_token().send().await?;
    let list = resp.authorization_data();
    let data = list
        .first()
        .ok_or_else(|| anyhow!("No ECR authorization data received"))?;
    let proxy_endpoint = data.proxy_endpoint().unwrap_or_default().to_string();
    let token_b64 = data.authorization_token().unwrap_or_default();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token_b64)
        .context("Failed to decode ECR authorization token")?;
    let decoded_str = String::from_utf8(decoded)?; // The decoded string looks like "AWS:password"
    let mut parts = decoded_str.splitn(2, ':');
    let username = parts.next().unwrap_or("AWS").to_string();
    let password = parts.next().unwrap_or("").to_string();

    let creds = DockerCredentials {
        username: Some(username),
        password: Some(password),
        serveraddress: Some(proxy_endpoint.clone()),
        ..Default::default()
    };
    Ok((proxy_endpoint, creds))
}

async fn create_ecr_repo_if_needed(ecr_client: &ecr::Client, template_id: &str) -> Result<()> {
    let repo_name = format!("e2bdev/base/{}", template_id);
    let res = ecr_client
        .create_repository()
        .repository_name(&repo_name)
        .send()
        .await;
    if let Err(err) = res {
        let msg = format!("{}", err);
        if !msg.contains("RepositoryAlreadyExistsException") {
            warn!("Failed to create ECR repository or already exists: {}", msg);
        }
    }
    Ok(())
}

async fn build_temp_image(dockerfile_path: &Path) -> Result<String> {
    // Run the build in the Dockerfile's directory so COPY instructions can access required files
    let tag = format!("temp_image_{}", chrono::Utc::now().timestamp());
    info!("Building image from Dockerfile: {}", tag);

    // Use the docker CLI to build for stable progress output
    let sh = Shell::new().context("failed to create shell")?;
    let context_dir = dockerfile_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // e2b currently does not support ARM images, so enforce linux/amd64 platform during build
    cmd!(
        sh,
        "docker build --platform linux/amd64 -t {tag} -f {dockerfile_path} {context_dir}"
    )
    .run()?;
    Ok(tag)
}

async fn pull_docker_image(image: &str, creds: Option<&DockerCredentials>) -> Result<()> {
    info!("Pulling image (docker CLI): {}", image);
    let sh = Shell::new().context("Failed to create shell")?;
    // Login if credentials are provided
    if let Some(c) = creds {
        if let (Some(user), Some(pass), Some(server)) = (
            c.username.as_ref(),
            c.password.as_ref(),
            c.serveraddress.as_ref(),
        ) {
            cmd!(sh, "docker login {server} -u {user} --password-stdin")
                .stdin(pass)
                .run()?;
        }
    }
    cmd!(sh, "docker pull {image}").run()?;
    Ok(())
}

async fn tag_image(source: &str, target: &str) -> Result<()> {
    let sh = Shell::new().context("Failed to create shell")?;
    cmd!(sh, "docker tag {source} {target}").run()?;
    Ok(())
}

async fn push_image(target: &str, creds: &DockerCredentials) -> Result<()> {
    info!("Pushing image (docker CLI): {}", target);
    let sh = Shell::new().context("Failed to create shell")?;
    if let (Some(user), Some(pass), Some(server)) = (
        creds.username.as_ref(),
        creds.password.as_ref(),
        creds.serveraddress.as_ref(),
    ) {
        cmd!(sh, "docker login {server} -u {user} --password-stdin")
            .stdin(pass)
            .run()?;
    }
    cmd!(sh, "docker push {target}").run()?;
    Ok(())
}

async fn notify_build_complete(
    e2b_domain: &str,
    access_token: &str,
    template_id: &str,
    build_id: &str,
) -> Result<()> {
    let url = format!(
        "https://api.{}/templates/{}/builds/{}",
        e2b_domain, template_id, build_id
    );
    info!("Notify API build complete: {}", url);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(access_token)?);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let client = reqwest::Client::new();
    let resp = client.post(url).headers(headers).send().await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        error!("Notify build complete failed HTTP {}: {}", status, text);
        return Err(anyhow!("Notify build complete failed HTTP {}", status));
    }
    info!("Notify response: {}", text);
    Ok(())
}

#[derive(Deserialize)]
struct StatusResp {
    status: String,
}

async fn poll_build_status_until_done(
    e2b_domain: &str,
    access_token: &str,
    template_id: &str,
    build_id: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.{}/templates/{}/builds/{}/status",
        e2b_domain, template_id, build_id
    );
    loop {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(access_token)?);
        let resp = client.get(&url).headers(headers).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Query status failed HTTP {}: {}", status, text));
        }
        let status_value = serde_json::from_str::<StatusResp>(&text).or_else(|_| {
            serde_json::from_str::<Value>(&text).map(|v| StatusResp {
                status: v
                    .get("status")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })?;
        info!("Current build status: {}", status_value.status);
        if status_value.status != "building" {
            info!("Final status: {}", status_value.status);
            break;
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
    Ok(())
}
