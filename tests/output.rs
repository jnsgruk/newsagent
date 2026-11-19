use newsagent::output::{ensure_absent, write_new};
use std::fs;
use std::io;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use tempfile::TempDir;

fn assert_refused(path: &Path) {
    let err = ensure_absent(path).unwrap_err();
    assert!(
        err.to_string().contains(&path.display().to_string()),
        "unexpected error: {err}"
    );

    let err = write_new(path, "new").unwrap_err();
    assert_eq!(
        err.downcast_ref::<io::Error>().map(io::Error::kind),
        Some(io::ErrorKind::AlreadyExists),
        "unexpected error: {err:#}"
    );
}

#[test]
fn writes_a_new_private_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("outline.md");

    ensure_absent(&path).expect("Path should be absent");
    write_new(&path, "### Heading").expect("Failed to write output");

    assert_eq!(fs::read_to_string(&path).unwrap(), "### Heading\n");
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn refuses_to_overwrite_an_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("outline.md");
    fs::write(&path, "draft").unwrap();

    assert_refused(&path);

    assert_eq!(fs::read_to_string(&path).unwrap(), "draft");
}

#[test]
fn refuses_to_follow_a_symlink() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target.md");
    let link = dir.path().join("outline.md");
    symlink(&target, &link).unwrap();

    assert_refused(&link);

    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(!target.exists());
}
