//! MCP server CLI entry point.
//!
//! This module provides the CLI parsing and entry point for running the MCP server.

use clap::Parser;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_mcp_server::run_mcp_server;
use std::env;
use std::sync::Arc;

/// Parse MCP CLI arguments and return the configuration.
pub struct McpConfig {
    pub root_dir: String,
    pub config_path: String,
    pub threads: u8,
}

/// Hakana MCP (Model Context Protocol) server for LLM integration
#[derive(Parser)]
#[command(name = "hakana-mcp")]
struct McpArgs {
    /// The root directory that Hakana runs in. Defaults to the current directory
    #[arg(long, value_name = "PATH")]
    root: Option<String>,

    /// Hakana config path — defaults to ./hakana.json
    #[arg(long, value_name = "PATH")]
    config: Option<String>,

    /// How many threads to use
    #[arg(long, value_name = "NUM", default_value_t = 8)]
    threads: u8,
}

/// Parse command line arguments for the MCP server.
pub fn parse_args() -> McpConfig {
    let args = McpArgs::parse();

    let cwd = env::current_dir().unwrap().to_str().unwrap().to_string();

    let root_dir = args.root.unwrap_or(cwd);

    let config_path = args
        .config
        .unwrap_or_else(|| format!("{}/hakana.json", root_dir));

    McpConfig {
        root_dir,
        config_path,
        threads: args.threads,
    }
}

/// Run the MCP server with the given plugins and header.
pub async fn run(plugins: Vec<Box<dyn CustomHook>>, header: String) {
    hakana_logger::init_stderr_logger(log::LevelFilter::Info);

    let config = parse_args();

    // Convert hooks to Arc for reuse
    let plugins: Vec<Arc<dyn CustomHook>> = plugins.into_iter().map(Arc::from).collect();

    if let Err(e) = run_mcp_server(
        config.root_dir,
        config.threads,
        Some(config.config_path),
        plugins,
        header,
    )
    .await
    {
        eprintln!("MCP server error: {}", e);
        std::process::exit(1);
    }
}
