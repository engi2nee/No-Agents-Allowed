use crate::config::find_root;
use crate::engine::{self, RunOpts};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "noagents",
    version,
    about = "Keep AI coding agents out of your secrets: one config, every agent's ignore file."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args)]
pub struct CommonOpts {
    /// Project root (default: nearest dir with .noagents, then .git, then cwd)
    #[arg(long)]
    pub root: Option<PathBuf>,
    /// Only these target ids, comma-separated (see `noagents list`)
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,
    /// Exclude these target ids, comma-separated
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,
    /// Show what would change without writing anything
    #[arg(long)]
    pub dry_run: bool,
    /// Suppress unchanged/informational output
    #[arg(long, short)]
    pub quiet: bool,
}

impl CommonOpts {
    fn resolve(&self) -> Result<RunOpts> {
        Ok(RunOpts {
            root: find_root(self.root.as_deref())?,
            only: self.only.clone(),
            exclude: self.exclude.clone(),
            dry_run: self.dry_run,
            quiet: self.quiet,
        })
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Create .noagents with sensible default secret patterns
    Init {
        /// Overwrite an existing .noagents
        #[arg(long)]
        force: bool,
        /// Project root (default: nearest .noagents/.git dir, else cwd)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Generate or update every agent's ignore file from .noagents
    #[command(alias = "sync")]
    Generate {
        #[command(flatten)]
        opts: CommonOpts,
    },
    /// Add pattern(s) to .noagents and regenerate
    Add {
        /// gitignore-style pattern(s), e.g. ".env" "secrets/"
        #[arg(required = true)]
        patterns: Vec<String>,
        #[command(flatten)]
        opts: CommonOpts,
    },
    /// Strip everything noagents manages from generated files
    #[command(alias = "clean")]
    Remove {
        #[command(flatten)]
        opts: CommonOpts,
    },
    /// List all known agent targets
    List {
        /// Project root (default: nearest .noagents/.git dir, else cwd)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Show per-target sync status
    Status {
        #[command(flatten)]
        opts: CommonOpts,
    },
    /// CI gate: exit 1 if any target is out of sync
    Check {
        #[command(flatten)]
        opts: CommonOpts,
    },
}

pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { force, root } => {
            engine::init(&find_root(root.as_deref())?, force)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Generate { opts } => {
            engine::generate(&opts.resolve()?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Add { patterns, opts } => {
            engine::add(&patterns, &opts.resolve()?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Remove { opts } => {
            engine::remove(&opts.resolve()?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::List { root } => {
            engine::list(&find_root(root.as_deref())?)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Status { opts } => {
            engine::status(&opts.resolve()?, false)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Check { opts } => {
            let drift = engine::status(&opts.resolve()?, true)?;
            Ok(if drift {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
    }
}
