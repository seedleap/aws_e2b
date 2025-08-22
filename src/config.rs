use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// Configuration for the `[e2b]` section in `aws_e2b.toml`
#[derive(Debug, Default, Deserialize)]
pub struct E2bSection {
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub cpu_count: Option<u32>,
    #[serde(default)]
    pub start_cmd: Option<String>,
    #[serde(default)]
    pub ready_cmd: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default, rename = "template_id", alias = "templateID")]
    pub template_id: Option<String>,
}

/// Configuration for the `[docker]` section in `aws_e2b.toml`
#[derive(Debug, Default, Deserialize)]
pub struct DockerSection {
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default, rename = "ecr-image", alias = "ecr_image")]
    pub ecr_image: Option<String>,
    #[serde(
        default,
        rename = "dockerimage",
        alias = "docker_image",
        alias = "image"
    )]
    pub docker_image: Option<String>,
}

/// Full structure of `aws_e2b.toml`
#[derive(Debug, Default, Deserialize)]
pub struct E2bConfigToml {
    #[serde(default)]
    pub e2b: Option<E2bSection>,
    #[serde(default)]
    pub docker: Option<DockerSection>,
}

/// User-level configuration in `~/.aws_e2b/config.toml`
#[derive(Debug, Default, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub aws: Option<UserAwsSection>,
    #[serde(default)]
    pub e2b: Option<UserE2bSection>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UserAwsSection {
    #[serde(default)]
    pub aws_region: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UserE2bSection {
    #[serde(default, rename = "e2b_domain")]
    pub e2b_domain: Option<String>,
    #[serde(default, rename = "e2b_access_token")]
    pub e2b_access_token: Option<String>,
    /// e2b API key for the user
    #[serde(default, rename = "e2b_api_key")]
    pub e2b_api_key: Option<String>,
    /// e2b team identifier for the user
    #[serde(default, rename = "e2b_team_id")]
    pub e2b_team_id: Option<String>,
}

/// Load `aws_e2b.toml` and return the configuration and its directory
pub fn load_e2b_toml(config_path: Option<&Path>) -> Result<(E2bConfigToml, Option<PathBuf>)> {
    if let Some(p) = config_path {
        if p.exists() {
            return parse_e2b_toml_file(p).map(|cfg| (cfg, p.parent().map(|d| d.to_path_buf())));
        }
        return Err(anyhow!(
            "Specified configuration file does not exist: {}",
            p.display()
        ));
    }

    let cwd = std::env::current_dir()?;
    let path = cwd.join("aws_e2b.toml");
    if path.exists() {
        let cfg = parse_e2b_toml_file(&path)?;
        return Ok((cfg, path.parent().map(|d| d.to_path_buf())));
    }

    Ok((E2bConfigToml::default(), None))
}

/// Parse `aws_e2b.toml` from disk
pub fn parse_e2b_toml_file(path: &Path) -> Result<E2bConfigToml> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read TOML: {}", path.display()))?;
    let cfg: E2bConfigToml = toml::from_str(&raw)
        .with_context(|| format!("failed to parse TOML: {}", path.display()))?;
    Ok(cfg)
}

/// Read user configuration `~/.aws_e2b/config.toml`
pub fn read_user_config() -> Result<Option<UserConfig>> {
    let home = env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return Ok(None);
    }
    let path = Path::new(&home).join(".aws_e2b").join("config.toml");
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("failed to read user configuration: {}", path.display()))?;
    let cfg: UserConfig = toml::from_str(&raw)
        .with_context(|| format!("failed to parse user configuration: {}", path.display()))?;
    Ok(Some(cfg))
}
