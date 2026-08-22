use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use info_provider::InfoProvider;
use parser::Db;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{InitializeParams, InitializeResult};
use tower_lsp::{Client, LanguageServer, LspService};
use url::Url;

struct TestContext;

#[tower_lsp::async_trait]
impl LanguageServer for TestContext {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

fn make_client() -> Client {
    let holder = Arc::new(Mutex::new(None));
    let holder2 = holder.clone();
    let (_service, _socket) = LspService::new(move |client| {
        *holder2.lock().unwrap() = Some(client);
        TestContext
    });
    let client = holder.lock().unwrap().take().unwrap();
    client
}

// https://github.com/frederik-uni/zed-cargotom/issues/25
// A workspace listing "." as a member resolves back to its own manifest
#[tokio::test]
async fn workspace_member_dot_does_not_recurse_forever() {
    let dir = std::env::temp_dir().join(format!("cargotom-self-member-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\".\"]\n\n[package]\nname = \"foo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let uri = Url::from_file_path(dir.join("Cargo.toml")).unwrap();
    // offline=true would make it slow, not need here
    let info = Arc::new(InfoProvider::new(50, false, dir.clone()).await);
    let db = Db::new(make_client(), info);

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        db.write("test").await.try_init(&uri).await;
    })
    .await;

    fs::remove_dir_all(&dir).ok();

    assert!(
        result.is_ok(),
        "try_init did not return within 10s, likely stuck in infinite recursion"
    );
}

// A member's manifest re-declares a workspace whose own member list resolves
// back to the root via a different path string (here "..")
#[tokio::test]
async fn workspace_member_aliasing_root_via_different_path_does_not_recurse_forever() {
    let dir = std::env::temp_dir().join(format!("cargotom-alias-member-{}", std::process::id()));
    let sub = dir.join("a");
    fs::create_dir_all(&sub).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\"]\n\n[package]\nname = \"root\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(sub.join("Cargo.toml"), "[workspace]\nmembers = [\"..\"]\n").unwrap();

    let uri = Url::from_file_path(dir.join("Cargo.toml")).unwrap();
    // offline=true would make it slow, not need here
    let info = Arc::new(InfoProvider::new(50, false, dir.clone()).await);
    let db = Db::new(make_client(), info);

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        db.write("test").await.try_init(&uri).await;
    })
    .await;

    fs::remove_dir_all(&dir).ok();

    assert!(
        result.is_ok(),
        "try_init did not return within 10s, likely stuck in infinite recursion"
    );
}

// A member directory is actually a symlink back to the workspace root, so
// resolving it yields a URL that is textually different from the root's but
// points at the same file on disk
#[cfg(unix)]
#[tokio::test]
async fn workspace_member_symlink_loop_does_not_recurse_forever() {
    let dir = std::env::temp_dir().join(format!("cargotom-symlink-member-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\"]\n\n[package]\nname = \"root\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(&dir, dir.join("a")).unwrap();

    let uri = Url::from_file_path(dir.join("Cargo.toml")).unwrap();
    // offline=true would make it slow, not need here
    let info = Arc::new(InfoProvider::new(50, false, dir.clone()).await);
    let db = Db::new(make_client(), info);

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        db.write("test").await.try_init(&uri).await;
    })
    .await;

    fs::remove_dir_all(&dir).ok();

    assert!(
        result.is_ok(),
        "try_init did not return within 10s, likely stuck in infinite recursion"
    );
}
