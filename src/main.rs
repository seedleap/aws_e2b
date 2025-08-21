use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::error;
use std::env;

mod args;
mod aws_utils;
mod build;
mod config;
mod docker_utils;
mod e2b_api;

use args::BuildArgs;
use build::run_template_build;
use config::read_user_config;

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

    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(|s| s.as_str()) == Some("auth") {
        error!("aws_e2b does not support the e2b auth command");
        std::process::exit(1);
    }

    if args.first().map(|s| s.as_str()) == Some("template")
        && args.get(1).map(|s| s.as_str()) == Some("build")
    {
        let build_args = BuildArgs::parse_from(
            std::iter::once("aws_e2b".to_string()).chain(args.iter().skip(2).cloned()),
        );
        return run_template_build(build_args).await;
    }

    proxy_to_e2b(&args)?;
    Ok(())
}

/// Forward unsupported commands to the e2b command-line interface and inject required environment variables
fn proxy_to_e2b(args: &[String]) -> Result<()> {
    let (domain_opt, token_opt) = resolve_e2b_env_vars();

    let mut command = std::process::Command::new("e2b");
    command.args(args);
    if let Some(domain) = domain_opt {
        command.env("E2B_DOMAIN", domain);
    }
    if let Some(token) = token_opt {
        command.env("E2B_ACCESS_TOKEN", token);
    }

    let status = command.status().context("failed to execute e2b command")?;
    if !status.success() {
        return Err(anyhow!("e2b command failed"));
    }
    Ok(())
}

/// Resolve e2b domain and access token from environment variables or user configuration
fn resolve_e2b_env_vars() -> (Option<String>, Option<String>) {
    let user_cfg = read_user_config().ok().flatten();
    let domain = env::var("E2B_DOMAIN").ok().or_else(|| {
        user_cfg
            .as_ref()
            .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_domain.clone()))
    });
    let token = env::var("E2B_ACCESS_TOKEN").ok().or_else(|| {
        user_cfg
            .as_ref()
            .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_access_token.clone()))
    });
    (domain, token)
}
