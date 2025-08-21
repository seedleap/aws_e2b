use anyhow::{Context, Result};
use bollard::auth::DockerCredentials;
use log::info;
use std::path::Path;
use xshell::{cmd, Shell};

/// Build a temporary image to upload
pub async fn build_temp_image(dockerfile_path: &Path) -> Result<String> {
    let tag = format!("aws-e2b-temp:{}", chrono::Utc::now().timestamp());
    info!("Building temporary image: {}", tag);
    let sh = Shell::new().context("failed to create shell")?;
    let context_dir = dockerfile_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // e2b does not support ARM, so force linux/amd64
    cmd!(
        sh,
        "docker build --platform linux/amd64 -t {tag} -f {dockerfile_path} {context_dir}"
    )
    .run()?;
    Ok(tag)
}

/// Pull an image through the docker command-line interface with optional credentials
pub async fn pull_docker_image(image: &str, creds: Option<&DockerCredentials>) -> Result<()> {
    info!("Pulling image: {}", image);
    let sh = Shell::new().context("failed to create shell")?;
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

/// Tag an image
pub async fn tag_image(source: &str, target: &str) -> Result<()> {
    let sh = Shell::new().context("failed to create shell")?;
    cmd!(sh, "docker tag {source} {target}").run()?;
    Ok(())
}

/// Push an image to a remote registry
pub async fn push_image(target: &str, creds: &DockerCredentials) -> Result<()> {
    info!("Pushing image: {}", target);
    let sh = Shell::new().context("failed to create shell")?;
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
