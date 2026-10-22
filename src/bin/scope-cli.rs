use anyhow::{Result, bail};
use ssd_scope_mac::{command::Control, disks, operations};
use std::path::Path;

fn main() {
    if let Err(error) = execute() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("import-usb-report") if args.len() == 3 => {
            let inventory = disks::inventory()?;
            let drive = inventory
                .drives
                .iter()
                .find(|d| d.id == args[2])
                .ok_or_else(|| anyhow::anyhow!("Selected disk is unavailable"))?;
            let snapshot = ssd_scope_mac::usb_cache::import(
                Path::new(&args[1]),
                drive,
                &ssd_scope_mac::usb_cache::default_path(),
                &Control::default(),
            )?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Some("decode-usb-report") if args.len() == 2 => {
            let raw = ssd_scope_mac::usb_report::load(Path::new(&args[1]))?;
            let report = ssd_scope_mac::usb_report::decode(&raw, &Control::default())?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Some("inventory") if args.len() == 1 => {
            println!("{}", serde_json::to_string_pretty(&disks::inventory()?)?)
        }
        Some("smart") if args.len() == 2 => {
            println!("{}", disks::extended_smart(&args[1], &Control::default())?)
        }
        Some("registry") if args.len() == 2 => println!(
            "{}",
            serde_json::to_string_pretty(&ssd_scope_mac::native::inspect(&args[1])?)?
        ),
        Some("write-icons") if args.len() == 2 => {
            let folder = Path::new(&args[1]);
            std::fs::create_dir_all(folder)?;
            for size in [16, 32, 64, 128, 256, 512, 1024] {
                ssd_scope_mac::icons::write_png(&folder.join(format!("icon-{size}.png")), size)?;
            }
            ssd_scope_mac::icons::write_icns(folder)?;
        }
        Some("benchmark") if args.len() == 2 => println!(
            "{}",
            serde_json::to_string_pretty(&operations::benchmark(
                Path::new(&args[1]),
                256 * 1024 * 1024,
                &Control::default()
            )?)?
        ),
        Some("backup") if args.len() == 3 => println!(
            "{}",
            operations::backup(
                Path::new(&args[1]),
                Path::new(&args[2]),
                true,
                &Control::default()
            )?
            .display()
        ),
        None | Some("--help") => println!(
            "scope-cli inventory\nscope-cli smart diskN\nscope-cli decode-usb-report /capture.json\nscope-cli import-usb-report /capture.json diskN\nscope-cli benchmark /test/folder\nscope-cli backup /source/folder /destination/parent\n\nUSB reports are decoded offline as a regular user.\nbenchmark writes then removes a new 256 MiB temporary file.\nbackup creates a NEW file backup and verifies regular-file SHA-256. No erase or raw clone commands."
        ),
        _ => bail!("Invalid command. Use --help"),
    }
    Ok(())
}
