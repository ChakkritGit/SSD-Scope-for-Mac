//! Private, explicitly imported snapshots. Never performs USB capture or reads raw disks.
use crate::{
    command::Control,
    disks::{self, Drive, Inventory},
    native::{self, UsbBridge},
    usb_report::{self, Snapshot},
    usb_sat::{self, RawCapture},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const LIMIT: usize = 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCapture {
    pub volume_uuid: String,
    pub capacity: u64,
    pub vendor_id: u16,
    pub product_id: u16,
    pub capture: RawCapture,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    pub version: u32,
    pub captures: Vec<SavedCapture>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            version: 1,
            captures: Vec::new(),
        }
    }
}
fn valid_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
impl SavedCapture {
    pub fn bind(capture: RawCapture, drive: &Drive, usb: &UsbBridge) -> Result<Self> {
        capture.validate()?;
        ensure!(
            !drive.internal
                && drive.bus == "USB"
                && (usb.vendor_id, usb.product_id) == (0x0bda, 0x9201),
            "Select the reviewed external USB SSD"
        );
        let capacity =
            usb_sat::identify_capacity(&usb_sat::unhex(&capture.identity_redacted_hex, 512)?)?;
        ensure!(
            capacity == drive.capacity,
            "Captured ATA capacity differs from the selected drive"
        );
        let uuid = drive
            .volumes
            .iter()
            .find_map(|v| v.uuid.as_deref().filter(|u| valid_uuid(u)))
            .context("Mount the SSD's data volume before importing a snapshot")?;
        let saved = Self {
            volume_uuid: uuid.to_ascii_lowercase(),
            capacity,
            vendor_id: usb.vendor_id,
            product_id: usb.product_id,
            capture,
        };
        saved.validate()?;
        Ok(saved)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            valid_uuid(&self.volume_uuid) && (self.vendor_id, self.product_id) == (0x0bda, 0x9201),
            "Invalid snapshot binding"
        );
        self.capture.validate()?;
        ensure!(
            self.capture.captured_unix > 0
                && self.capture.captured_unix <= disks::unix_now().saturating_add(300),
            "Invalid snapshot capture time"
        );
        ensure!(
            self.capacity
                == usb_sat::identify_capacity(&usb_sat::unhex(
                    &self.capture.identity_redacted_hex,
                    512
                )?)?,
            "Invalid bound ATA capacity"
        );
        Ok(())
    }
    pub fn matches(&self, drive: &Drive, usb: &UsbBridge) -> bool {
        !drive.internal
            && drive.bus == "USB"
            && drive.capacity == self.capacity
            && (usb.vendor_id, usb.product_id) == (self.vendor_id, self.product_id)
            && drive.volumes.iter().any(|v| {
                v.uuid
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&self.volume_uuid))
            })
    }
}
impl Store {
    pub fn load(path: &Path) -> Result<Self> {
        let mut file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    captures: Vec::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.len() <= LIMIT as u64
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.mode() & 0o077 == 0,
            "Snapshot cache must be a bounded private owned regular file"
        );
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= LIMIT, "Snapshot cache too large");
        let value: Self = serde_json::from_slice(&bytes)?;
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && self.captures.len() <= 16,
            "Unsupported snapshot cache"
        );
        let mut uuids = HashSet::new();
        for saved in &self.captures {
            saved.validate()?;
            ensure!(
                uuids.insert(saved.volume_uuid.to_ascii_lowercase()),
                "Duplicate snapshot binding"
            );
        }
        Ok(())
    }
    pub fn upsert(&mut self, saved: SavedCapture) -> Result<()> {
        saved.validate()?;
        self.captures
            .retain(|v| !v.volume_uuid.eq_ignore_ascii_case(&saved.volume_uuid));
        ensure!(self.captures.len() < 16, "Snapshot cache is full");
        self.captures.push(saved);
        self.validate()
    }
    pub fn remove(&mut self, drive: &Drive) {
        self.captures.retain(|saved| {
            !drive.volumes.iter().any(|v| {
                v.uuid
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&saved.volume_uuid))
            })
        });
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        ensure!(bytes.len() <= LIMIT, "Snapshot cache too large");
        let parent = path.parent().context("Missing snapshot cache directory")?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let metadata = fs::symlink_metadata(parent)?;
        ensure!(
            metadata.is_dir()
                && metadata.uid() == unsafe { libc::geteuid() }
                && metadata.mode() & 0o077 == 0,
            "Snapshot cache directory must be private and owned"
        );
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }
    pub fn resolve(
        &self,
        inventory: &Inventory,
        control: &Control,
    ) -> (HashMap<String, Snapshot>, Vec<String>) {
        let mut snapshots = HashMap::new();
        let mut warnings = Vec::new();
        for drive in inventory
            .drives
            .iter()
            .filter(|d| !d.internal && d.bus == "USB")
        {
            let Some(saved) = self.captures.iter().find(|s| {
                drive.volumes.iter().any(|v| {
                    v.uuid
                        .as_deref()
                        .is_some_and(|u| u.eq_ignore_ascii_case(&s.volume_uuid))
                })
            }) else {
                continue;
            };
            let count = inventory
                .drives
                .iter()
                .filter(|d| {
                    d.volumes.iter().any(|v| {
                        v.uuid
                            .as_deref()
                            .is_some_and(|u| u.eq_ignore_ascii_case(&saved.volume_uuid))
                    })
                })
                .count();
            if count != 1 {
                warnings
                    .push("Duplicate mounted volume UUID; ignoring saved SMART snapshot.".into());
                continue;
            }
            let result = (|| -> Result<Snapshot> {
                let bridge = native::inspect(&drive.id)?
                    .usb
                    .context("USB identity unavailable")?;
                ensure!(
                    saved.matches(drive, &bridge),
                    "Saved snapshot binding does not match this drive"
                );
                usb_report::decode(&saved.capture, control)
            })();
            match result {
                Ok(snapshot) => {
                    snapshots.insert(drive.id.clone(), snapshot);
                }
                Err(e) => warnings.push(format!("Snapshot for {}: {e:#}", drive.id)),
            }
        }
        (snapshots, warnings)
    }
}
pub fn default_path() -> PathBuf {
    crate::settings::default_path().with_file_name("usb-captures.json")
}
pub fn import(
    path: &Path,
    selected: &Drive,
    cache_path: &Path,
    control: &Control,
) -> Result<Snapshot> {
    control.check()?;
    let capture = usb_report::load(path)?;
    let inventory = disks::inventory()?;
    let drive = inventory
        .drives
        .iter()
        .find(|d| d.id == selected.id)
        .context("Selected drive is no longer connected")?;
    ensure!(
        selected.capacity == drive.capacity
            && selected
                .volumes
                .iter()
                .filter_map(|v| v.uuid.as_deref())
                .any(|uuid| drive
                    .volumes
                    .iter()
                    .any(|v| v.uuid.as_deref() == Some(uuid))),
        "Selected volume identity changed; refresh and choose the SSD again"
    );
    let usb = native::inspect(&drive.id)?.usb.context("No USB bridge")?;
    let saved = SavedCapture::bind(capture, drive, &usb)?;
    let snapshot = usb_report::decode(&saved.capture, control)?;
    let mut store = Store::load(cache_path)?;
    store.upsert(saved)?;
    control.check()?;
    store.save(cache_path)?;
    Ok(snapshot)
}
