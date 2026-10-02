#[allow(dead_code)]
#[path = "../src/snapshot/format.rs"]
mod format;

use format::{FileMeta, ImageMeta, Manifest, validate_image_path};

fn digest() -> String {
    "a".repeat(64)
}

fn manifest(paths: &[&str]) -> Manifest {
    Manifest {
        version: 1,
        database: FileMeta {
            bytes: 4096,
            sha256: digest(),
        },
        images: paths
            .iter()
            .map(|path| ImageMeta {
                path: (*path).to_owned(),
                bytes: 8,
                sha256: digest(),
            })
            .collect(),
    }
}

#[test]
fn exact_image_path_grammar() {
    assert_eq!(
        validate_image_path("images/7/3.png").unwrap(),
        (7, 3, "png")
    );
    for path in [
        "/images/7/3.png",
        "../images/7/3.png",
        "images/7/../3.png",
        "images/07/3.png",
        "images/7/03.png",
        "images/0/3.png",
        "images/7/3.svg",
        "images/7/3.png/extra",
        "images\\7\\3.png",
    ] {
        assert!(validate_image_path(path).is_err(), "{path}");
    }
}

#[test]
fn manifest_rejects_duplicate_paths_and_unknown_version() {
    manifest(&["images/7/3.png"]).validate().unwrap();
    assert!(
        manifest(&["images/7/3.png", "images/7/3.png"])
            .validate()
            .is_err()
    );
    let mut unsupported = manifest(&[]);
    unsupported.version = 2;
    assert!(unsupported.validate().is_err());
}

#[test]
fn hash_reader_reports_size_and_sha256() {
    let meta = format::hash_reader("abc".as_bytes()).unwrap();
    assert_eq!(meta.bytes, 3);
    assert_eq!(
        meta.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
