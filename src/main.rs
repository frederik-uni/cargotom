pub mod api;
mod crate_lookup;
mod generate_tree;
mod git;
pub mod lsp;

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    storage: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let path = args.storage.join("crates.io-index-minfied");
    lsp::main(path).await
}
