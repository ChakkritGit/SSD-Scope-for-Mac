//! Decode a captured USB snapshot offline. Never runs as administrator or opens a disk.
use crate::{
    command::{self, Control},
    disks::Health,
    smart,
    usb_sat::{AtaHealth, RawCapture},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: u32,
    pub captured_unix: u64,
    pub model: String,
    pub source: String,
    pub health: Health,
    pub driver_reattached: bool,
    pub warnings: Vec<String>,
}
pub fn load(path: &Path) -> Result<RawCapture> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    ensure!(
        f.metadata()?.is_file() && f.metadata()?.len() <= 64 * 1024,
        "Invalid capture file"
    );
    let mut data = Vec::new();
    Read::by_ref(&mut f)
        .take(64 * 1024 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= 64 * 1024, "Capture file too large");
    let report: RawCapture = serde_json::from_slice(&data)?;
    report.validate()?;
    Ok(report)
}
pub fn decode(report: &RawCapture, control: &Control) -> Result<Snapshot> {
    ensure!(
        unsafe { libc::geteuid() } != 0,
        "Decode captures as a regular user, after USB access is restored"
    );
    report.validate()?;
    let helper = smart::helper_path().context("Bundled smartctl is missing")?;
    let replay = report.replay()?;
    let output = command::run_input(
        helper.to_str().context("Invalid helper path")?,
        &["-j", "-i", "-A", "-H", "-"],
        Duration::from_secs(20),
        control,
        replay.as_bytes(),
    )?;
    ensure!(
        output.code >= 0 && output.code & 3 == 0,
        "Offline smartctl decoder failed: {}",
        output.text()
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Decoder returned invalid JSON")?;
    ensure!(
        value.get("model_name").and_then(|v| v.as_str()) == Some(report.model.as_str()),
        "Decoder model mismatch"
    );
    let mut health = smart::parse_health(&value, 0).context("No decoded SMART attributes")?;
    // Health status comes only from the actual ATA return registers, never a fallback estimate.
    health.status = match report.health()? {
        Some(AtaHealth::Passed) => "Verified",
        Some(AtaHealth::Failing) => "Failing",
        None => "Unknown",
    }
    .into();
    let mut warnings = report.notes.clone();
    if output.code & 4 != 0 {
        warnings.push("Some optional offline decoder commands were unavailable.".into());
    }
    if report.health()?.is_none() {
        warnings
            .push("SMART RETURN STATUS was unavailable; overall health remains Unknown.".into());
    }
    if !report.driver_reattached {
        warnings.push("USB driver restoration was not confirmed. Reconnect the SSD.".into());
    }
    warnings
        .push("This is a captured snapshot, not live temperature or continuous monitoring.".into());
    Ok(Snapshot {
        version: 1,
        captured_unix: report.captured_unix,
        model: report.model.clone(),
        source: "Direct USB BOT / SAT snapshot".into(),
        health,
        driver_reattached: report.driver_reattached,
        warnings,
    })
}
