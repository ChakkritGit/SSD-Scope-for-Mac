//! Experimental one-session USB capture. Never called by dashboard polling.
use crate::{
    disks,
    native::{self, UsbBridge},
    usb_safety,
    usb_sat::{self, RawCapture, ReadCommand, Request, SatMode, Status},
};
use anyhow::{Context as _, Result, bail, ensure};
use rusb::{Context, Device, DeviceHandle, Direction, TransferType, UsbContext};
use serde::Serialize;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub device: String,
    pub model: String,
    pub capacity: u64,
    pub usb: UsbBridge,
    pub mounted_paths: Vec<String>,
    pub warnings: Vec<String>,
    pub libusb_version: String,
    pub bot_available: bool,
}

pub fn plan(id: &str) -> Result<Plan> {
    ensure!(
        usb_sat::physical_id(id),
        "A physical diskN identifier is required"
    );
    let info = native::disk_description(id)?;
    ensure!(
        info.get("WholeDisk").and_then(plist::Value::as_boolean) == Some(true)
            && info.get("Internal").and_then(plist::Value::as_boolean) == Some(false)
            && info.get("BusProtocol").and_then(plist::Value::as_string) == Some("USB"),
        "Cached metadata must confirm a whole external USB disk"
    );
    let inventory = disks::inventory()?;
    let drive = inventory
        .drives
        .iter()
        .find(|d| d.id == id)
        .context("Physical disk not found")?;
    ensure!(
        !drive.internal && drive.bus == "USB",
        "Only external USB disks are accepted"
    );
    let usb = native::inspect(id)?.usb.context("No USB bridge identity")?;
    ensure!(
        (usb.vendor_id, usb.product_id) == (0x0bda, 0x9201),
        "Only the reviewed RTL9201 is accepted"
    );
    let ctx = Context::new()?;
    let device = matching_device(&ctx, &usb)?;
    let bot_available = bot_endpoints(&device).is_ok();
    let version = rusb::version();
    let physical = disks::parse_plist(
        &crate::command::quick("/usr/sbin/diskutil", &["list", "-plist", "physical"])?
            .success()?
            .stdout,
    )?;
    let apfs = disks::parse_plist(
        &crate::command::quick("/usr/sbin/diskutil", &["apfs", "list", "-plist"])?
            .success()?
            .stdout,
    )?;
    let roots = usb_safety::device_roots(&physical, &apfs, id)?;
    let mut mounted_paths: Vec<String> = drive.volumes.iter().map(|v| v.mount.clone()).collect();
    mounted_paths.extend(usb_safety::selected_mounts(
        &roots,
        &usb_safety::kernel_mounts()?,
    )?);
    mounted_paths.sort();
    mounted_paths.dedup();
    Ok(Plan {
        device: id.into(),
        model: drive.name.clone(),
        capacity: drive.capacity,
        usb,
        mounted_paths,
        warnings: inventory.warnings,
        libusb_version: format!(
            "{}.{}.{}",
            version.major(),
            version.minor(),
            version.micro()
        ),
        bot_available,
    })
}

pub fn check_capture_plan(plan: &Plan, expected_registry_id: u64) -> Result<()> {
    ensure!(
        usb_sat::physical_id(&plan.device),
        "Invalid physical identifier"
    );
    ensure!(
        (plan.usb.vendor_id, plan.usb.product_id) == (0x0bda, 0x9201),
        "Unreviewed USB bridge"
    );
    ensure!(
        expected_registry_id != 0 && plan.usb.registry_id == expected_registry_id,
        "USB identity changed; prepare a new probe"
    );
    ensure!(
        plan.usb.location_id != 0 && plan.usb.device_address != 0,
        "USB location/address is missing"
    );
    ensure!(
        plan.mounted_paths.is_empty(),
        "Drive is still mounted: {}. This helper never unmounts or forces access.",
        plan.mounted_paths.join(", ")
    );
    ensure!(
        plan.warnings.is_empty(),
        "Drive discovery was incomplete; refusing capture"
    );
    ensure!(
        plan.bot_available && plan.capacity > 0,
        "Reviewed BOT interface is unavailable"
    );
    Ok(())
}

fn matching_device(ctx: &Context, usb: &UsbBridge) -> Result<Device<Context>> {
    ensure!(
        usb.location_id != 0 && usb.device_address != 0,
        "Missing USB bus/address mapping"
    );
    let mut matches = Vec::new();
    for device in ctx.devices()?.iter() {
        let d = device.device_descriptor()?;
        if d.vendor_id() == usb.vendor_id
            && d.product_id() == usb.product_id
            && device.bus_number() == (usb.location_id >> 24) as u8
            && device.address() == usb.device_address
        {
            matches.push(device);
        }
    }
    ensure!(
        matches.len() == 1,
        "Could not uniquely map this physical disk to libusb"
    );
    Ok(matches.remove(0))
}
fn bot_endpoints(device: &Device<Context>) -> Result<(u8, u8)> {
    let config = device.active_config_descriptor()?;
    ensure!(config.number() == 1, "Unexpected USB configuration");
    for interface in config.interfaces() {
        for d in interface.descriptors() {
            if d.interface_number() == 0
                && d.setting_number() == 0
                && d.class_code() == 8
                && d.sub_class_code() == 6
                && d.protocol_code() == 0x50
            {
                let mut endpoints = Vec::new();
                for e in d.endpoint_descriptors() {
                    ensure!(
                        e.transfer_type() == TransferType::Bulk,
                        "Unexpected BOT endpoint type"
                    );
                    endpoints.push((e.direction(), e.address()));
                }
                ensure!(
                    endpoints.len() == 2
                        && endpoints.contains(&(Direction::In, 0x81))
                        && endpoints.contains(&(Direction::Out, 0x02)),
                    "Unexpected BOT endpoints"
                );
                return Ok((0x81, 0x02));
            }
        }
    }
    bail!("No reviewed interface 0 / alternate 0 BOT descriptor")
}

struct Session<'a> {
    handle: DeviceHandle<Context>,
    captured: bool,
    claimed: bool,
    closed: bool,
    input: u8,
    output: u8,
    tag: u32,
    started: Instant,
    cancelled: &'a AtomicBool,
}
impl<'a> Session<'a> {
    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancelled.load(Ordering::Relaxed),
            "USB diagnostic cancelled"
        );
        ensure!(
            self.started.elapsed() < Duration::from_secs(120),
            "USB diagnostic session expired"
        );
        Ok(())
    }
    fn execute(&mut self, request: Request) -> Result<(Status, Vec<u8>)> {
        self.tag = self.tag.checked_add(1).context("BOT tag exhausted")?;
        let tag = self.tag;
        usb_sat::transfer(self, request, tag)
    }
    fn sense(&mut self, mode: SatMode) -> Result<Option<Vec<u8>>> {
        let (status, bytes) = self.execute(Request::new(ReadCommand::RequestSense, mode))?;
        Ok((status == Status::Passed).then_some(bytes))
    }
    fn sector(
        &mut self,
        command: ReadCommand,
        mode: SatMode,
        notes: &mut Vec<String>,
    ) -> Result<Option<Vec<u8>>> {
        let (status, bytes) = self.execute(Request::new(command, mode))?;
        if status == Status::Passed {
            ensure!(bytes.len() == 512, "Short ATA sector response");
            return Ok(Some(bytes));
        }
        let sense = self.sense(mode)?;
        notes.push(format!(
            "{mode:?} {command:?}: SCSI command failed; sense={}",
            sense
                .as_deref()
                .map(usb_sat::hex)
                .unwrap_or_else(|| "unavailable".into())
        ));
        Ok(None)
    }
    fn finish(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        self.closed = true;
        let release = if self.claimed {
            self.claimed = false;
            self.handle.release_interface(0)
        } else {
            Ok(())
        };
        let attach = if self.captured {
            self.captured = false;
            // Do not retry attach: libusb decrements its capture count once per call.
            self.handle.attach_kernel_driver(0)
        } else {
            Ok(())
        };
        release.context("Could not release BOT interface")?;
        attach.context("Could not restore macOS driver; reconnect the USB drive")?;
        Ok(())
    }
}
impl usb_sat::BulkIo for Session<'_> {
    fn check(&self) -> Result<()> {
        Session::check(self)
    }
    fn write_command(&mut self, bytes: &[u8]) -> Result<usize> {
        ensure!(
            bytes.len() == 31 && bytes.starts_with(b"USBC") && bytes[12] == 0x80,
            "Refusing non-read BOT command"
        );
        Ok(self
            .handle
            .write_bulk(self.output, bytes, Duration::from_secs(5))?)
    }
    fn read_input(&mut self, bytes: &mut [u8]) -> std::result::Result<usize, usb_sat::ReadError> {
        self.handle
            .read_bulk(self.input, bytes, Duration::from_secs(5))
            .map_err(|e| match e {
                rusb::Error::Pipe => usb_sat::ReadError::Stall,
                e => usb_sat::ReadError::Failed(e.into()),
            })
    }
    fn clear_input_halt(&mut self) -> Result<()> {
        Ok(self.handle.clear_halt(self.input)?)
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub fn capture(id: &str, expected_registry_id: u64, cancelled: &AtomicBool) -> Result<RawCapture> {
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "USB capture requires administrator authorization; probe needs no administrator"
    );
    let version = rusb::version();
    ensure!(
        (version.major(), version.minor(), version.micro()) >= (1, 0, 30),
        "Build with scripts/build-libusb.sh; capture requires libusb 1.0.30+"
    );
    let before = plan(id)?;
    check_capture_plan(&before, expected_registry_id)?;
    let ctx = Context::new()?;
    let device = matching_device(&ctx, &before.usb)?;
    let (input, output) = bot_endpoints(&device)?;
    let handle = device.open()?;
    // Recheck mounts and the actual registry identity immediately before detaching anything.
    let fresh = plan(id)?;
    check_capture_plan(&fresh, expected_registry_id)?;
    ensure!(
        fresh.usb.location_id == before.usb.location_id
            && fresh.usb.device_address == before.usb.device_address,
        "USB device mapping changed"
    );
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "Cancelled before USB capture"
    );
    handle
        .detach_kernel_driver(0)
        .context("USB capture failed; no SAT kernel extension is installed by this helper")?;
    let mut session = Session {
        handle,
        captured: true,
        claimed: false,
        closed: false,
        input,
        output,
        tag: 0,
        started: Instant::now(),
        cancelled,
    };
    let result = (|| -> Result<RawCapture> {
        session.handle.claim_interface(0)?;
        session.claimed = true;
        session.handle.set_alternate_setting(0, 0)?;
        let mut notes = Vec::new();
        for mode in [SatMode::Sat16, SatMode::Sat12] {
            let Some(identity) = session.sector(ReadCommand::Identify, mode, &mut notes)? else {
                continue;
            };
            let model = usb_sat::identify_model(&identity)?;
            let Some(data) = session.sector(ReadCommand::SmartData, mode, &mut notes)? else {
                continue;
            };
            usb_sat::validate_attributes(&data)?;
            let thresholds = session.sector(ReadCommand::SmartThresholds, mode, &mut notes)?;
            if let Some(bytes) = &thresholds {
                usb_sat::checksum(bytes)?;
            }
            let _ = session.execute(Request::new(ReadCommand::SmartStatus, mode))?;
            let sense = session.sense(mode)?;
            if sense.is_none() {
                notes.push(
                    "SMART RETURN STATUS sense is unavailable; overall health remains Unknown."
                        .into(),
                );
            }
            let report = RawCapture {
                version: 1,
                device: id.into(),
                usb_registry_id: expected_registry_id,
                captured_unix: disks::unix_now(),
                mode,
                model,
                identity_redacted_hex: usb_sat::hex(&usb_sat::redacted_identity(&identity)?),
                smart_data_hex: usb_sat::hex(&data),
                thresholds_hex: thresholds.as_deref().map(usb_sat::hex),
                status_sense_hex: sense.as_deref().map(usb_sat::hex),
                driver_reattached: false,
                notes,
            };
            report.validate()?;
            return Ok(report);
        }
        bail!("Both SAT modes failed to read SMART: {}", notes.join("; "))
    })();
    let cleanup = session.finish();
    match (result, cleanup) {
        (Ok(mut report), Ok(())) => {
            report.driver_reattached = true;
            Ok(report)
        }
        (Ok(mut report), Err(e)) => {
            report.notes.push(format!(
                "USB driver restoration failed: {e:#}. Reconnect the drive."
            ));
            Ok(report)
        }
        (Err(e), Ok(())) => Err(e),
        (Err(e), Err(cleanup)) => Err(e.context(format!(
            "USB restoration also failed: {cleanup:#}; reconnect the drive"
        ))),
    }
}
