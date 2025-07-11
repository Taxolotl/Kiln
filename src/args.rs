use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "kiln")]
#[command(
    about = "A CLI tool for managing Vintage Story modpacks",
    version,
    about
)]
pub struct KilnArgs {
    #[command(subcommand)]
    pub command: KilnCommand,
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum KilnCommand {
    /// Set up the kiln configs
    Setup,

    /// Add a new project by name
    New {
        /// Name of the new project
        name: String,
    },

    /// Import a project from a .kiln file
    Import { filename: String },

    /// Clone the current Vintagestory data dir as a modpack
    Clone,

    /// List all modpacks in the current system
    List,

    /// Operate on an existing project
    #[command(flatten)]
    Project(ProjectCommand),
}

#[derive(Subcommand, Debug, PartialEq)]
pub enum ProjectCommand {
    /// Add a mod to a project
    Add { name: String, id: String },

    /// Remove a mod from a project
    Remove { name: String, id: String },

    /// Launch a project
    #[clap(aliases = &["launch", "start"])]
    Run { name: String },

    /// Export a project
    Export { name: String },

    /// List currently installed mods in a project
    Mods { name: String },
}
