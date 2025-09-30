use hakana_daemon_server::run_daemon_cli;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run daemon CLI with no plugins (generic version)
    run_daemon_cli(vec![]).await
}