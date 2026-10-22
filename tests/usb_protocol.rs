//! USB protocol tests use fixtures and owned temporary files, never a USB device.
use anyhow::Result;
use ssd_scope_mac::{
    command::{self, Control},
    usb_report,
    usb_sat::{
        self, AtaHealth, BulkIo, RawCapture, ReadCommand, ReadError, Request, SatMode, Status,
    },
};
use std::{collections::VecDeque, fs, os::unix::fs::symlink, time::Duration};

fn csw(tag: u32, residue: u32, status: u8) -> Vec<u8> {
    let mut b = b"USBS".to_vec();
    b.extend(tag.to_le_bytes());
    b.extend(residue.to_le_bytes());
    b.push(status);
    b
}
fn fix_checksum(b: &mut [u8]) {
    b[511] = 0u8.wrapping_sub(b[..511].iter().fold(0u8, |s, b| s.wrapping_add(*b)));
}
fn identity() -> Vec<u8> {
    let mut b = vec![0; 512];
    b[0] = 0x40;
    b[164] = 1;
    b[170] = 1;
    b[167] = 0x44;
    b[173] = 0x44;
    b[175] = 0x40;
    b[160] = 0xf0;
    b[161] = 1;
    b[120..124].copy_from_slice(&500_000_000u32.to_le_bytes());
    b[200..208].copy_from_slice(&500_000_000u64.to_le_bytes());
    let mut model = [b' '; 40];
    model[..12].copy_from_slice(b"TS256GSSD370");
    for (i, w) in model.as_chunks::<2>().0.iter().enumerate() {
        b[54 + i * 2] = w[1];
        b[55 + i * 2] = w[0];
    }
    b[20..28].copy_from_slice(b"SERIAL12");
    b[216..224].copy_from_slice(b"WWN12345");
    b[510] = 0xa5;
    fix_checksum(&mut b);
    b
}
fn smart_data() -> Vec<u8> {
    let mut b = vec![0; 512];
    b[0] = 1;
    for (i, (id, raw)) in [(5u8, 0u64), (9, 1234), (12, 20), (194, 31)]
        .into_iter()
        .enumerate()
    {
        let p = 2 + i * 12;
        b[p] = id;
        b[p + 1] = 0x32;
        b[p + 3] = 100;
        b[p + 4] = 100;
        b[p + 5..p + 11].copy_from_slice(&raw.to_le_bytes()[..6]);
    }
    fix_checksum(&mut b);
    b
}
fn sense(failing: bool) -> Vec<u8> {
    let mut b = vec![0; 22];
    b[0] = 0x72;
    b[1] = 1;
    b[3] = 0x1d;
    b[7] = 14;
    b[8] = 9;
    b[9] = 12;
    b[21] = 0x50;
    (b[17], b[19]) = if failing { (0xf4, 0x2c) } else { (0x4f, 0xc2) };
    b
}
fn fixture() -> RawCapture {
    let id = identity();
    let mut th = vec![0; 512];
    th[0] = 1;
    th[2] = 5;
    th[3] = 5;
    fix_checksum(&mut th);
    RawCapture {
        version: 1,
        device: "disk6".into(),
        usb_registry_id: 42,
        captured_unix: 1,
        mode: SatMode::Sat16,
        model: usb_sat::identify_model(&id).unwrap(),
        identity_redacted_hex: usb_sat::hex(&usb_sat::redacted_identity(&id).unwrap()),
        smart_data_hex: usb_sat::hex(&smart_data()),
        thresholds_hex: Some(usb_sat::hex(&th)),
        status_sense_hex: Some(usb_sat::hex(&sense(false))),
        driver_reattached: true,
        notes: Vec::new(),
    }
}

fn cache_fixture() -> (
    RawCapture,
    ssd_scope_mac::disks::Drive,
    ssd_scope_mac::native::UsbBridge,
) {
    use ssd_scope_mac::{
        disks::{Drive, Health, Volume},
        native::UsbBridge,
    };
    let mut capture = fixture();
    let mut id = usb_sat::unhex(&capture.identity_redacted_hex, 512).unwrap();
    id[98..100].copy_from_slice(&0x0200u16.to_le_bytes());
    fix_checksum(&mut id);
    capture.identity_redacted_hex = usb_sat::hex(&id);
    let drive = Drive {
        id: "disk6".into(),
        name: "Fixture SSD".into(),
        capacity: 256_000_000_000,
        internal: false,
        ssd: Some(true),
        bus: "USB".into(),
        health: Health::parse(&plist::Dictionary::new()),
        volumes: vec![Volume {
            id: "disk7s1".into(),
            uuid: Some("aaaaaaaa-1111-2222-3333-444444444444".into()),
            name: "Fixture".into(),
            mount: "/fixture".into(),
            filesystem: "APFS".into(),
            capacity: 256_000_000_000,
            free: None,
            writable: true,
        }],
        captured_unix: 1,
    };
    let usb = UsbBridge {
        vendor_id: 0x0bda,
        product_id: 0x9201,
        ..Default::default()
    };
    (capture, drive, usb)
}

#[test]
fn ata_capacity_validates_lba_sector_size_and_overflow() -> Result<()> {
    let (capture, _, _) = cache_fixture();
    let mut id = usb_sat::unhex(&capture.identity_redacted_hex, 512)?;
    assert_eq!(usb_sat::identify_capacity(&id)?, 256_000_000_000);
    id[212..214].copy_from_slice(&0x5000u16.to_le_bytes());
    id[234..238].copy_from_slice(&2048u32.to_le_bytes());
    fix_checksum(&mut id);
    assert_eq!(usb_sat::identify_capacity(&id)?, 2_048_000_000_000);
    id[234..238].copy_from_slice(&257u32.to_le_bytes());
    fix_checksum(&mut id);
    assert!(usb_sat::identify_capacity(&id).is_err());
    id[212..214].copy_from_slice(&0x4000u16.to_le_bytes());
    id[200..208].copy_from_slice(&u64::MAX.to_le_bytes());
    fix_checksum(&mut id);
    assert!(usb_sat::identify_capacity(&id).is_err());
    id[166..168].fill(0);
    id[120..124].copy_from_slice(&0x0fff_ffffu32.to_le_bytes());
    fix_checksum(&mut id);
    assert_eq!(usb_sat::identify_capacity(&id)?, 137_438_952_960);
    id[98..100].fill(0);
    fix_checksum(&mut id);
    assert!(usb_sat::identify_capacity(&id).is_err());
    Ok(())
}

#[test]
fn snapshot_binding_follows_uuid_but_refuses_other_disks_or_bridges() -> Result<()> {
    use ssd_scope_mac::usb_cache::SavedCapture;
    let (capture, mut drive, mut usb) = cache_fixture();
    let saved = SavedCapture::bind(capture.clone(), &drive, &usb)?;
    drive.id = "disk42".into();
    assert!(saved.matches(&drive, &usb));
    drive.volumes[0].uuid = Some("AAAAAAAA-1111-2222-3333-444444444444".into());
    assert!(saved.matches(&drive, &usb));
    drive.capacity += 512;
    assert!(!saved.matches(&drive, &usb));
    assert!(SavedCapture::bind(capture.clone(), &drive, &usb).is_err());
    drive.capacity -= 512;
    usb.product_id = 0x9999;
    assert!(!saved.matches(&drive, &usb));
    assert!(SavedCapture::bind(capture.clone(), &drive, &usb).is_err());
    usb.product_id = 0x9201;
    drive.internal = true;
    assert!(SavedCapture::bind(capture.clone(), &drive, &usb).is_err());
    drive.internal = false;
    drive.volumes[0].uuid = Some("bbbbbbbb-1111-2222-3333-444444444444".into());
    assert!(!saved.matches(&drive, &usb));
    drive.volumes.clear();
    assert!(SavedCapture::bind(capture, &drive, &usb).is_err());
    Ok(())
}

#[test]
fn snapshot_cache_is_private_atomic_bounded_and_removable() -> Result<()> {
    use ssd_scope_mac::usb_cache::{SavedCapture, Store};
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("private/cache.json");
    let (capture, drive, usb) = cache_fixture();
    let mut store = Store::default();
    store.upsert(SavedCapture::bind(capture, &drive, &usb)?)?;
    store.save(&path)?;
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap())?.permissions().mode() & 0o777,
        0o700
    );
    let mut loaded = Store::load(&path)?;
    assert_eq!(loaded.captures.len(), 1);
    loaded.upsert(loaded.captures[0].clone())?;
    assert_eq!(loaded.captures.len(), 1);
    loaded.remove(&drive);
    loaded.save(&path)?;
    assert!(Store::load(&path)?.captures.is_empty());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1])?;
    assert!(Store::load(&path).is_err());
    Ok(())
}

#[test]
fn snapshot_cache_refuses_symlinks_fifo_public_files_and_unsafe_parent() -> Result<()> {
    use ssd_scope_mac::usb_cache::Store;
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let file = temp.path().join("cache.json");
    Store::default().save(&file)?;
    let alias = temp.path().join("alias.json");
    symlink(&file, &alias)?;
    assert!(Store::load(&alias).is_err());
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644))?;
    assert!(Store::load(&file).is_err());
    let fifo = temp.path().join("fifo");
    let name = std::ffi::CString::new(fifo.to_str().unwrap())?;
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(Store::load(&fifo).is_err());
    let directory = temp.path().join("public");
    fs::create_dir(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))?;
    assert!(
        Store::default()
            .save(&directory.join("cache.json"))
            .is_err()
    );
    let linked = temp.path().join("linked");
    symlink(&directory, &linked)?;
    assert!(Store::default().save(&linked.join("cache.json")).is_err());
    Ok(())
}

#[test]
fn snapshot_cache_rejects_future_captures_duplicate_bindings_and_versions() -> Result<()> {
    use ssd_scope_mac::usb_cache::{SavedCapture, Store};
    let temp = tempfile::tempdir()?;
    let file = temp.path().join("cache.json");
    let (mut capture, drive, usb) = cache_fixture();
    capture.captured_unix = ssd_scope_mac::disks::unix_now() + 3600;
    assert!(SavedCapture::bind(capture, &drive, &usb).is_err());
    let (capture, drive, usb) = cache_fixture();
    let saved = SavedCapture::bind(capture, &drive, &usb)?;
    let mut store = Store {
        version: 1,
        captures: vec![saved.clone(), saved],
    };
    assert!(store.save(&file).is_err());
    store.captures.clear();
    store.version = 99;
    assert!(store.save(&file).is_err());
    Ok(())
}

#[test]
fn captured_attribute_raw_zero_does_not_become_zero_or_full_health_percent() -> Result<()> {
    let mut capture = fixture();
    let mut bytes = usb_sat::unhex(&capture.smart_data_hex, 512)?;
    let p = 2 + 4 * 12;
    bytes[p] = 169;
    bytes[p + 1] = 0x32;
    bytes[p + 3] = 100;
    bytes[p + 4] = 100;
    fix_checksum(&mut bytes);
    capture.smart_data_hex = usb_sat::hex(&bytes);
    capture.status_sense_hex = None;
    let snapshot = usb_report::decode(&capture, &Control::default())?;
    let attribute = snapshot
        .ata_attributes
        .iter()
        .find(|a| a.id == 169)
        .unwrap();
    assert_eq!(attribute.normalized, Some(100));
    assert_eq!(attribute.raw, "0");
    assert_eq!(snapshot.health.status, "Unknown");
    assert!(snapshot.health.percentage_used.is_none());
    assert_eq!(snapshot.threshold_counts(), (1, 0));
    assert_eq!(snapshot.age(61), "1m ago");
    assert_eq!(snapshot.captured_at(), "1970-01-01 00:00 UTC");
    assert!(snapshot.label().contains("not live"));
    Ok(())
}

#[test]
fn erase_count_wear_follows_the_windows_ssd_scope_ratio() {
    let raw = |pairs: &[(&str, f64)]| {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let worn = raw(&[
        ("Average_Erase_Count", 2366.0),
        ("Max_Erase_Count_of_Spec", 1500.0),
    ]);
    let used = usb_report::erase_count_wear(&worn).unwrap();
    assert!((used - 157.73).abs() < 0.01);
    let health = ssd_scope_mac::disks::Health {
        percentage_used: Some(used),
        ..Default::default()
    };
    assert_eq!(health.remaining_life_estimate(), Some(0.0));
    let fresh = raw(&[
        ("Average_Erase_Count", 150.0),
        ("Max_Erase_Count_of_Spec", 3000.0),
    ]);
    assert_eq!(usb_report::erase_count_wear(&fresh), Some(5.0));
    for missing in [
        raw(&[("Average_Erase_Count", 10.0)]),
        raw(&[("Max_Erase_Count_of_Spec", 1500.0)]),
        raw(&[
            ("Average_Erase_Count", 10.0),
            ("Max_Erase_Count_of_Spec", 0.0),
        ]),
        raw(&[("Remaining_Lifetime_Perc", 0.0)]),
    ] {
        assert_eq!(usb_report::erase_count_wear(&missing), None);
    }
}

#[test]
fn commands_are_data_in_and_cannot_encode_disk_writes() {
    for mode in [SatMode::Sat12, SatMode::Sat16] {
        for command in [
            ReadCommand::Identify,
            ReadCommand::SmartData,
            ReadCommand::SmartThresholds,
            ReadCommand::SmartStatus,
            ReadCommand::RequestSense,
        ] {
            let r = Request::new(command, mode);
            let b = r.cbw(0x12345678);
            assert_eq!(&b[..4], b"USBC");
            assert_eq!(&b[4..8], &[0x78, 0x56, 0x34, 0x12]);
            assert_eq!(b[12], 0x80);
            assert_eq!(b[13], 0);
            let c = r.cdb();
            assert!(matches!(c[0], 0x85 | 0xa1 | 0x03));
            if c[0] != 3 {
                let (op, feature) = if c[0] == 0x85 {
                    (c[14], c[4])
                } else {
                    (c[9], c[3])
                };
                assert!(
                    op == 0xec && feature == 0
                        || op == 0xb0 && matches!(feature, 0xd0 | 0xd1 | 0xda)
                );
            }
        }
    }
}
#[test]
fn sat_register_positions_and_lengths_follow_the_read_protocol() {
    let a = Request::new(ReadCommand::SmartData, SatMode::Sat16);
    assert_eq!(
        a.cdb(),
        &[
            0x85, 8, 0x0e, 0, 0xd0, 0, 1, 0, 0, 0, 0x4f, 0, 0xc2, 0, 0xb0, 0
        ]
    );
    assert_eq!(a.expected_len(), 512);
    let b = Request::new(ReadCommand::SmartData, SatMode::Sat12);
    assert_eq!(
        b.cdb(),
        &[0xa1, 8, 0x0e, 0xd0, 1, 0, 0x4f, 0xc2, 0, 0xb0, 0, 0]
    );
    let c = Request::new(ReadCommand::SmartStatus, SatMode::Sat16);
    assert_eq!(c.expected_len(), 0);
    assert_eq!(c.cdb()[2], 0x20);
}
#[test]
fn status_rejects_wrong_tag_signature_residue_and_phase_error() {
    assert_eq!(
        usb_sat::parse_csw(&csw(8, 0, 0), 8, 512, 512).unwrap(),
        Status::Passed
    );
    assert_eq!(
        usb_sat::parse_csw(&csw(8, 512, 1), 8, 512, 0).unwrap(),
        Status::CommandFailed
    );
    for b in [
        csw(9, 0, 0),
        csw(8, 513, 0),
        csw(8, 0, 2),
        csw(8, 0, 255),
        vec![0; 13],
        vec![0; 12],
    ] {
        assert!(usb_sat::parse_csw(&b, 8, 512, 512).is_err());
    }
    assert!(usb_sat::parse_csw(&csw(8, 0, 0), 8, 512, 511).is_err());
}
#[test]
fn health_needs_a_complete_successful_ata_return_descriptor() {
    assert_eq!(
        usb_sat::status_from_sense(&sense(false)),
        Some(AtaHealth::Passed)
    );
    assert_eq!(
        usb_sat::status_from_sense(&sense(true)),
        Some(AtaHealth::Failing)
    );
    for i in 0..22 {
        assert_eq!(usb_sat::status_from_sense(&sense(false)[..i]), None);
    }
    for status in [0, 1, 8, 0x20, 0x80, 0x51] {
        let mut s = sense(false);
        s[21] = status;
        assert_eq!(usb_sat::status_from_sense(&s), None);
    }
    let mut s = sense(false);
    s[1] = 5;
    assert_eq!(usb_sat::status_from_sense(&s), None);
    s[1] = 1;
    s[17] = 0;
    assert_eq!(usb_sat::status_from_sense(&s), None);
    let mut deferred = sense(false);
    deferred[0] = 0x73;
    assert_eq!(usb_sat::status_from_sense(&deferred), None);
}

#[test]
fn apfs_mount_guard_finds_synthesized_volumes_and_snapshots_without_confusing_siblings()
-> Result<()> {
    use ssd_scope_mac::usb_safety::{self, Mount};
    fn p(value: serde_json::Value) -> Result<plist::Dictionary> {
        let mut xml = Vec::new();
        plist::to_writer_xml(&mut xml, &value)?;
        Ok(plist::Value::from_reader(std::io::Cursor::new(xml))?
            .into_dictionary()
            .unwrap())
    }
    let physical = p(
        serde_json::json!({"AllDisksAndPartitions":[{"DeviceIdentifier":"disk6","Partitions":[{"DeviceIdentifier":"disk6s3","Content":"Apple_APFS"}]}]}),
    )?;
    let apfs = p(
        serde_json::json!({"Containers":[{"PhysicalStores":[{"DeviceIdentifier":"disk6s3"}],"ContainerReference":"disk7","Volumes":[{"DeviceIdentifier":"disk7s1"}]}]}),
    )?;
    let roots = usb_safety::device_roots(&physical, &apfs, "disk6")?;
    let mounts = vec![
        Mount {
            device: "/dev/disk7s1s1".into(),
            path: "/tmp/test-snapshot".into(),
            filesystem: "apfs".into(),
        },
        Mount {
            device: "/dev/disk60s1".into(),
            path: "/unrelated".into(),
            filesystem: "exfat".into(),
        },
    ];
    assert_eq!(
        usb_safety::selected_mounts(&roots, &mounts)?,
        vec!["/tmp/test-snapshot"]
    );
    let empty = p(serde_json::json!({"Containers":[]}))?;
    assert!(usb_safety::device_roots(&physical, &empty, "disk6").is_err());
    let malformed = p(serde_json::json!({"Containers":[{"ContainerReference":"disk7"}]}))?;
    assert!(usb_safety::device_roots(&physical, &malformed, "disk6").is_err());
    assert!(!usb_safety::descendant("disk60s1", "disk6"));
    assert!(usb_safety::descendant("disk6s3", "disk6"));
    assert!(
        usb_safety::selected_mounts(
            &roots,
            &[Mount {
                device: "unknown".into(),
                path: "/".into(),
                filesystem: "apfs".into()
            }]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn report_loader_refuses_fifo_without_waiting_for_a_writer() -> Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("fifo");
    let c = CString::new(path.as_os_str().as_bytes())?;
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    assert!(usb_report::load(&path).is_err());
    Ok(())
}
#[test]
fn corrupted_empty_and_duplicate_sectors_are_not_health_data() {
    assert!(usb_sat::validate_attributes(&smart_data()).is_ok());
    let mut data = smart_data();
    data[14] = 5;
    fix_checksum(&mut data);
    assert!(usb_sat::validate_attributes(&data).is_err());
    let mut data = smart_data();
    data[10] ^= 1;
    assert!(usb_sat::validate_attributes(&data).is_err());
    assert!(usb_sat::validate_attributes(&vec![0; 512]).is_err());
    assert!(usb_sat::checksum(&[0; 511]).is_err());
}
#[test]
fn identity_redaction_preserves_model_and_validates_original_checksum() {
    let id = identity();
    let red = usb_sat::redacted_identity(&id).unwrap();
    assert_eq!(usb_sat::identify_model(&red).unwrap(), "TS256GSSD370");
    assert!(
        red[20..40]
            .iter()
            .chain(red[216..224].iter())
            .all(|b| *b == 0)
    );
    assert!(usb_sat::checksum(&red).is_ok());
    let mut bad = id;
    bad[20] ^= 1;
    assert!(usb_sat::redacted_identity(&bad).is_err());
}
#[test]
fn capture_schema_cannot_claim_health_without_registers() {
    let mut r = fixture();
    assert_eq!(r.health().unwrap(), Some(AtaHealth::Passed));
    r.status_sense_hex = None;
    assert_eq!(r.health().unwrap(), None);
    let mut j = serde_json::to_value(&r).unwrap();
    j["passed"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<RawCapture>(j).is_err());
    r.model = "Another SSD".into();
    assert!(r.validate().is_err());
    r = fixture();
    r.version = 2;
    assert!(r.validate().is_err());
}
#[test]
fn strict_hex_and_physical_ids_reject_ambiguous_input() {
    for s in ["0", "zz", " 00", "é0"] {
        assert!(usb_sat::unhex(s, 512).is_err());
    }
    assert!(usb_sat::unhex(&"00".repeat(513), 512).is_err());
    for s in [
        "disk6s1",
        "/dev/disk6",
        "disk6;id",
        "disk",
        "disk-1",
        "disk1\0",
    ] {
        assert!(!usb_sat::physical_id(s));
    }
    assert!(usb_sat::physical_id("disk6"));
}

enum Read {
    Bytes(Vec<u8>),
    Stall,
    Fail,
}
struct Fake {
    reads: VecDeque<Read>,
    commands: Vec<Vec<u8>>,
    clears: usize,
    cancelled: bool,
    short_write: bool,
}
impl Fake {
    fn new(reads: Vec<Read>) -> Self {
        Self {
            reads: reads.into(),
            commands: Vec::new(),
            clears: 0,
            cancelled: false,
            short_write: false,
        }
    }
}
impl BulkIo for Fake {
    fn check(&self) -> Result<()> {
        anyhow::ensure!(!self.cancelled, "cancelled");
        Ok(())
    }
    fn write_command(&mut self, b: &[u8]) -> Result<usize> {
        self.commands.push(b.to_vec());
        Ok(if self.short_write { 1 } else { b.len() })
    }
    fn read_input(&mut self, b: &mut [u8]) -> std::result::Result<usize, ReadError> {
        match self.reads.pop_front().expect("unexpected extra USB read") {
            Read::Bytes(v) => {
                assert!(v.len() <= b.len());
                b[..v.len()].copy_from_slice(&v);
                Ok(v.len())
            }
            Read::Stall => Err(ReadError::Stall),
            Read::Fail => Err(ReadError::Failed(anyhow::anyhow!("timeout"))),
        }
    }
    fn clear_input_halt(&mut self) -> Result<()> {
        self.clears += 1;
        Ok(())
    }
}
#[test]
fn bot_reads_a_sector_then_validates_status() {
    let mut io = Fake::new(vec![Read::Bytes(smart_data()), Read::Bytes(csw(1, 0, 0))]);
    let (status, bytes) = usb_sat::transfer(
        &mut io,
        Request::new(ReadCommand::SmartData, SatMode::Sat16),
        1,
    )
    .unwrap();
    assert_eq!(status, Status::Passed);
    assert_eq!(bytes, smart_data());
    assert_eq!(io.commands.len(), 1);
    assert!(io.reads.is_empty());
}
#[test]
fn bot_failed_command_can_return_an_early_status_without_an_extra_read() {
    let mut io = Fake::new(vec![Read::Bytes(csw(1, 512, 1))]);
    let (status, bytes) = usb_sat::transfer(
        &mut io,
        Request::new(ReadCommand::SmartData, SatMode::Sat16),
        1,
    )
    .unwrap();
    assert_eq!(status, Status::CommandFailed);
    assert!(bytes.is_empty());
    assert!(io.reads.is_empty());
}
#[test]
fn bot_handles_one_data_stall_and_one_status_stall() {
    let mut io = Fake::new(vec![Read::Stall, Read::Stall, Read::Bytes(csw(1, 512, 1))]);
    assert!(
        usb_sat::transfer(
            &mut io,
            Request::new(ReadCommand::SmartData, SatMode::Sat16),
            1
        )
        .is_ok()
    );
    assert_eq!(io.clears, 2);
    let mut io = Fake::new(vec![Read::Stall, Read::Stall, Read::Stall]);
    assert!(
        usb_sat::transfer(
            &mut io,
            Request::new(ReadCommand::SmartData, SatMode::Sat16),
            1
        )
        .is_err()
    );
    assert_eq!(io.clears, 2);
}
#[test]
fn bot_timeout_short_write_and_cancel_do_not_retry_commands() {
    let mut io = Fake::new(vec![Read::Fail]);
    assert!(
        usb_sat::transfer(
            &mut io,
            Request::new(ReadCommand::Identify, SatMode::Sat16),
            1
        )
        .is_err()
    );
    assert_eq!(io.commands.len(), 1);
    let mut io = Fake::new(vec![]);
    io.short_write = true;
    assert!(
        usb_sat::transfer(
            &mut io,
            Request::new(ReadCommand::Identify, SatMode::Sat16),
            1
        )
        .is_err()
    );
    assert_eq!(io.commands.len(), 1);
    let mut io = Fake::new(vec![]);
    io.cancelled = true;
    assert!(
        usb_sat::transfer(
            &mut io,
            Request::new(ReadCommand::Identify, SatMode::Sat16),
            1
        )
        .is_err()
    );
    assert!(io.commands.is_empty());
}
#[test]
fn capture_files_refuse_symlinks_oversized_and_invalid_content() -> Result<()> {
    let t = tempfile::tempdir()?;
    let file = t.path().join("capture.json");
    let link = t.path().join("link");
    fs::write(&file, serde_json::to_vec(&fixture())?)?;
    assert!(usb_report::load(&file).is_ok());
    symlink(&file, &link)?;
    assert!(usb_report::load(&link).is_err());
    fs::write(&file, vec![b' '; 64 * 1024 + 1])?;
    assert!(usb_report::load(&file).is_err());
    fs::write(&file, b"{\"version\":99}")?;
    assert!(usb_report::load(&file).is_err());
    Ok(())
}
#[test]
fn private_file_backed_stdin_is_bounded_and_exact() -> Result<()> {
    let bytes: Vec<u8> = (0..32768).map(|i| (i % 256) as u8).collect();
    let output = command::run_input(
        "/bin/cat",
        &[],
        Duration::from_secs(5),
        &Control::default(),
        &bytes,
    )?
    .success()?;
    assert_eq!(output.stdout, bytes);
    assert!(
        command::run_input(
            "/DO_NOT_EXECUTE",
            &[],
            Duration::from_secs(1),
            &Control::default(),
            &vec![0; 32769]
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn real_offline_decoder_reads_fixture_temperature_and_preserves_unknown_health() -> Result<()> {
    let r = fixture();
    let snapshot = usb_report::decode(&r, &Control::default())?;
    assert_eq!(snapshot.health.temperature_c, Some(31.0));
    assert_eq!(snapshot.health.status, "Verified");
    assert!(snapshot.health.percentage_used.is_none());
    let mut unknown = r.clone();
    unknown.status_sense_hex = None;
    let snapshot = usb_report::decode(&unknown, &Control::default())?;
    assert_eq!(snapshot.health.status, "Unknown");
    let mut failing = r;
    failing.status_sense_hex = Some(usb_sat::hex(&sense(true)));
    let snapshot = usb_report::decode(&failing, &Control::default())?;
    assert_eq!(snapshot.health.status, "Failing");
    Ok(())
}

#[cfg(feature = "experimental-usb")]
#[test]
fn capture_guards_refuse_mounts_identity_changes_and_incomplete_discovery() {
    use ssd_scope_mac::{
        native::UsbBridge,
        usb_access::{Plan, check_capture_plan},
    };
    let mut p = Plan {
        device: "disk6".into(),
        model: "test".into(),
        capacity: 256_000_000_000,
        usb: UsbBridge {
            registry_id: 42,
            location_id: 0x14100000,
            device_address: 3,
            vendor_id: 0x0bda,
            product_id: 0x9201,
            ..Default::default()
        },
        mounted_paths: vec![],
        warnings: vec![],
        libusb_version: "1.0.30".into(),
        bot_available: true,
    };
    assert!(check_capture_plan(&p, 42).is_ok());
    assert!(check_capture_plan(&p, 43).is_err());
    assert!(check_capture_plan(&p, 0).is_err());
    p.mounted_paths.push("/test/mounted".into());
    assert!(check_capture_plan(&p, 42).is_err());
    p.mounted_paths.clear();
    p.warnings.push("missing APFS mapping".into());
    assert!(check_capture_plan(&p, 42).is_err());
    p.warnings.clear();
    p.usb.device_address = 0;
    assert!(check_capture_plan(&p, 42).is_err());
    p.usb.device_address = 3;
    p.usb.vendor_id = 0;
    assert!(check_capture_plan(&p, 42).is_err());
    p.usb.vendor_id = 0x0bda;
    p.bot_available = false;
    assert!(check_capture_plan(&p, 42).is_err());
}
