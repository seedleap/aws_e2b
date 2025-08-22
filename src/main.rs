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

use args::{BuildArgs, ListArgs};
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

    match args.first().map(|s| s.as_str()) {
        Some("template") => match args.get(1).map(|s| s.as_str()) {
            Some("build") => {
                let build_args = BuildArgs::parse_from(
                    std::iter::once("aws_e2b".to_string()).chain(args.iter().skip(2).cloned()),
                );
                return run_template_build(build_args).await;
            }
            Some("list") => {
                let list_args = ListArgs::parse_from(
                    std::iter::once("aws_e2b".to_string()).chain(args.iter().skip(2).cloned()),
                );
                run_template_list(list_args)?;
                return Ok(());
            }
            _ => {
                error!("aws_e2b does not support this template subcommand");
                std::process::exit(1);
            }
        },
        Some("sandbox") => {
            proxy_to_e2b(&args)?;
            Ok(())
        }
        _ => {
            error!("aws_e2b does not support this command");
            std::process::exit(1);
        }
    }
}

/// Forward unsupported commands to the official e2b CLI and inject required environment variables
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

/// Handle the `template list` subcommand
fn run_template_list(args: ListArgs) -> Result<()> {
    let team_id = if let Some(tid) = args.team {
        tid
    } else {
        read_user_config()
            .ok()
            .flatten()
            .and_then(|c| c.e2b.and_then(|e| e.e2b_team_id))
            .ok_or_else(|| {
                anyhow!("Missing team identifier, please use --team or set [e2b].e2b_team_id in the configuration")
            })?
    };
    let cmd_args = vec![
        "template".to_string(),
        "list".to_string(),
        "--team".to_string(),
        team_id,
    ];
    proxy_to_e2b(&cmd_args)
}

/// Resolve the e2b domain and access token from environment variables or user configuration
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
