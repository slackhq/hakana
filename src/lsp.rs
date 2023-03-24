use mimalloc::MiMalloc;

use std::env;

use hakana_language_server::{get_config, Backend};
use tower_lsp::{LspService, Server};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let cwd = (env::current_dir()).unwrap().to_str().unwrap().to_string();
    let config = get_config(vec![], &cwd);

    let (service, socket) = LspService::new(|client| Backend {
        client,
        analysis_config: config,
    });
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
