#[allow(dead_code)]
#[path = "../src/images.rs"]
mod images;

use images::{ImageStore, PendingFiles};
use std::fs;
use tempfile::TempDir;

#[test]
fn paths_use_task_and_image_ids_with_media_extension() {
    let dir = TempDir::new().unwrap();
    let store = ImageStore::new(dir.path().join("images"));
    for (media_type, ext) in [
        ("image/png", "png"),
        ("image/jpeg", "jpg"),
        ("image/gif", "gif"),
        ("image/webp", "webp"),
    ] {
        assert_eq!(
            store.path(7, 12, media_type).unwrap(),
            dir.path().join(format!("images/7/12.{ext}"))
        );
    }
    assert!(store.path(7, 12, "image/svg+xml").is_err());
}

#[test]
fn committed_files_survive_and_existing_bytes_never_get_replaced() {
    let dir = TempDir::new().unwrap();
    let store = ImageStore::new(dir.path().join("images"));
    let path = store.path(7, 12, "image/png").unwrap();
    let mut pending = PendingFiles::new();
    store
        .write(&mut pending, 7, 12, "image/png", b"original")
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"original");
    pending.keep();
    drop(pending);

    let mut retry = PendingFiles::new();
    store
        .write(&mut retry, 7, 12, "image/png", b"original")
        .unwrap();
    assert!(
        store
            .write(&mut retry, 7, 12, "image/png", b"different")
            .is_err()
    );
    drop(retry);
    assert_eq!(fs::read(path).unwrap(), b"original");
}

#[test]
fn uncommitted_files_get_removed_and_reads_validate_size_and_type() {
    let dir = TempDir::new().unwrap();
    let store = ImageStore::new(dir.path().join("images"));
    let path = store.path(4, 9, "image/jpeg").unwrap();
    assert!(store.read(4, 9, "image/jpeg", 3).is_err());
    let mut pending = PendingFiles::new();
    store
        .write(&mut pending, 4, 9, "image/jpeg", b"abc")
        .unwrap();
    assert_eq!(store.read(4, 9, "image/jpeg", 3).unwrap(), b"abc");
    assert!(store.read(4, 9, "image/jpeg", 2).is_err());
    drop(pending);
    assert!(!path.exists());

    fs::create_dir_all(&path).unwrap();
    assert!(store.read(4, 9, "image/jpeg", 3).is_err());
}

#[cfg(unix)]
#[test]
fn read_rejects_symlinked_storage_file() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let store = ImageStore::new(dir.path().join("images"));
    let path = store.path(2, 3, "image/png").unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let target = dir.path().join("elsewhere");
    fs::write(&target, b"abc").unwrap();
    symlink(&target, path).unwrap();
    assert!(store.read(2, 3, "image/png", 3).is_err());
}
