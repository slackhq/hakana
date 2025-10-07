use hakana_language_server::serve;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let current_dir = std::env::current_dir();

    serve(stdin, stdout, current_dir).await;
}
