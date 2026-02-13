use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "whspr-rs", version, about = "Speech-to-text dictation tool for Wayland")]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Increase log verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Interactive first-time setup wizard
    Setup,

    /// Manage whisper models
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModelAction {
    /// List available models and their status
    List,

    /// Download a model
    Download {
        /// Model name (e.g. large-v3-turbo, tiny, base)
        name: String,
    },

    /// Select a downloaded model as active
    Select {
        /// Model name to use
        name: String,
    },
}
