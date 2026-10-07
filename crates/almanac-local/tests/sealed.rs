//! What is on disk: sealed files and encrypted databases hold no plaintext, and the typed errors
//! a wrong key gives at each seam.

mod common;

use almanac_local::LocalVault;
use almanac_seal::KeyStore;
use almanac_service::{Backend, BackendError};
use common::*;
use eventlog::LogError;
use memfiles::{Vault, VaultError, VaultPath};
use std::path::Path;

const NEEDLE: &str = "zebracorp";

fn files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|e| match e.path() {
            p if p.is_dir() => files_under(&p),
            p => vec![p],
        })
        .collect()
}

async fn written(root: &Path) -> std::sync::Arc<Service> {
    let service = fresh(root, 7, almanac_fake::ScriptedConsolidator::default());
    shell(&service)
        .propose(work(), draft("orgs/zebra", &format!("{NEEDLE} pays late.")))
        .await
        .expect("propose");
    service
}

#[tokio::test]
async fn no_file_under_the_root_holds_the_plaintext() {
    let dir = tempfile::tempdir().expect("dir");
    let service = written(dir.path()).await;
    drop(service);
    let files = files_under(dir.path());
    assert!(files.iter().any(|f| f.ends_with("events.db")));
    assert!(files.iter().any(|f| f.ends_with("index.db")));
    for file in files.iter().filter(|f| !f.ends_with("spaces.toml")) {
        let bytes = std::fs::read(file).expect("read");
        assert!(
            !bytes.windows(NEEDLE.len()).any(|w| w == NEEDLE.as_bytes()),
            "{} holds the plaintext",
            file.display()
        );
    }
}

#[tokio::test]
async fn each_seam_gives_a_typed_error_for_the_wrong_key() {
    let dir = tempfile::tempdir().expect("dir");
    let service = written(dir.path()).await;
    let meta = service.metas().remove(0);
    drop(service);

    let wrong = backend(dir.path(), 8, almanac_fake::ScriptedConsolidator::default());
    let key = wrong.keys().get(&work()).await.expect("derived key");
    assert!(matches!(
        wrong.open_log(&work(), meta.replica, &key),
        Err(BackendError::Log(LogError::Locked))
    ));
    assert!(wrong.open_index(&work(), &key).is_err());
    let LocalVault::Sealed(_) = wrong.open_files(&meta, &key).expect("files") else {
        panic!("a sealed Space opens a sealed vault");
    };
    let files = wrong.open_files(&meta, &key).expect("files");
    let path = VaultPath::topic(&topic("orgs/zebra"));
    assert!(matches!(files.read(&path), Err(VaultError::Sealed(_))));

    let right = backend(dir.path(), 7, almanac_fake::ScriptedConsolidator::default());
    let key = right.keys().get(&work()).await.expect("key");
    let text = right.open_files(&meta, &key).expect("files").read(&path);
    assert!(
        String::from_utf8(text.expect("read"))
            .expect("utf8")
            .contains(NEEDLE)
    );
}
