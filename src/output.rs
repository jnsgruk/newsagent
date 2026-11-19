use anyhow::{bail, Context};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub fn ensure_absent(path: &Path) -> anyhow::Result<()> {
    if path.symlink_metadata().is_ok() {
        bail!("Output {} already exists", path.display());
    }
    Ok(())
}

pub fn write_new(path: &Path, contents: &str) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("Creating output {}", path.display()))?;
    writeln!(file, "{}", contents)?;
    file.sync_all()?;
    Ok(())
}
