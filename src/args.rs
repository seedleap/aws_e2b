use clap::Parser;
use std::path::PathBuf;

/// All arguments for the `template build` subcommand
#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Optional path to the configuration file, defaulting to `aws_e2b.toml` in the current directory
    #[arg(long = "config")]
    pub config_path: Option<PathBuf>,

    #[command(flatten)]
    pub e2b: E2bArgs,

    #[command(flatten)]
    pub docker: DockerArgs,
}

/// Parameters related to the e2b template
#[derive(Parser, Debug, Clone)]
pub struct E2bArgs {
    /// Optional override for the default memory size in megabytes
    #[arg(long = "memory-mb", help_heading = "E2B")]
    pub memory_mb: Option<u32>,

    /// Optional override for the number of CPU cores
    #[arg(long = "cpu-count", help_heading = "E2B")]
    pub cpu_count: Option<u32>,

    /// Optional command to execute after the template starts
    #[arg(long = "start-cmd", help_heading = "E2B")]
    pub start_cmd: Option<String>,

    /// Optional command to check whether the template is ready
    #[arg(long = "ready-cmd", help_heading = "E2B")]
    pub ready_cmd: Option<String>,

    /// Optional alias for the template
    #[arg(long = "alias", help_heading = "E2B")]
    pub alias: Option<String>,

    /// Optional existing template identifier to build from
    #[arg(long = "template-id", help_heading = "E2B")]
    pub template_id: Option<String>,
}

/// Parameters related to Docker
#[derive(Parser, Debug, Clone)]
pub struct DockerArgs {
    /// Path to a Dockerfile whose contents will be used for the build
    #[arg(long = "docker-file", help_heading = "DOCKER")]
    pub docker_file: Option<PathBuf>,

    /// Existing Amazon ECR image to use as the base image
    #[arg(long = "ecr-image", help_heading = "DOCKER")]
    pub ecr_image: Option<String>,

    /// Base image to use when neither Dockerfile nor ECR image is provided
    #[arg(long = "base-image", help_heading = "DOCKER")]
    pub base_image: Option<String>,
}
