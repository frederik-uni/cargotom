use std::{env, path::PathBuf, process::exit};

#[tokio::main]
async fn main() {
    let mut args = env::args();
    args.next();
    let args: Vec<String> = args.collect();
    let path = if let Some(index) = args.iter().position(|arg| arg == "--storage") {
        if let Some(value) = args.get(index + 1) {
            PathBuf::from(value)
        } else {
            eprintln!("Error: --storage provided but no value was given.");
            exit(1)
        }
    } else {
        eprintln!("--storage flag not found.");
        exit(1)
    };
    lsp::main(path).await;
}
