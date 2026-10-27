pub mod add_fixmes;
pub mod analyze;
pub mod codegen;
pub mod cyclomatic_complexity;
pub mod find_executable;
pub mod find_paths;
pub mod fix;
pub mod lint;
pub mod migrate;
pub mod migration_candidates;
pub mod remove_unused_fixmes;
pub mod security_check;
pub mod server;
pub mod test;

use clap::Args;

/// Arguments shared by most analysis commands.
#[derive(Args)]
pub struct CommonArgs {
    /// The root directory that Hakana runs in. Defaults to the current directory
    #[arg(long, value_name = "PATH")]
    pub root: Option<String>,

    /// Hakana config path — defaults to ./hakana.json
    #[arg(long, value_name = "PATH")]
    pub config: Option<String>,

    /// How many threads to use (default: 8)
    #[arg(long, value_name = "COUNT")]
    pub threads: Option<u8>,

    /// Add output for debugging
    #[arg(long)]
    pub debug: bool,
}
