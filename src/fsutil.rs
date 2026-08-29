use std::{
    ffi::CString,
    fs,
    io::{Read, Write},
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempPath};

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to open {} for hashing", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

pub fn atomic_replace_if_unchanged(path: &Path, expected: &[u8], replacement: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.is_file(),
        "file changed concurrently; refusing to replace it"
    );
    anyhow::ensure!(
        fs::read(path)? == expected,
        "file changed concurrently; refusing to replace it"
    );

    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(replacement)?;
    temporary
        .as_file()
        .set_permissions(metadata.permissions())?;
    temporary.as_file().sync_all()?;
    let temporary_path = temporary.into_temp_path();
    let replacement_identity = file_identity(&temporary_path)?;
    rename_swap(&temporary_path, path)?;

    if !same_file(path, replacement_identity)? {
        let conflict = keep_temp(temporary_path)?;
        anyhow::bail!(
            "file changed concurrently; refusing to replace it (preserved previous file at {})",
            conflict.display()
        );
    }

    let previous_metadata = fs::symlink_metadata(&temporary_path)?;
    let previous = fs::read(&temporary_path)?;
    if !previous_metadata.is_file() || previous != expected {
        if same_file(path, replacement_identity)? {
            rename_swap(path, &temporary_path)
                .context("failed to roll back concurrent file replacement")?;
            anyhow::bail!("file changed concurrently; refusing to replace it");
        }
        let conflict = keep_temp(temporary_path)?;
        anyhow::bail!(
            "file changed concurrently; refusing to replace it (preserved previous file at {})",
            conflict.display()
        );
    }

    if !same_file(path, replacement_identity)? || fs::read(path)? != replacement {
        let conflict = keep_temp(temporary_path)?;
        anyhow::bail!(
            "file changed concurrently; refusing to replace it (preserved previous file at {})",
            conflict.display()
        );
    }
    fs::remove_file(&temporary_path)?;
    sync_parent(parent);
    Ok(())
}

pub fn atomic_remove_if_unchanged(path: &Path, expected: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.is_file(),
        "file changed concurrently; refusing to remove it"
    );
    anyhow::ensure!(
        fs::read(path)? == expected,
        "file changed concurrently; refusing to remove it"
    );

    let temporary = NamedTempFile::new_in(parent)?.into_temp_path();
    fs::remove_file(&temporary)?;
    fs::rename(path, &temporary)?;
    let moved = fs::read(&temporary)?;
    if moved != expected {
        if path.exists() {
            let conflict = keep_temp(temporary)?;
            anyhow::bail!(
                "file changed concurrently; refusing to remove it (preserved file at {})",
                conflict.display()
            );
        }
        fs::rename(&temporary, path)?;
        anyhow::bail!("file changed concurrently; refusing to remove it");
    }
    if path.exists() {
        fs::remove_file(&temporary)?;
        anyhow::bail!("file changed concurrently; refusing to remove it");
    }
    fs::remove_file(&temporary)?;
    sync_parent(parent);
    Ok(())
}

pub fn atomic_create(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    let temporary_path = temporary.into_temp_path();
    let source = CString::new(temporary_path.as_os_str().as_bytes())?;
    let target = CString::new(path.as_os_str().as_bytes())?;
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("file appeared concurrently");
    }
    sync_parent(parent);
    Ok(())
}

fn rename_swap(first: &Path, second: &Path) -> Result<()> {
    let first = CString::new(first.as_os_str().as_bytes())?;
    let second = CString::new(second.as_os_str().as_bytes())?;
    let result = unsafe { libc::renamex_np(first.as_ptr(), second.as_ptr(), libc::RENAME_SWAP) };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("failed to atomically exchange files");
    }
    Ok(())
}

fn file_identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}

fn same_file(path: &Path, identity: (u64, u64)) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok((metadata.dev(), metadata.ino()) == identity),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn keep_temp(path: TempPath) -> Result<PathBuf> {
    path.keep()
        .map_err(|error| error.error)
        .context("failed to preserve the concurrent file")
}

fn sync_parent(parent: &Path) {
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let previous_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if let Some(permissions) = previous_permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically replace {}", path.display()))?;

    sync_parent(parent);
    Ok(())
}
