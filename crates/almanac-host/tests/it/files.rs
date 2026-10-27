//! Config files are written whole and private, and a damaged file is never taken for an absent one.

use almanac_host::{read_config, write_private_atomic};

#[test]
fn config_files_are_written_whole_private_and_read_without_mistaking_damage_for_absence() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("nested").join("memory.toml");
    assert_eq!(read_config(&path).expect("absent is fine"), None);

    write_private_atomic(&path, "first = 1\n").expect("write");
    write_private_atomic(&path, "second = 2\n").expect("rewrite");
    assert_eq!(
        read_config(&path).expect("read").as_deref(),
        Some("second = 2\n")
    );
    let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    let left: Vec<_> = std::fs::read_dir(path.parent().expect("parent"))
        .expect("list")
        .collect();
    assert_eq!(left.len(), 1, "no temp file is left behind");

    // A directory where the file should be is a read error, not an empty file.
    let odd = dir.path().join("odd");
    std::fs::create_dir(&odd).expect("dir");
    assert!(read_config(&odd).is_err());
}
