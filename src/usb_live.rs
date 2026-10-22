//! One-click USB SMART refresh for bridges that macOS does not pass SMART through.
//! Runs only on an explicit user action: probe, normal (never forced) unmount,
//! administrator-approved capture by the separate helper, wait for the volume, then import.
use crate::{
    command::{self, Control},
    disks::{self, Drive},
    usb_cache,
    usb_report::Snapshot,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn helper() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let helper = exe
        .parent()
        .context("Missing executable folder")?
        .join("scope-usb-smart");
    ensure!(
        helper.is_file(),
        "The USB SMART helper is missing. Use the packaged app from the DMG."
    );
    Ok(helper)
}

fn quote(path: &Path) -> Result<String> {
    let text = path.to_str().context("Invalid path")?;
    ensure!(
        !text.contains(['\'', '"', '\\', '\n']),
        "Unsupported characters in path"
    );
    Ok(format!("'{text}'"))
}

pub fn read_now(drive: &Drive, control: &Control) -> Result<Snapshot> {
    ensure!(
        !drive.internal && drive.bus == "USB",
        "Only external USB drives use this reader"
    );
    let exe = std::env::current_exe()?;
    for volume in &drive.volumes {
        ensure!(
            !exe.starts_with(&volume.mount),
            "SSD Scope is running from {}. Install it in Applications first: a drive cannot be unmounted while the app runs from it.",
            volume.name
        );
    }
    let uuids: Vec<String> = drive
        .volumes
        .iter()
        .filter_map(|v| v.uuid.clone())
        .collect();
    ensure!(
        !uuids.is_empty(),
        "Mount the SSD's data volume first so it can be found again after reading"
    );
    let helper = helper()?;
    let probe = command::run(
        helper.to_str().context("Invalid helper path")?,
        &["probe", &drive.id],
        Duration::from_secs(30),
        control,
    )?
    .success()?;
    let plan: serde_json::Value = serde_json::from_slice(&probe.stdout)?;
    let registry = plan
        .pointer("/usb/registry_id")
        .and_then(|v| v.as_u64())
        .filter(|r| *r > 0)
        .context("The USB bridge could not be identified")?;
    let warnings = plan.get("warnings").and_then(|w| w.as_array());
    ensure!(
        plan.get("bot_available").and_then(|v| v.as_bool()) == Some(true)
            && warnings.is_none_or(|w| w.is_empty()),
        "This USB bridge is not supported by the reader: {}",
        warnings
            .map(|w| w
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("; "))
            .unwrap_or_default()
    );
    control.check()?;
    let folder = tempfile::Builder::new()
        .prefix("ssd-scope-smart.")
        .tempdir()?;
    std::fs::set_permissions(folder.path(), std::fs::Permissions::from_mode(0o700))?;
    let capture = folder.path().join("capture.json");
    let errors = folder.path().join("capture-error.txt");
    let unmount = command::run(
        "/usr/sbin/diskutil",
        &["unmountDisk", &drive.id],
        Duration::from_secs(60),
        control,
    )?;
    if unmount.code != 0 {
        let users = volume_users(drive, control);
        bail!(
            "macOS could not unmount {}. {}\n{}",
            drive.name,
            if users.is_empty() {
                "Close files, apps and Terminal windows using it, then try again.".to_string()
            } else {
                format!(
                    "These apps are using it: {}. Quit them (or change Terminal folders away from the drive), then try again.",
                    users.join(", ")
                )
            },
            unmount.text().trim()
        );
    }
    let shell = format!(
        "{} capture-read {} --registry-id {registry} --allow-usb-capture > {} 2> {}",
        quote(&helper)?,
        drive.id,
        quote(&capture)?,
        quote(&errors)?
    );
    let script = format!(
        "do shell script \"{shell}\" with administrator privileges with prompt \"SSD Scope needs permission to read SMART data from {}. The drive is unmounted while it is read.\"",
        drive.name.replace(['"', '\\'], "")
    );
    let result = command::run(
        "/usr/bin/osascript",
        &["-e", &script],
        Duration::from_secs(300),
        control,
    );
    let drive_back = wait_for_volume(&drive.id, &uuids, control);
    let result = result?;
    let detail = std::fs::read_to_string(&errors).unwrap_or_default();
    let captured = std::fs::metadata(&capture).is_ok_and(|m| m.len() > 0);
    write_log(&format!(
        "osascript exit {}\n{}\nhelper stderr:\n{}\ncapture written: {captured}\ndrive back: {}",
        result.code,
        result.stderr.trim(),
        detail.trim(),
        match &drive_back {
            Ok(d) => d.id.clone(),
            Err(e) => format!("{e:#}"),
        }
    ));
    if result.code != 0 && result.stderr.contains("-128") {
        bail!("Cancelled. The drive will be mounted again by macOS.");
    }
    // Exit 2 means SMART was read but the helper could not confirm the macOS driver was
    // restored. The reading is still valid once the volume is back.
    if result.code != 0 && !captured {
        bail!(
            "SMART read failed: {}{}",
            detail.trim(),
            result.stderr.trim()
        );
    }
    let drive = drive_back?;
    usb_cache::import(&capture, &drive, &usb_cache::default_path(), control)
}

/// Private diagnostics for the last read (no SMART data or serials), next to the settings.
fn write_log(text: &str) {
    use std::io::Write;
    let path = usb_cache::default_path().with_file_name("last-smart-read.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
    {
        let _ = writeln!(file, "time {}\n{text}", disks::unix_now());
    }
}

/// Names of processes with files or working folders open on the drive's volumes.
fn volume_users(drive: &Drive, control: &Control) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for volume in &drive.volumes {
        let Ok(output) = command::run(
            "/usr/sbin/lsof",
            &["-n", "-w", "-Fpc", "--", &volume.mount],
            Duration::from_secs(20),
            control,
        ) else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let mut pid = "";
        for line in text.lines() {
            if let Some(value) = line.strip_prefix('p') {
                pid = value;
            } else if let Some(name) = line.strip_prefix('c') {
                let name = readable_name(name, pid, control);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names.truncate(8);
    names
}

/// Some executables are named after their version (for example "2.1.308"); show the
/// command the user started instead.
fn readable_name(name: &str, pid: &str, control: &Control) -> String {
    if name.chars().any(char::is_alphabetic) || pid.is_empty() {
        return name.to_string();
    }
    command::run(
        "/bin/ps",
        &["-o", "command=", "-p", pid],
        Duration::from_secs(5),
        control,
    )
    .ok()
    .and_then(|output| {
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let first = text.split_whitespace().next()?;
        Some(first.rsplit('/').next().unwrap_or(first).to_string())
    })
    .unwrap_or_else(|| name.to_string())
}

/// After the helper restores the driver macOS re-enumerates the disk; its number may change.
fn wait_for_volume(old_id: &str, uuids: &[String], control: &Control) -> Result<Drive> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut mount_requested = false;
    loop {
        if let Ok(inventory) = disks::inventory()
            && let Some(drive) = inventory.drives.into_iter().find(|d| {
                d.volumes
                    .iter()
                    .any(|v| v.uuid.as_ref().is_some_and(|u| uuids.contains(u)))
            })
        {
            return Ok(drive);
        }
        if !mount_requested && Instant::now() + Duration::from_secs(105) > deadline {
            mount_requested = true;
            let _ = command::run(
                "/usr/sbin/diskutil",
                &["mountDisk", old_id],
                Duration::from_secs(30),
                control,
            );
        }
        ensure!(
            Instant::now() < deadline,
            "The drive did not come back. Unplug it and connect it again."
        );
        std::thread::sleep(Duration::from_secs(1));
    }
}
