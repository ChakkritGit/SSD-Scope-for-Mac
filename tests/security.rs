//! Security regressions use only owned temporary directories and read-only parsers.
use anyhow::Result;
use serde_json::json;
use ssd_scope_mac::{
    command::{self, Control},
    disks::Health,
    native, operations, smart,
    telemetry::History,
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
    time::Duration,
};

#[test]
fn shell_metacharacters_are_literal_arguments() -> Result<()> {
    let root = tempfile::tempdir()?;
    let marker = root.path().join("SHOULD_NOT_EXIST");
    let payload = format!(
        "$(touch '{}'); `touch '{}'`",
        marker.display(),
        marker.display()
    );
    let result = command::quick("/usr/bin/printf", &["%s", &payload])?.success()?;
    assert_eq!(result.text(), payload);
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn device_paths_and_nul_are_rejected_before_native_access() {
    for id in [
        "../disk6",
        "/dev/disk6",
        "disk6\0ignored",
        "disk6;echo hello",
        "--all",
    ] {
        assert!(native::inspect(id).is_err());
        assert!(smart::diagnose(id, &Control::default()).is_err());
    }
}

#[test]
fn benchmark_rejects_devices_invalid_sizes_and_preserves_neighbors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = root.path().join("keep");
    fs::write(&original, b"do not modify")?;
    for size in [0, 1, 1024 * 1024 - 1, 1024 * 1024 * 1024 + 1, u64::MAX] {
        assert!(operations::benchmark(root.path(), size, &Control::default()).is_err());
    }
    assert!(
        operations::benchmark(Path::new("/dev/null"), 1024 * 1024, &Control::default()).is_err()
    );
    assert_eq!(fs::read(original)?, b"do not modify");
    assert_eq!(fs::read_dir(root.path())?.count(), 1);
    Ok(())
}

#[test]
fn source_symlinks_to_system_and_nested_destination_aliases_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let alias = root.path().join("system-alias");
    symlink("/System", &alias)?;
    assert!(operations::validate_backup(&alias, root.path()).is_err());
    let source = root.path().join("data");
    let child = source.join("child");
    fs::create_dir_all(&child)?;
    let dest = root.path().join("destination-alias");
    symlink(&child, &dest)?;
    assert!(operations::validate_backup(&source, &dest).is_err());
    Ok(())
}

#[test]
fn similar_sibling_names_are_not_treated_as_nested_paths() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("data");
    let destination = root.path().join("data-copy");
    fs::create_dir(&source)?;
    fs::create_dir(&destination)?;
    assert!(operations::validate_backup(&source, &destination).is_ok());
    Ok(())
}

#[test]
fn symlink_in_copy_cannot_substitute_for_a_regular_file() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    let copy = root.path().join("copy");
    fs::create_dir(&source)?;
    fs::create_dir(&copy)?;
    fs::write(source.join("file"), b"content")?;
    let outside = root.path().join("outside");
    fs::write(&outside, b"content")?;
    symlink(&outside, copy.join("file"))?;
    assert!(operations::verify_copy(&source, &copy, &Control::default()).is_err());
    assert_eq!(fs::read(outside)?, b"content");
    Ok(())
}

#[test]
fn hash_refuses_symlink_targets() -> Result<()> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("data");
    let alias = root.path().join("alias");
    fs::write(&file, b"unchanged")?;
    symlink(&file, &alias)?;
    assert!(operations::sha256(&alias, &Control::default()).is_err());
    Ok(())
}

#[test]
fn completed_backup_is_private_and_never_overwrites_an_existing_backup() -> Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source)?;
    fs::create_dir(&destination)?;
    fs::write(source.join("data"), b"first")?;
    let one = operations::backup(&source, &destination, true, &Control::default())?;
    fs::write(source.join("data"), b"second")?;
    let two = operations::backup(&source, &destination, true, &Control::default())?;
    assert_ne!(one, two);
    assert_eq!(fs::read(one.join("Files/data"))?, b"first");
    assert_eq!(fs::metadata(one)?.permissions().mode() & 0o077, 0);
    Ok(())
}

#[test]
fn incomplete_malformed_and_symlink_payload_backups_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let partial = root.path().join("backup.partial");
    fs::create_dir(&partial)?;
    assert!(operations::restore_source(&partial).is_err());
    let broken = root.path().join("broken");
    fs::create_dir(&broken)?;
    fs::write(broken.join("scope-backup.json"), b"not-json")?;
    assert!(operations::restore_source(&broken).is_err());
    let fake = root.path().join("fake");
    fs::create_dir(&fake)?;
    let manifest = operations::Manifest {
        format_version: 1,
        source: Path::new("/unused").into(),
        completed_unix: 0,
        regular_files_verified: None,
        kind: "test".into(),
    };
    fs::write(
        fake.join("scope-backup.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    symlink(root.path(), fake.join("Files"))?;
    assert!(operations::restore_source(&fake).is_err());
    Ok(())
}

#[test]
fn failed_smart_commands_do_not_turn_reported_pass_into_verified_health() {
    let fake = json!({"smart_status":{"passed":true},"temperature":{"current":30}});
    for code in [1, 2, 4, -1] {
        assert!(smart::parse_health(&fake, code).is_none());
    }
    assert_eq!(smart::parse_health(&fake, 8).unwrap().status, "Verified");
    assert_eq!(
        smart::parse_health(&json!({"smart_status":{"passed":false}}), 8)
            .unwrap()
            .status,
        "Failing"
    );
    assert!(smart::parse_health(&json!({"temperature":{"current":30}}), 0).is_none());
}

#[test]
fn ata_vendor_attributes_do_not_invent_an_endurance_percentage() {
    let report = json!({"smart_status":{"passed":true},"ata_smart_attributes":{"table":[{"name":"Unknown_Attribute","raw":{"value":100}}]}});
    let health = smart::parse_health(&report, 0).unwrap();
    assert_eq!(health.percentage_used, None);
    let value = json!({"nvme_smart_health_information_log":{"percentage_used":-1}});
    assert_eq!(
        smart::parse_health(&value, 0).unwrap().percentage_used,
        None
    );
    assert_eq!(
        Health::parse(&plist::Dictionary::new()).remaining_life_estimate(),
        None
    );
}

#[test]
fn telemetry_handles_disconnect_counter_reset_and_bounded_memory() {
    let mut history = History::default();
    let counter = |id, read, write| native::Counters {
        registry_id: id,
        read_bytes: read,
        write_bytes: write,
    };
    history.push(counter(1, 0, 0), 0.0);
    history.push(counter(1, 2_000_000, 1_000_000), 1.0);
    assert_eq!(history.points.back().unwrap().read_mbps, 2.0);
    history.push(counter(2, 10, 10), 2.0);
    history.push(counter(2, 0, 0), 3.0);
    assert_eq!(history.points.len(), 1);
    for i in 4..200 {
        history.push(counter(2, i * 1_000_000, i * 1_000_000), i as f64);
    }
    assert_eq!(history.points.len(), 120);
}

#[test]
fn timeout_and_precancelled_writes_leave_no_test_files() -> Result<()> {
    assert!(
        command::run(
            "/bin/sleep",
            &["5"],
            Duration::from_millis(100),
            &Control::default()
        )
        .is_err()
    );
    let root = tempfile::tempdir()?;
    let control = Control::default();
    control.cancel();
    assert!(operations::benchmark(root.path(), 1024 * 1024, &control).is_err());
    assert_eq!(fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[test]
fn widget_settings_roundtrip_and_private_permissions() -> Result<()> {
    use ssd_scope_mac::settings::Settings;
    let root = tempfile::tempdir()?;
    let path = root.path().join("preferences/settings.json");
    let mut settings = Settings::load(&path)?;
    settings.menu_bar_enabled = false;
    settings.widget_enabled = false;
    settings.widget_mode = 1;
    settings.close_to_menu_bar = false;
    settings.save(&path)?;
    assert_eq!(Settings::load(&path)?, settings);
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o077, 0);
    assert_eq!(
        fs::metadata(path.parent().unwrap())?.permissions().mode() & 0o077,
        0
    );
    Ok(())
}

#[test]
fn settings_reject_oversized_invalid_and_future_schema() -> Result<()> {
    use ssd_scope_mac::settings::Settings;
    let root = tempfile::tempdir()?;
    let path = root.path().join("settings.json");
    for data in [
        vec![b' '; 16 * 1024 + 1],
        b"not-json".to_vec(),
        br#"{"version":999}"#.to_vec(),
        br#"{"widget_mode":99}"#.to_vec(),
    ] {
        fs::write(&path, data)?;
        assert!(Settings::load(&path).is_err());
    }
    Ok(())
}

#[test]
fn settings_do_not_follow_a_symlink_to_another_file() -> Result<()> {
    let root = tempfile::tempdir()?;
    let target = root.path().join("other.json");
    let alias = root.path().join("settings.json");
    fs::write(&target, b"{}")?;
    symlink(&target, &alias)?;
    assert!(ssd_scope_mac::settings::Settings::load(&alias).is_err());
    assert_eq!(fs::read(target)?, b"{}");
    Ok(())
}
