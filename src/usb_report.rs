//! Decode a captured USB snapshot offline. Never runs as administrator or opens a disk.
use crate::{
    command::{self, Control},
    disks::Health,
    smart,
    usb_sat::{AtaHealth, RawCapture},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap, fs, io::Read, os::unix::fs::OpenOptionsExt, path::Path, time::Duration,
};

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
    #[serde(default)]
    pub ata_attributes: Vec<AtaAttribute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtaAttribute {
    pub id: u8,
    pub name: String,
    pub normalized: Option<u8>,
    pub worst: Option<u8>,
    pub threshold: Option<u8>,
    pub raw: String,
}
impl Snapshot {
    pub fn age(&self, now: u64) -> String {
        let seconds = now.saturating_sub(self.captured_unix);
        match seconds {
            0..60 => format!("{seconds}s ago"),
            60..3600 => format!("{}m ago", seconds / 60),
            3600..86400 => format!("{}h ago", seconds / 3600),
            _ => format!("{}d ago", seconds / 86400),
        }
    }
    pub fn captured_at(&self) -> String {
        let Ok(time) = libc::time_t::try_from(self.captured_unix) else {
            return self.captured_unix.to_string();
        };
        let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
        if unsafe { libc::gmtime_r(&time, &mut tm) }.is_null() {
            return self.captured_unix.to_string();
        }
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02} UTC",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        )
    }
    pub fn label(&self) -> String {
        format!(
            "SMART snapshot · {} · {} · {} · not live",
            self.model,
            self.captured_at(),
            self.age(crate::disks::unix_now())
        )
    }
    pub fn threshold_counts(&self) -> (usize, usize) {
        self.ata_attributes
            .iter()
            .filter_map(|a| a.normalized.zip(a.threshold))
            .filter(|(_, threshold)| *threshold > 0)
            .fold((0, 0), |(checked, failed), (value, threshold)| {
                (checked + 1, failed + usize::from(value <= threshold))
            })
    }
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
/// Transcend's Windows SSD Scope derives SATA health as
/// `(Max_Erase_Count_of_Spec - Average_Erase_Count) / Max_Erase_Count_of_Spec`.
/// The same ratio is reported here as percentage used (it may exceed 100).
/// Normalized values stay at 100 on this firmware and are not used.
pub fn erase_count_wear(raw: &BTreeMap<String, f64>) -> Option<f64> {
    let average = *raw.get("Average_Erase_Count")?;
    let rated = *raw.get("Max_Erase_Count_of_Spec")?;
    (rated > 0.0 && average >= 0.0).then(|| (average / rated * 100.0).min(255.0))
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
    health.percentage_used = erase_count_wear(&health.raw);
    // Transcend firmware counts host transfers in 32 MiB units (attributes 241/242).
    let mib32 = |name: &str| health.raw.get(name).map(|n| n * 32.0 * 1024.0 * 1024.0);
    health.bytes_written = health.bytes_written.or(mib32("Host_Writes_32MiB"));
    health.bytes_read = health.bytes_read.or(mib32("Host_Reads_32MiB"));
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
    if report.health()?.is_none()
        && !warnings
            .iter()
            .any(|m| m.contains("overall health remains Unknown"))
    {
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
        ata_attributes: value
            .pointer("/ata_smart_attributes/table")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .take(30)
            .filter_map(|a| {
                let number = |key: &str| {
                    a.get(key)
                        .and_then(|v| v.as_u64())
                        .and_then(|v| u8::try_from(v).ok())
                };
                Some(AtaAttribute {
                    id: number("id")?,
                    name: a.get("name")?.as_str()?.chars().take(128).collect(),
                    normalized: number("value"),
                    worst: number("worst"),
                    threshold: number("thresh"),
                    raw: a
                        .pointer("/raw/string")
                        .and_then(|v| v.as_str())
                        .map(str::to_owned)
                        .or_else(|| a.pointer("/raw/value").map(ToString::to_string))
                        .unwrap_or_default()
                        .chars()
                        .take(128)
                        .collect(),
                })
            })
            .collect(),
    })
}
