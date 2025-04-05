use std::path::PathBuf;

#[tokio::main]
async fn main() {
    lsp::main(PathBuf::new()).await;
}
