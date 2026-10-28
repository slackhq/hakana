use super::CommonArgs;
use clap::Args;
use hakana_analyzer::custom_hook::CustomHook;
use hakana_protocol::ClientSocket;
use std::path::Path;
use std::process::exit;
use std::sync::Arc;

#[derive(Args)]
pub struct ServerArgs {
    #[command(flatten)]
    pub common: CommonArgs,

    /// Stop a running server
    #[arg(long)]
    pub stop: bool,

    /// Show server status
    #[arg(long)]
    pub status: bool,
}

pub async fn handle(
    args: &ServerArgs,
    root_dir: &str,
    threads: u8,
    header: &str,
    analysis_hooks: Vec<Box<dyn CustomHook>>,
    migration_hooks: Vec<Box<dyn CustomHook>>,
) {
    use hakana_protocol::{Message, ShutdownRequest, SocketPath, StatusRequest};
    use hakana_server::{Server, ServerConfig};

    let socket_path = SocketPath::for_project(Path::new(root_dir));

    if args.status {
        if !socket_path.server_exists() {
            println!("Server not running");
            return;
        }

        match ClientSocket::connect(&socket_path).await {
            Ok(mut client) => {
                let response = client.request(&Message::Status(StatusRequest)).await;
                match response {
                    Ok(Message::StatusResult(status)) => {
                        println!("Server Status:");
                        println!("  Ready: {}", status.ready);
                        println!("  Files: {}", status.files_count);
                        println!("  Symbols: {}", status.symbols_count);
                        println!("  Uptime: {}s", status.uptime_secs);
                        println!("  Analysis in progress: {}", status.analysis_in_progress);
                        println!("  Project root: {}", status.project_root);
                    }
                    Ok(_) => println!("Unexpected response from server"),
                    Err(e) => println!("Error communicating with server: {}", e),
                }
            }
            Err(e) => {
                println!("Cannot connect to server: {}", e);
            }
        }
        return;
    }

    if args.stop {
        if !socket_path.server_exists() {
            println!("Server not running");
            return;
        }

        match ClientSocket::connect(&socket_path).await {
            Ok(mut client) => match client.send(&Message::Shutdown(ShutdownRequest)).await {
                Ok(_) => println!("Shutdown signal sent"),
                Err(e) => println!("Error sending shutdown: {}", e),
            },
            Err(e) => {
                println!("Cannot connect to server: {}", e);
            }
        }
        return;
    }

    let config_path = args
        .common
        .config
        .clone()
        .unwrap_or_else(|| format!("{}/hakana.json", root_dir));

    let plugins: Vec<Arc<dyn CustomHook>> = analysis_hooks.into_iter().map(Arc::from).collect();
    let migration_hooks: Vec<Arc<dyn CustomHook>> =
        migration_hooks.into_iter().map(Arc::from).collect();

    let server_config = ServerConfig {
        root_dir: root_dir.to_string(),
        threads,
        config_path: Some(config_path),
        plugins,
        migration_hooks,
        header: header.to_string(),
        chaos_monkey: None,
    };

    match Server::new(server_config) {
        Ok(mut server) => {
            tty_println!("Starting hakana server...");
            tty_println!("Socket: {}", server.socket_path().path().display());
            if let Err(e) = server.run().await {
                println!("Server error: {}", e);
                exit(1);
            }
        }
        Err(e) => {
            println!("Failed to start server: {}", e);
            exit(1);
        }
    }
}
