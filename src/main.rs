pub mod api;
mod crate_lookup;
mod generate_tree;
mod git;
mod helper;
mod lock;
pub mod lsp;
mod rust_version;

use std::path::PathBuf;

use clap::Parser;
use crate_lookup::CratesIoStorage;
use helper::crate_version;
use proctitle::set_title;
use tcp_struct::Starter as _;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    daemon: Option<u16>,
    #[arg(long)]
    storage: PathBuf,
    #[arg(long)]
    stable: bool,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    per_page_web: Option<u32>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let path = args.storage.join("crates.io-index-minfied");
    if let Some(port) = args.daemon {
        set_title("cargotom-daemon");
        CratesIoStorage::start_gen(port, crate_version(), || {
            CratesIoStorage::new(
                &path,
                args.stable,
                args.offline,
                args.per_page_web.unwrap_or(25),
            )
        })
        .await
        .unwrap();
    } else {
        lsp::main(path).await
    }
}
