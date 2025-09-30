use hakana_language_server::daemon_backend::DaemonBackend;
use mimalloc::MiMalloc;
use tower_lsp::{LspService, Server};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();


    let (service, socket) = LspService::new(|client| DaemonBackend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}