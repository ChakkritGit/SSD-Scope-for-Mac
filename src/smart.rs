use crate::{
    command::{self, Control},
    disks::{self, Health},
    native::{self, UsbBridge},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attempt {
    pub mode: String,
    pub exit_code: i32,
    pub messages: Vec<String>,
    pub response: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SmartReport {
    pub device: String,
    pub captured_unix: u64,
    pub bridge: Option<UsbBridge>,
    pub helper: String,
    pub attempts: Vec<Attempt>,
    pub health: Option<Health>,
    pub conclusion: String,
    pub explanation: String,
}

pub fn helper_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    // Packaged executable first, then a local development build, then standard installations.
    let mut candidates = vec![
        executable.parent()?.join("smartctl"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/smartctl"),
    ];
    candidates.extend(
        [
            "/opt/homebrew/sbin/smartctl",
            "/opt/homebrew/bin/smartctl",
            "/usr/local/sbin/smartctl",
            "/usr/local/bin/smartctl",
        ]
        .map(PathBuf::from),
    );
    candidates.into_iter().find(|p| p.is_file())
}

pub fn parse_health(value: &serde_json::Value, exit_code: i32) -> Option<Health> {
    // Command/open/read errors are not health results. Bits 3–7 remain valid health warnings.
    if exit_code < 0 || exit_code & 7 != 0 {
        return None;
    }
    let passed = value
        .pointer("/smart_status/passed")
        .and_then(|v| v.as_bool());
    let nvme = value.get("nvme_smart_health_information_log");
    if passed.is_none() && nvme.is_none() && value.get("ata_smart_attributes").is_none() {
        return None;
    }
    let number = |pointer: &str| value.pointer(pointer).and_then(|v| v.as_f64());
    let mut raw = BTreeMap::new();
    if let Some(attributes) = value
        .pointer("/ata_smart_attributes/table")
        .and_then(|v| v.as_array())
    {
        for attr in attributes {
            if let (Some(name), Some(value)) = (
                attr.get("name").and_then(|v| v.as_str()),
                attr.pointer("/raw/value").and_then(|v| v.as_f64()),
            ) {
                raw.insert(name.into(), value);
            }
        }
    }
    if let Some(values) = nvme.and_then(|v| v.as_object()) {
        for (key, value) in values {
            if let Some(value) = value.as_f64() {
                raw.insert(key.clone(), value);
            }
        }
    }
    Some(Health {
        status: match passed {
            Some(true) => "Verified",
            Some(false) => "Failing",
            None => "Unknown",
        }
        .into(),
        temperature_c: number("/temperature/current")
            .or_else(|| number("/nvme_smart_health_information_log/temperature")),
        percentage_used: number("/nvme_smart_health_information_log/percentage_used")
            .filter(|v| (0.0..=255.0).contains(v)),
        power_on_hours: number("/power_on_time/hours")
            .or_else(|| number("/nvme_smart_health_information_log/power_on_hours")),
        power_cycles: number("/power_cycle_count")
            .or_else(|| number("/nvme_smart_health_information_log/power_cycles")),
        unsafe_shutdowns: number("/nvme_smart_health_information_log/unsafe_shutdowns"),
        media_errors: number("/nvme_smart_health_information_log/media_errors"),
        bytes_read: number("/nvme_smart_health_information_log/data_units_read")
            .map(|v| v * 512_000.0),
        bytes_written: number("/nvme_smart_health_information_log/data_units_written")
            .map(|v| v * 512_000.0),
        raw,
    })
}

pub fn diagnose(id: &str, control: &Control) -> Result<SmartReport> {
    ensure!(disks::valid_id(id), "Invalid disk identifier");
    let current = disks::info(id)?;
    ensure!(
        current.get("WholeDisk").and_then(plist::Value::as_boolean) == Some(true),
        "Select a physical disk"
    );
    let bridge = native::inspect(id)?.usb;
    let helper =
        helper_path().context("Bundled smartctl is missing. Rebuild with scripts/package.sh")?;
    let node = format!("/dev/{id}");
    let mut report = SmartReport {
        device: id.into(),
        captured_unix: disks::unix_now(),
        bridge,
        helper: helper.display().to_string(),
        attempts: Vec::new(),
        health: None,
        conclusion: "unavailable".into(),
        explanation: String::new(),
    };
    let modes: &[&str] = if report.bridge.is_some() {
        &["auto", "ata", "sat", "sat,12", "sat,16"]
    } else {
        &["auto"]
    };
    for mode in modes {
        control.check()?;
        let result = command::run(
            helper.to_str().context("Invalid helper path")?,
            &["-a", "-j", "-d", mode, &node],
            Duration::from_secs(20),
            control,
        )?;
        let response: serde_json::Value =
            serde_json::from_slice(&result.stdout).context("smartctl returned invalid JSON")?;
        let messages = response
            .pointer("/smartctl/messages")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.get("string").and_then(|v| v.as_str()).map(str::to_owned))
            .collect();
        let health = parse_health(&response, result.code);
        report.attempts.push(Attempt {
            mode: (*mode).into(),
            exit_code: result.code,
            messages,
            response,
        });
        if let Some(health) = health {
            report.health = Some(health);
            break;
        }
    }
    if report.health.is_some() {
        report.conclusion = "available".into();
        report.explanation = "SMART was read successfully with the current macOS driver.".into();
    } else {
        let all = report
            .attempts
            .iter()
            .flat_map(|a| &a.messages)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if all.to_lowercase().contains("permission") {
            report.conclusion = "permission_denied".into();
            report.explanation="macOS denied access to SMART. This is an access failure, not evidence that the SSD lacks SMART.".into();
        } else if report.bridge.as_ref().is_some_and(|b| !b.ata_smart_exposed)
            && all.contains("Not a device of type 'scsi'")
        {
            report.conclusion = "transport_unavailable".into();
            report.explanation="The app's current USB SMART transport did not return data: smartctl auto/ATA failed and its macOS SAT backend has no generic SCSI transport. Direct USB SMART access has not been implemented or validated in this app. A compatible SAT driver is one possible approach, not proof that other approaches are impossible. The SSD's SMART capability is not disproven.".into();
        } else {
            report.explanation="No valid SMART report was returned. Review the exact device responses below; missing values do not establish the SSD's health or capability.".into();
        }
    }
    Ok(report)
}
