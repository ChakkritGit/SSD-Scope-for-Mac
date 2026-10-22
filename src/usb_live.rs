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
    os::unix::fs::PermissionsExt,
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
        bail!(
            "macOS could not unmount {}. Close files, apps and Terminal windows using it, then try again.\n{}",
            drive.name,
            unmount.text()
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
    if result.code != 0 {
        let detail = std::fs::read_to_string(&errors).unwrap_or_default();
        if result.text().contains("-128") {
            bail!("Cancelled. The drive will be mounted again by macOS.");
        }
        bail!(
            "SMART read failed: {}{}",
            detail.trim(),
            result.text().trim()
        );
    }
    let drive = drive_back?;
    usb_cache::import(&capture, &drive, &usb_cache::default_path(), control)
}

/// After the helper restores the driver macOS re-enumerates the disk; its number may change.
fn wait_for_volume(old_id: &str, uuids: &[String], control: &Control) -> Result<Drive> {
    let deadline = Instant::now() + Duration::from_secs(45);
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
        if !mount_requested && Instant::now() + Duration::from_secs(38) > deadline {
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
