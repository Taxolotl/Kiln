use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "kiln")]
#[command(about = "A CLI tool", version = "1.0")]
pub struct KilnArgs {
    #[command(subcommand)]
    pub command: KilnCommand,
}

#[derive(Subcommand, Debug)]
pub enum KilnCommand {
    /// Initialize a project in the current directory
    Setup,

    /// Add a new project by name
    New {
        /// Name of the new project
        name: String,
    },

    /// Operate on an existing project
    /// Operate on an existing project
    #[command(flatten)]
    Project(ProjectCommand),
}

#[derive(Subcommand, Debug)]
pub enum ProjectCommand {
    /// Add a mod to a project
    Add {
        name: String,
        id: String,
    },

    /// Remove a mod from a project
    Remove {
        name: String,
        id: String,
    },

    /// Launch a project
    Launch {
        name: String,
    },

    /// Export a project
    Export {
        name: String,
    },
}
