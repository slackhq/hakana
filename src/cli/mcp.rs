//! MCP server CLI entry point.
//!
//! This module provides the CLI parsing and entry point for running the MCP server.

use clap::{arg, Command};
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

/// Parse command line arguments for the MCP server.
pub fn parse_args() -> McpConfig {
    let matches = Command::new("hakana-mcp")
        .about("Hakana MCP (Model Context Protocol) server for LLM integration")
        .arg(
            arg!(--"root" <PATH>)
                .required(false)
                .help("The root directory that Hakana runs in. Defaults to the current directory"),
        )
        .arg(
            arg!(--"config" <PATH>)
                .required(false)
                .help("Hakana config path — defaults to ./hakana.json"),
        )
        .arg(
            arg!(--"threads" <NUM>)
                .required(false)
                .help("How many threads to use (default: 8)"),
        )
        .get_matches();

    let cwd = env::current_dir()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let root_dir = matches
        .get_one::<String>("root")
        .map(|s| s.to_string())
        .unwrap_or_else(|| cwd.clone());

    let config_path = matches
        .get_one::<String>("config")
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/hakana.json", root_dir));

    let threads: u8 = matches
        .get_one::<String>("threads")
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);

    McpConfig {
        root_dir,
        config_path,
        threads,
    }
}

/// Run the MCP server with the given plugins and header.
pub fn run(
    plugins: Vec<Box<dyn CustomHook>>,
    header: String,
) {
    let config = parse_args();

    // Convert hooks to Arc for reuse
    let plugins: Vec<Arc<dyn CustomHook>> = plugins
        .into_iter()
        .map(Arc::from)
        .collect();

    if let Err(e) = run_mcp_server(
        config.root_dir,
        config.threads,
        Some(config.config_path),
        plugins,
        header,
    ) {
        eprintln!("MCP server error: {}", e);
        std::process::exit(1);
    }
}
