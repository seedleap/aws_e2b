use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::env;
use which::which;

mod args;
mod aws_utils;
mod build;
mod config;
mod docker_utils;
mod e2b_api;

use args::{AwsE2bCli, AwsE2bCommand, ListArgs, TemplateCommand};
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

    let cli = AwsE2bCli::parse();

    match cli.command {
        AwsE2bCommand::Template { command } => match command {
            TemplateCommand::Build(build_args) => run_template_build(build_args).await,
            TemplateCommand::List(list_args) => {
                run_template_list(list_args)?;
                Ok(())
            }
        },
        AwsE2bCommand::Sandbox(sandbox_args) => {
            let forward_args = std::iter::once("sandbox".to_string())
                .chain(sandbox_args.args.into_iter())
                .collect::<Vec<String>>();
            proxy_to_e2b(&forward_args)?;
            Ok(())
        }
    }
}

/// Forward a command to the official e2b CLI and inject domain, access token, and API key environment variables
fn proxy_to_e2b(args: &[String]) -> Result<()> {
    // Ensure the official e2b CLI is installed before forwarding the command.
    if which("e2b").is_err() {
        return Err(anyhow!(
            "The e2b CLI was not found. Please install it by following https://e2b.dev/docs/cli"
        ));
    }

    let (domain_opt, token_opt, api_key_opt) = resolve_e2b_env_vars();

    let mut command = std::process::Command::new("e2b");
    command.args(args);
    if let Some(domain) = domain_opt {
        command.env("E2B_DOMAIN", domain);
    }
    if let Some(token) = token_opt {
        command.env("E2B_ACCESS_TOKEN", token);
    }
    if let Some(api_key) = api_key_opt {
        command.env("E2B_API_KEY", api_key);
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

/// Resolve the e2b domain, access token, and API key from environment variables or user configuration
fn resolve_e2b_env_vars() -> (Option<String>, Option<String>, Option<String>) {
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
    let api_key = env::var("E2B_API_KEY").ok().or_else(|| {
        user_cfg
            .as_ref()
            .and_then(|c| c.e2b.as_ref().and_then(|e| e.e2b_api_key.clone()))
    });
    (domain, token, api_key)
}
