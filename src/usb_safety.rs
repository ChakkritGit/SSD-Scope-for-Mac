//! Independent APFS-to-kernel-mount check before any USB capture.
use crate::usb_sat::physical_id;
use anyhow::{Context, Result, ensure};
use plist::{Dictionary, Value};

fn array<'a>(d: &'a Dictionary, key: &str) -> Result<&'a Vec<Value>> {
    d.get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("Missing {key} discovery array"))
}
fn text<'a>(d: &'a Dictionary, key: &str) -> Result<&'a str> {
    d.get(key)
        .and_then(Value::as_string)
        .with_context(|| format!("Missing {key} discovery field"))
}
fn dict(v: &Value) -> Result<&Dictionary> {
    v.as_dictionary().context("Invalid discovery dictionary")
}
pub fn descendant(device: &str, root: &str) -> bool {
    device == root
        || device
            .strip_prefix(root)
            .and_then(|s| s.strip_prefix('s'))
            .is_some_and(|s| {
                !s.is_empty()
                    && s.split('s')
                        .all(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            })
}
pub fn device_roots(physical: &Dictionary, apfs: &Dictionary, id: &str) -> Result<Vec<String>> {
    ensure!(physical_id(id), "Invalid whole-disk identifier");
    let mut target = None;
    for n in array(physical, "AllDisksAndPartitions")? {
        let node = dict(n)?;
        if text(node, "DeviceIdentifier")? == id {
            ensure!(target.is_none(), "Duplicate physical disk");
            target = Some(node);
        }
    }
    let target = target.context("Physical disk missing from mount discovery")?;
    let mut needs_apfs = false;
    if let Some(partitions) = target.get("Partitions") {
        for p in partitions.as_array().context("Invalid partition array")? {
            let p = dict(p)?;
            ensure!(
                descendant(text(p, "DeviceIdentifier")?, id),
                "Partition belongs to another disk"
            );
            needs_apfs |= text(p, "Content")?.starts_with("Apple_APFS");
        }
    }
    let mut roots = vec![id.to_owned()];
    for container in array(apfs, "Containers")? {
        let c = dict(container)?;
        let mut matches = false;
        for s in array(c, "PhysicalStores")? {
            matches |= descendant(text(dict(s)?, "DeviceIdentifier")?, id);
        }
        if matches {
            let root = text(c, "ContainerReference")?;
            ensure!(
                physical_id(root),
                "Invalid APFS synthesized disk identifier"
            );
            roots.push(root.into());
        }
    }
    ensure!(
        !needs_apfs || roots.len() > 1,
        "APFS physical-store mapping is incomplete; refusing capture"
    );
    roots.sort();
    roots.dedup();
    Ok(roots)
}

#[derive(Debug, Clone)]
pub struct Mount {
    pub device: String,
    pub path: String,
    pub filesystem: String,
}
pub fn selected_mounts(roots: &[String], mounts: &[Mount]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for m in mounts {
        let id = m.device.strip_prefix("/dev/");
        if m.filesystem == "apfs" {
            ensure!(
                id.is_some_and(|s| s.starts_with("disk")),
                "Unrecognized APFS kernel mount source"
            );
        }
        if id.is_some_and(|id| roots.iter().any(|root| descendant(id, root))) {
            paths.push(m.path.clone());
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

#[cfg(target_os = "macos")]
pub fn kernel_mounts() -> Result<Vec<Mount>> {
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    ensure!(
        count > 0 && count < 2048,
        "Could not obtain bounded kernel mount inventory"
    );
    let capacity = count as usize + 16;
    let mut rows: Vec<libc::statfs> = (0..capacity)
        .map(|_| unsafe { std::mem::zeroed() })
        .collect();
    let bytes = i32::try_from(capacity * std::mem::size_of::<libc::statfs>())?;
    let n = unsafe { libc::getfsstat(rows.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    ensure!(
        n > 0 && (n as usize) < capacity,
        "Kernel mount inventory changed or failed"
    );
    fn string(chars: &[libc::c_char]) -> Result<String> {
        let end = chars
            .iter()
            .position(|c| *c == 0)
            .context("Unterminated kernel mount field")?;
        Ok(
            String::from_utf8_lossy(&chars[..end].iter().map(|c| *c as u8).collect::<Vec<_>>())
                .into_owned(),
        )
    }
    rows.iter()
        .take(n as usize)
        .map(|r| {
            Ok(Mount {
                device: string(&r.f_mntfromname)?,
                path: string(&r.f_mntonname)?,
                filesystem: string(&r.f_fstypename)?,
            })
        })
        .collect()
}
