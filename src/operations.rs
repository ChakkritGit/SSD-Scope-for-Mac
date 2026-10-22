use crate::{
    command::{self, Control},
    disks::unix_now,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use walkdir::WalkDir;

pub fn validate_backup(source: &Path, parent: &Path) -> Result<(PathBuf, PathBuf)> {
    let source = source
        .canonicalize()
        .context("Source folder is unavailable")?;
    let parent = parent
        .canonicalize()
        .context("Destination folder is unavailable")?;
    ensure!(
        source.is_dir() && parent.is_dir(),
        "Select source and destination folders"
    );
    ensure!(
        source != Path::new("/") && !source.starts_with("/System") && !source.starts_with("/dev"),
        "Choose a data folder or an external volume. This is a file backup, not a macOS system clone."
    );
    ensure!(
        !parent.starts_with(&source),
        "Destination is inside the source. Choose a destination outside the folder/volume being backed up."
    );
    Ok((source, parent))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub source: PathBuf,
    pub completed_unix: u64,
    pub regular_files_verified: Option<u64>,
    pub kind: String,
}

/// Always creates a new directory; never merges into or erases an existing destination.
pub fn backup(source: &Path, parent: &Path, verify: bool, control: &Control) -> Result<PathBuf> {
    let (source, parent) = validate_backup(source, parent)?;
    control.check()?;
    let directory = tempfile::Builder::new()
        .prefix(&format!("Scope Backup {} ", unix_now()))
        .suffix(".partial")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&parent)?;
    // Retain partial output even on error or cancellation, for explicit user recovery.
    let partial = directory.keep();
    let payload = partial.join("Files");
    let work = || -> Result<PathBuf> {
        command::run(
            "/usr/bin/ditto",
            &[
                "--rsrc",
                "--extattr",
                "--acl",
                source.to_str().context("Source path is not valid UTF-8")?,
                payload
                    .to_str()
                    .context("Destination path is not valid UTF-8")?,
            ],
            Duration::from_secs(86_400),
            control,
        )?
        .success()?;
        control.check()?;
        let count = if verify {
            Some(verify_copy(&source, &payload, control)?)
        } else {
            None
        };
        let manifest = Manifest {
            format_version: 1,
            source: source.clone(),
            completed_unix: unix_now(),
            regular_files_verified: count,
            kind: "File data backup; not a bootable system clone; no live snapshot".into(),
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(partial.join("scope-backup.json"))?;
        file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        file.sync_all()?;
        control.check()?;
        let name = partial
            .file_name()
            .context("Missing backup name")?
            .to_string_lossy();
        let completed = partial.with_file_name(name.trim_end_matches(".partial"));
        // macOS RENAME_EXCL prevents replacing a concurrently created destination.
        rename_exclusive(&partial, &completed)?;
        Ok(completed)
    };
    work().with_context(|| {
        format!(
            "Partial files retained at {}. A .partial folder is NOT a completed backup.",
            partial.display()
        )
    })
}

#[cfg(target_os = "macos")]
fn rename_exclusive(source: &Path, target: &Path) -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes())?;
    let target = CString::new(target.as_os_str().as_bytes())?;
    let result = unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn sha256(path: &Path, control: &Control) -> Result<[u8; 32]> {
    // A symlink replacing a file during verification must not escape the selected tree.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Expected a regular file: {}",
        path.display()
    );
    let mut buffer = vec![0; 1024 * 1024];
    let mut hash = Sha256::new();
    loop {
        control.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().into())
}

/// Checks regular-file SHA-256 and symlink targets. Metadata is preserved by ditto, not certified here.
pub fn verify_copy(source: &Path, copy: &Path, control: &Control) -> Result<u64> {
    let mut count = 0;
    for entry in WalkDir::new(source).follow_links(false).min_depth(1) {
        control.check()?;
        let entry = entry?; // Never silently skip inaccessible files.
        let relative = entry.path().strip_prefix(source)?;
        let target = copy.join(relative);
        let kind = entry.file_type();
        let target_kind = fs::symlink_metadata(&target)
            .with_context(|| format!("Missing copy: {}", relative.display()))?
            .file_type();
        if kind.is_symlink() {
            ensure!(
                target_kind.is_symlink() && fs::read_link(entry.path())? == fs::read_link(&target)?,
                "Symlink mismatch: {}",
                relative.display()
            );
        } else if kind.is_file() {
            ensure!(
                target_kind.is_file()
                    && sha256(entry.path(), control)? == sha256(&target, control)?,
                "SHA-256 mismatch or live source changed: {}",
                relative.display()
            );
            count += 1;
        } else if kind.is_dir() {
            ensure!(
                target_kind.is_dir(),
                "Directory mismatch: {}",
                relative.display()
            );
        } else {
            bail!("Cannot verify special file: {}", relative.display())
        }
    }
    Ok(count)
}

pub fn restore_source(backup: &Path) -> Result<PathBuf> {
    let manifest_path = backup.join("scope-backup.json");
    ensure!(
        !backup
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".partial")),
        "Incomplete backup"
    );
    let manifest: Manifest = serde_json::from_reader(File::open(manifest_path)?)?;
    ensure!(manifest.format_version == 1, "Unsupported backup version");
    let files = backup.join("Files");
    ensure!(
        fs::symlink_metadata(&files)?.is_dir(),
        "Backup Files folder is missing or is a symlink"
    );
    Ok(files)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Benchmark {
    pub directory: PathBuf,
    pub bytes: u64,
    pub write_mbps: f64,
    pub read_mbps: f64,
    pub captured_unix: u64,
}

pub fn benchmark(directory: &Path, size: u64, control: &Control) -> Result<Benchmark> {
    ensure!(
        size > 0 && size <= 1024 * 1024 * 1024 && size.is_multiple_of(1024 * 1024),
        "Invalid benchmark size"
    );
    let directory = directory.canonicalize()?;
    ensure!(directory.is_dir(), "Select a writable test folder");
    ensure!(
        free_space(&directory)? > size + 1024 * 1024 * 1024,
        "At least 1 GB free space beyond the test size is required"
    );
    let mut file = tempfile::Builder::new()
        .prefix(".scope-benchmark-")
        .suffix(".tmp")
        .tempfile_in(&directory)?;
    let fd = file.as_raw_fd();
    ensure!(
        unsafe { libc::fcntl(fd, libc::F_NOCACHE, 1) } == 0,
        "Cannot disable the OS file cache: {}",
        std::io::Error::last_os_error()
    );
    let mut buffer = vec![0_u8; 1024 * 1024];
    File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    let write_start = Instant::now();
    for _ in 0..size / buffer.len() as u64 {
        control.check()?;
        file.write_all(&buffer)?;
    }
    file.as_file().sync_all()?;
    let write_time = write_start.elapsed().as_secs_f64();
    file.seek(SeekFrom::Start(0))?;
    let mut read = vec![0; buffer.len()];
    let read_start = Instant::now();
    for _ in 0..size / buffer.len() as u64 {
        control.check()?;
        file.read_exact(&mut read)?;
        ensure!(read == buffer, "Read-back data verification failed");
    }
    Ok(Benchmark {
        directory,
        bytes: size,
        write_mbps: size as f64 / 1e6 / write_time,
        read_mbps: size as f64 / 1e6 / read_start.elapsed().as_secs_f64(),
        captured_unix: unix_now(),
    })
    // NamedTempFile removes only its own test file on success, error, or cancellation.
}

pub fn free_space(path: &Path) -> Result<u64> {
    use std::{ffi::CString, mem::MaybeUninit, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes())?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    ensure!(
        unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } == 0,
        "Cannot read free space"
    );
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail as u64 * stat.f_frsize)
}
