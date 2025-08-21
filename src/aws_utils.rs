use anyhow::{Context, Result};
use aws_sdk_ecr as ecr;
use aws_sdk_sts as sts;
use base64::Engine;
use bollard::auth::DockerCredentials;
use log::info;

/// Retrieve the AWS account identifier of the current caller
pub async fn fetch_aws_account_id(sts_client: &sts::Client) -> Result<String> {
    let resp = sts_client.get_caller_identity().send().await?;
    Ok(resp.account.as_deref().unwrap_or("").to_string())
}

/// Retrieve authentication information from Amazon ECR
pub async fn get_ecr_auth(ecr_client: &ecr::Client) -> Result<(String, DockerCredentials)> {
    let auth = ecr_client.get_authorization_token().send().await?;
    let data = auth
        .authorization_data
        .unwrap_or_default()
        .into_iter()
        .next()
        .context("failed to obtain ECR authorization data")?;
    let token = data
        .authorization_token
        .context("missing authorization token")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(token)
        .context("failed to decode ECR authorization token")?;
    let decoded_str =
        String::from_utf8(decoded).context("failed to decode authorization token as UTF-8")?;
    let mut parts = decoded_str.split(':');
    let username = parts.next().unwrap_or("").to_string();
    let password = parts.next().unwrap_or("").to_string();
    let server = data.proxy_endpoint.unwrap_or_default();
    let creds = DockerCredentials {
        username: Some(username),
        password: Some(password),
        serveraddress: Some(server.clone()),
        ..Default::default()
    };
    Ok((server, creds))
}

/// Create the repository if it does not already exist
pub async fn create_ecr_repo_if_needed(ecr_client: &ecr::Client, template_id: &str) -> Result<()> {
    let repo_name = format!("e2bdev/base/{}", template_id);
    if ecr_client
        .describe_repositories()
        .repository_names(repo_name.clone())
        .send()
        .await
        .is_err()
    {
        info!("Creating ECR repository: {}", repo_name);
        ecr_client
            .create_repository()
            .repository_name(repo_name)
            .send()
            .await?;
    }
    Ok(())
}
