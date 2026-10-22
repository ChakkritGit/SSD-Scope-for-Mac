use plist::{Dictionary, Value};
use ssd_scope_mac::{
    command::{self, Control},
    disks::{self, Health},
    operations,
};
use std::{
    fs,
    os::unix::fs::symlink,
    time::{Duration, Instant},
};

#[test]
fn unknown_smart_does_not_become_healthy() {
    let health = Health::parse(&Dictionary::new());
    assert_eq!(health.status, "Unknown");
    assert_eq!(health.remaining_life_estimate(), None);
    assert_eq!(health.temperature_c, None);
    assert_eq!(health.media_errors, None);
}

#[test]
fn nvme_units_and_wear_are_interpreted_without_losing_unknowns() {
    let mut raw = Dictionary::new();
    raw.insert("TEMPERATURE".into(), Value::Integer(310.into()));
    raw.insert("PERCENTAGE_USED".into(), Value::Integer(1.into()));
    raw.insert("DATA_UNITS_WRITTEN_0".into(), Value::Integer(10.into()));
    raw.insert("DATA_UNITS_WRITTEN_1".into(), Value::Integer(0.into()));
    let mut info = Dictionary::new();
    info.insert("SMARTStatus".into(), Value::String("Verified".into()));
    info.insert(
        "SMARTDeviceSpecificKeysMayVaryNotGuaranteed".into(),
        Value::Dictionary(raw),
    );
    let health = Health::parse(&info);
    assert!((health.temperature_c.unwrap() - 36.85).abs() < 0.001);
    assert_eq!(health.remaining_life_estimate(), Some(99.0));
    assert_eq!(health.bytes_written, Some(5_120_000.0));
    assert_eq!(health.power_on_hours, None);
}

#[test]
fn device_ids_reject_paths_flags_and_shell_text() {
    for id in ["disk0", "disk12s3", "disk3s1s1"] {
        assert!(disks::valid_id(id));
    }
    for id in [
        "disk",
        "disk1s",
        "/dev/disk1",
        "-all",
        "disk1;touch x",
        "disk1s-1",
    ] {
        assert!(!disks::valid_id(id));
    }
}

#[test]
fn nested_destinations_and_symlink_aliases_are_rejected() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    let nested = source.join("nested");
    fs::create_dir_all(&nested)?;
    let alias = root.path().join("alias");
    symlink(&nested, &alias)?;
    assert!(operations::validate_backup(&source, &nested).is_err());
    assert!(operations::validate_backup(&source, &source).is_err());
    assert!(operations::validate_backup(&source, &alias).is_err());
    Ok(())
}

#[test]
fn real_backup_preserves_data_symlinks_and_detects_same_size_corruption() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("ข้อมูล source '$; test");
    let parent = root.path().join("destination");
    fs::create_dir(&source)?;
    fs::create_dir(&parent)?;
    fs::create_dir(source.join("empty"))?;
    fs::write(source.join("data.bin"), [0, 1, 2, 3, 255])?;
    fs::write(source.join(".hidden"), "hidden content")?;
    symlink("data.bin", source.join("link"))?;
    let control = Control::default();
    let result = operations::backup(&source, &parent, true, &control)?;
    assert!(!result.to_string_lossy().ends_with(".partial"));
    let payload = operations::restore_source(&result)?;
    assert_eq!(operations::verify_copy(&source, &payload, &control)?, 2);
    assert_eq!(
        fs::read_link(payload.join("link"))?,
        std::path::PathBuf::from("data.bin")
    );
    assert!(payload.join("empty").is_dir());
    fs::write(payload.join("data.bin"), [9, 9, 9, 9, 9])?;
    assert!(operations::verify_copy(&source, &payload, &control).is_err());
    assert_eq!(fs::read(source.join("data.bin"))?, [0, 1, 2, 3, 255]);
    Ok(())
}

#[test]
fn benchmark_verifies_data_and_cleans_its_temporary_file() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("keep.txt"), "untouched")?;
    let result = operations::benchmark(root.path(), 4 * 1024 * 1024, &Control::default())?;
    assert!(result.write_mbps > 0.0 && result.read_mbps > 0.0);
    assert_eq!(fs::read_dir(root.path())?.count(), 1);
    assert_eq!(
        fs::read_to_string(root.path().join("keep.txt"))?,
        "untouched"
    );
    let cancelled = Control::default();
    cancelled.cancel();
    assert!(operations::benchmark(root.path(), 4 * 1024 * 1024, &cancelled).is_err());
    assert_eq!(fs::read_dir(root.path())?.count(), 1);
    Ok(())
}

#[test]
fn command_cancellation_reaps_the_child() {
    let control = Control::default();
    let worker = control.clone();
    let start = Instant::now();
    let handle = std::thread::spawn(move || {
        command::run("/bin/sleep", &["10"], Duration::from_secs(20), &worker)
    });
    std::thread::sleep(Duration::from_millis(200));
    control.cancel();
    assert!(handle.join().unwrap().is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn nonzero_command_is_reported_as_failure() -> anyhow::Result<()> {
    let result = command::quick("/usr/bin/false", &[])?;
    assert!(result.success().is_err());
    Ok(())
}
