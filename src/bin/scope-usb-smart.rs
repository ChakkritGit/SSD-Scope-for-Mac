use anyhow::{Result, bail, ensure};
use ssd_scope_mac::usb_access;
use std::sync::atomic::{AtomicBool, Ordering};

static CANCELLED: AtomicBool = AtomicBool::new(false);
extern "C" fn cancel(_: libc::c_int) {
    CANCELLED.store(true, Ordering::Relaxed);
}
fn main() {
    if let Err(e) = execute() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("probe") if args.len() == 2 => println!(
            "{}",
            serde_json::to_string_pretty(&usb_access::plan(&args[1])?)?
        ),
        Some("capture-read")
            if args.len() == 5
                && args[2] == "--registry-id"
                && args[4] == "--allow-usb-capture" =>
        {
            let registry: u64 = args[3].parse()?;
            ensure!(
                std::env::vars_os().all(|(k, _)| !k.to_string_lossy().starts_with("DYLD_")),
                "Loader overrides are not allowed in capture mode"
            );
            // Signal handlers only set an atomic flag. Bounded I/O returns to Rust cleanup.
            unsafe {
                libc::signal(libc::SIGINT, cancel as *const () as libc::sighandler_t);
                libc::signal(libc::SIGTERM, cancel as *const () as libc::sighandler_t);
            }
            let report = usb_access::capture(&args[1], registry, &CANCELLED)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.driver_reattached {
                std::process::exit(2);
            }
        }
        None | Some("--help") => println!(
            "scope-usb-smart probe diskN\n\nscope-usb-smart capture-read diskN --registry-id NUMBER --allow-usb-capture\n\nExperimental: capture-read requires administrator authorization and an UNMOUNTED target.\nIt temporarily captures only the reviewed RTL9201 USB device, reads IDENTIFY/SMART/SENSE,\nand attempts to restore the macOS driver. It never unmounts, formats or writes SSD data.\nRun the helper from the internal disk / an owned temporary folder, after closing SSD files.\nFull Security is retained. Hardware support has not yet been verified."
        ),
        _ => bail!("Invalid arguments. Use --help; arbitrary CDBs and ATA writes are not accepted"),
    }
    Ok(())
}
