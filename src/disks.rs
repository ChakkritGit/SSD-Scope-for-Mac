use crate::command::{self, Control};
use anyhow::{Context, Result, bail};
use plist::{Dictionary, Value};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::Cursor,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub uuid: Option<String>,
    pub name: String,
    pub mount: String,
    pub filesystem: String,
    pub capacity: u64,
    pub free: Option<u64>,
    pub writable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    pub temperature_c: Option<f64>,
    pub percentage_used: Option<f64>,
    pub power_on_hours: Option<f64>,
    pub power_cycles: Option<f64>,
    pub unsafe_shutdowns: Option<f64>,
    pub media_errors: Option<f64>,
    pub bytes_read: Option<f64>,
    pub bytes_written: Option<f64>,
    pub raw: BTreeMap<String, f64>,
}
impl Health {
    pub fn remaining_life_estimate(&self) -> Option<f64> {
        self.percentage_used.map(|used| (100.0 - used).max(0.0))
    }
    pub fn parse(info: &Dictionary) -> Self {
        let empty = Dictionary::new();
        let values = info
            .get("SMARTDeviceSpecificKeysMayVaryNotGuaranteed")
            .and_then(Value::as_dictionary)
            .unwrap_or(&empty);
        let raw: BTreeMap<String, f64> = values
            .iter()
            .filter_map(|(key, value)| number(value).map(|v| (key.clone(), v)))
            .collect();
        let counter = |key: &str| -> Option<f64> {
            raw.get(&format!("{key}_0"))
                .map(|low| {
                    low + raw.get(&format!("{key}_1")).unwrap_or(&0.0)
                        * 18_446_744_073_709_551_616.0
                })
                .or_else(|| raw.get(key).copied())
        };
        Self {
            status: text(info, "SMARTStatus").unwrap_or("Unknown").into(),
            temperature_c: raw
                .get("TEMPERATURE")
                .filter(|&&n| n > 0.0)
                .map(|n| n - 273.15),
            percentage_used: raw
                .get("PERCENTAGE_USED")
                .filter(|&&n| (0.0..=255.0).contains(&n))
                .copied(),
            power_on_hours: counter("POWER_ON_HOURS"),
            power_cycles: counter("POWER_CYCLES"),
            unsafe_shutdowns: counter("UNSAFE_SHUTDOWNS"),
            media_errors: counter("MEDIA_ERRORS"),
            bytes_read: counter("DATA_UNITS_READ").map(|n| n * 512_000.0),
            bytes_written: counter("DATA_UNITS_WRITTEN").map(|n| n * 512_000.0),
            raw,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub name: String,
    pub capacity: u64,
    pub internal: bool,
    pub ssd: Option<bool>,
    pub bus: String,
    pub health: Health,
    pub volumes: Vec<Volume>,
    pub captured_unix: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Inventory {
    pub drives: Vec<Drive>,
    pub warnings: Vec<String>,
}

pub fn valid_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix("disk") else {
        return false;
    };
    rest.split('s')
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}
fn number(value: &Value) -> Option<f64> {
    value
        .as_unsigned_integer()
        .map(|v| v as f64)
        .or_else(|| value.as_signed_integer().map(|v| v as f64))
        .or_else(|| value.as_real())
}
fn text<'a>(dict: &'a Dictionary, key: &str) -> Option<&'a str> {
    dict.get(key).and_then(Value::as_string)
}
fn uint(dict: &Dictionary, key: &str) -> Option<u64> {
    dict.get(key).and_then(Value::as_unsigned_integer)
}
fn flag(dict: &Dictionary, key: &str) -> Option<bool> {
    dict.get(key).and_then(Value::as_boolean)
}
fn array<'a>(dict: &'a Dictionary, key: &str) -> Vec<&'a Dictionary> {
    dict.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_dictionary)
        .collect()
}
pub fn parse_plist(bytes: &[u8]) -> Result<Dictionary> {
    Value::from_reader(Cursor::new(bytes))?
        .into_dictionary()
        .context("Expected a macOS disk property list")
}
pub fn info(id: &str) -> Result<Dictionary> {
    if !valid_id(id) {
        bail!("Invalid disk identifier")
    }
    parse_plist(
        &command::quick("/usr/sbin/diskutil", &["info", "-plist", id])?
            .success()?
            .stdout,
    )
}
pub fn inventory() -> Result<Inventory> {
    let all = parse_plist(
        &command::quick("/usr/sbin/diskutil", &["list", "-plist"])?
            .success()?
            .stdout,
    )?;
    let physical = parse_plist(
        &command::quick("/usr/sbin/diskutil", &["list", "-plist", "physical"])?
            .success()?
            .stdout,
    )?;
    let mut result = Inventory::default();
    for node in array(&physical, "AllDisksAndPartitions") {
        let Some(id) = text(node, "DeviceIdentifier").filter(|id| valid_id(id)) else {
            continue;
        };
        match load_drive(id, node, &all, &mut result.warnings) {
            Ok(drive) => result.drives.push(drive),
            Err(error) => result.warnings.push(format!("{id}: {error:#}")),
        }
    }
    Ok(result)
}
fn load_drive(
    id: &str,
    physical: &Dictionary,
    all: &Dictionary,
    warnings: &mut Vec<String>,
) -> Result<Drive> {
    let details = info(id)?;
    let mut related = vec![physical];
    for node in array(all, "AllDisksAndPartitions") {
        if array(node, "APFSPhysicalStores").iter().any(|store| {
            text(store, "DeviceIdentifier")
                .is_some_and(|s| s == id || s.starts_with(&format!("{id}s")))
        }) {
            related.push(node);
        }
    }
    let mut seen = HashSet::new();
    let mut volumes = Vec::new();
    for node in related {
        for child in array(node, "Partitions")
            .into_iter()
            .chain(array(node, "APFSVolumes"))
        {
            let Some(volume_id) = text(child, "DeviceIdentifier") else {
                continue;
            };
            let Some(mount) = text(child, "MountPoint").filter(|m| !m.is_empty()) else {
                continue;
            };
            if mount.starts_with("/System/") && mount != "/System/Volumes/Data" {
                continue;
            }
            if !seen.insert(volume_id.to_owned()) {
                continue;
            }
            match info(volume_id) {
                Ok(v) => volumes.push(Volume {
                    id: volume_id.into(),
                    uuid: text(&v, "VolumeUUID").map(str::to_owned),
                    name: text(&v, "VolumeName").unwrap_or(volume_id).into(),
                    mount: mount.into(),
                    filesystem: text(&v, "FilesystemName")
                        .or_else(|| text(&v, "Content"))
                        .unwrap_or("Unknown")
                        .into(),
                    capacity: uint(&v, "TotalSize").unwrap_or(0),
                    free: uint(&v, "APFSContainerFree").or_else(|| uint(&v, "VolumeFreeSpace")),
                    writable: flag(&v, "WritableVolume").unwrap_or(false),
                }),
                Err(error) => warnings.push(format!("{volume_id}: {error:#}")),
            }
        }
    }
    Ok(Drive {
        id: id.into(),
        name: text(&details, "MediaName").unwrap_or(id).into(),
        capacity: uint(&details, "TotalSize").unwrap_or(0),
        internal: flag(&details, "Internal").unwrap_or(false),
        ssd: flag(&details, "SolidState"),
        bus: text(&details, "BusProtocol").unwrap_or("Unknown").into(),
        health: Health::parse(&details),
        volumes,
        captured_unix: unix_now(),
    })
}
pub fn verify_volume(volume: &Volume, control: &Control) -> Result<String> {
    let current = info(&volume.id)?;
    if flag(&current, "WholeDisk") != Some(false)
        || text(&current, "MountPoint") != Some(&volume.mount)
        || volume
            .uuid
            .as_deref()
            .is_none_or(|uuid| text(&current, "VolumeUUID") != Some(uuid))
    {
        bail!("Volume identity changed. Refresh drives and select it again.")
    }
    Ok(command::run(
        "/usr/sbin/diskutil",
        &["verifyVolume", &volume.id],
        Duration::from_secs(3600),
        control,
    )?
    .success()?
    .text())
}
pub fn smartctl_path() -> Option<&'static str> {
    [
        "/opt/homebrew/sbin/smartctl",
        "/opt/homebrew/bin/smartctl",
        "/usr/local/sbin/smartctl",
        "/usr/local/bin/smartctl",
    ]
    .into_iter()
    .find(|p| Path::new(p).is_file())
}
pub fn extended_smart(id: &str, control: &Control) -> Result<String> {
    Ok(serde_json::to_string_pretty(&crate::smart::diagnose(
        id, control,
    )?)?)
}
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn bytes(n: u64) -> String {
    let mut value = n as f64;
    for unit in ["B", "KB", "MB", "GB", "TB", "PB"] {
        if value < 1000.0 || unit == "PB" {
            return format!("{value:.1} {unit}");
        }
        value /= 1000.0;
    }
    unreachable!()
}
