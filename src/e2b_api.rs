use anyhow::{anyhow, Result};
use log::{error, info};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Call the e2b API to create or update a template
#[allow(clippy::too_many_arguments)]
pub async fn build_template(
    e2b_domain: &str,
    access_token: &str,
    dockerfile: &str,
    memory_mb: u32,
    cpu_count: u32,
    start_cmd: Option<String>,
    ready_cmd: Option<String>,
    alias: Option<String>,
    template_id: Option<String>,
) -> Result<(String, String)> {
    let url = if let Some(ref tid) = template_id {
        format!("https://api.{}/templates/{}", e2b_domain, tid)
    } else {
        format!("https://api.{}/templates", e2b_domain)
    };
    info!("Calling API to build template: {}", url);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(access_token)?);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = serde_json::json!({
        "dockerfile": dockerfile,
        "memoryMb": memory_mb,
        "cpuCount": cpu_count,
        "startCmd": start_cmd,
        "readyCmd": ready_cmd,
        "alias": alias,
        "templateID": template_id
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        error!("Failed to build template HTTP {}: {}", status, text);
        return Err(anyhow!("failed to build template HTTP {}", status));
    }
    let value: Value = serde_json::from_str(&text)?;
    let build_id = value
        .get("buildID")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let template_id = value
        .get("templateID")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((build_id, template_id))
}

/// Notify the API that the build has finished after the image is pushed
pub async fn notify_build_complete(
    e2b_domain: &str,
    access_token: &str,
    template_id: &str,
    build_id: &str,
) -> Result<()> {
    let url = format!(
        "https://api.{}/templates/{}/builds/{}",
        e2b_domain, template_id, build_id
    );
    info!("Notifying API that build is complete: {}", url);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(access_token)?);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let client = reqwest::Client::new();
    let resp = client.post(url).headers(headers).send().await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        error!("Notification failed HTTP {}: {}", status, text);
        return Err(anyhow!("notification failed HTTP {}", status));
    }
    info!("Notification response: {}", text);
    Ok(())
}

#[derive(Deserialize)]
struct StatusResp {
    status: String,
}

/// Poll build status until completion
pub async fn poll_build_status_until_done(
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
            return Err(anyhow!("failed to query status HTTP {}: {}", status, text));
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
