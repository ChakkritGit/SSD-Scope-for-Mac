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
            "scope-cli inventory\nscope-cli smart diskN\nscope-cli benchmark /test/folder\nscope-cli backup /source/folder /destination/parent\n\nbenchmark writes then removes a new 256 MiB temporary file.\nbackup creates a NEW file backup and verifies regular-file SHA-256. No erase or raw clone commands."
        ),
        _ => bail!("Invalid command. Use --help"),
    }
    Ok(())
}
