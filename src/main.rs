use anyhow::Result;
use eframe::egui::{self, Color32, RichText};
use ssd_scope_mac::{
    command::Control,
    disks::{self, Drive, Inventory},
    native,
    operations::{self, Benchmark},
    smart::{self, SmartReport},
    telemetry::History,
};
mod charts;
mod native_menu;
mod statusbar;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const ACCENT: Color32 = Color32::from_rgb(36, 139, 112);
const MUTED: Color32 = Color32::from_rgb(113, 125, 121);
const GREEN: Color32 = Color32::from_rgb(36, 139, 112);
const AMBER: Color32 = Color32::from_rgb(169, 110, 30);
const TEXT: Color32 = Color32::from_rgb(34, 51, 44);

enum Event {
    Inventory(Inventory),
    Benchmark(Benchmark),
    Text(String),
    Backup(PathBuf),
    Smart(Box<SmartReport>),
}
enum Confirmation {
    Benchmark(PathBuf),
    Backup {
        source: PathBuf,
        destination: PathBuf,
        verify: bool,
    },
}

struct ScopeApp {
    inventory: Inventory,
    selected: String,
    tab: usize,
    receiver: Option<mpsc::Receiver<std::result::Result<Event, String>>>,
    control: Control,
    busy: String,
    log: String,
    error: bool,
    stale: bool,
    benchmark: Option<Benchmark>,
    source: Option<PathBuf>,
    destination: Option<PathBuf>,
    verify: bool,
    result_folder: Option<PathBuf>,
    confirmation: Option<Confirmation>,
    smart_reports: HashMap<String, SmartReport>,
    histories: HashMap<String, History>,
    tray: Option<statusbar::StatusBar>,
    native_menu: Option<native_menu::NativeMenu>,
    settings: ssd_scope_mac::settings::Settings,
    settings_path: PathBuf,
    started: Instant,
    last_sample: Instant,
    last_refresh: Instant,
    quitting: bool,
}

impl ScopeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = Color32::from_rgb(247, 249, 247);
        visuals.window_fill = Color32::WHITE;
        visuals.selection.bg_fill = Color32::from_rgb(220, 237, 228);
        visuals.selection.stroke = egui::Stroke::new(1.0_f32, TEXT);
        visuals.override_text_color = Some(TEXT);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(236, 241, 237);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(223, 235, 226);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(12.0, 12.0);
            style.spacing.button_padding = egui::vec2(12.0, 8.0);
        });
        let settings_path = ssd_scope_mac::settings::default_path();
        let settings = ssd_scope_mac::settings::Settings::load(&settings_path).unwrap_or_default();
        let native_menu = native_menu::NativeMenu::new(&cc.egui_ctx).ok();
        let tray = if settings.menu_bar_enabled {
            statusbar::StatusBar::new(settings.widget_enabled).ok()
        } else {
            None
        };
        let mut app = Self {
            inventory: Inventory::default(),
            selected: String::new(),
            tab: 0,
            receiver: None,
            control: Control::default(),
            busy: String::new(),
            log: String::new(),
            error: false,
            stale: true,
            benchmark: None,
            source: None,
            destination: None,
            verify: true,
            result_folder: None,
            confirmation: None,
            smart_reports: HashMap::new(),
            histories: HashMap::new(),
            tray,
            native_menu,
            settings,
            settings_path,
            started: Instant::now(),
            last_sample: Instant::now() - Duration::from_secs(2),
            last_refresh: Instant::now(),
            quitting: false,
        };
        app.refresh(&cc.egui_ctx);
        app
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        label: &str,
        work: impl FnOnce(Control) -> Result<Event> + Send + 'static,
    ) {
        if self.receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let control = Control::default();
        let worker_control = control.clone();
        let ctx = ctx.clone();
        self.control = control;
        self.receiver = Some(rx);
        self.busy = label.into();
        self.error = false;
        self.log.clear();
        std::thread::spawn(move || {
            let result = work(worker_control).map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn refresh(&mut self, ctx: &egui::Context) {
        self.stale = true;
        self.start(ctx, "Reading drive information…", |_| {
            Ok(Event::Inventory(disks::inventory()?))
        });
    }
    fn poll(&mut self) {
        let result = self.receiver.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Background task stopped unexpectedly".into()))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        });
        let Some(result) = result else { return };
        self.receiver = None;
        self.busy.clear();
        match result {
            Ok(Event::Inventory(inventory)) => {
                if !inventory.drives.iter().any(|d| d.id == self.selected) {
                    self.selected = inventory
                        .drives
                        .iter()
                        .find(|d| d.volumes.iter().any(|v| v.mount == "/Volumes/SSD256GB"))
                        .or_else(|| inventory.drives.iter().find(|d| !d.internal))
                        .or_else(|| inventory.drives.first())
                        .map(|d| d.id.clone())
                        .unwrap_or_default();
                }
                self.log = inventory.warnings.join("\n");
                self.inventory = inventory;
                self.stale = false;
                self.last_refresh = Instant::now();
            }
            Ok(Event::Benchmark(value)) => {
                self.log = format!(
                    "Sequential test completed.\nWrite: {:.1} MB/s\nRead + read-back verification: {:.1} MB/s\nTemporary test file removed.",
                    value.write_mbps, value.read_mbps
                );
                self.benchmark = Some(value);
            }
            Ok(Event::Text(text)) => self.log = text,
            Ok(Event::Smart(report)) => {
                let report = *report;
                self.log = report.explanation.clone();
                self.smart_reports.insert(report.device.clone(), report);
            }
            Ok(Event::Backup(path)) => {
                self.log = format!(
                    "File backup completed: {}\nOpen the Files folder to access your data.",
                    path.display()
                );
                self.result_folder = Some(path);
            }
            Err(error) => {
                self.error = true;
                self.log = error;
            }
        }
    }
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.clone();
        section(ui, "Menu bar & widget", |ui| {
            ui.label("Choose what appears at the top right of your Mac. Changes are saved automatically.");
            ui.checkbox(
                &mut self.settings.menu_bar_enabled,
                "Show SSD Scope in the menu bar",
            );
            ui.add_enabled_ui(self.settings.menu_bar_enabled, |ui| {
                ui.checkbox(
                    &mut self.settings.widget_enabled,
                    "Show a value beside the disk icon",
                );
                ui.add_enabled_ui(self.settings.widget_enabled, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Widget value:");
                        ui.selectable_value(&mut self.settings.widget_mode, 0, "Free space");
                        ui.selectable_value(&mut self.settings.widget_mode, 1, "Read / write MB/s");
                        ui.selectable_value(&mut self.settings.widget_mode, 2, "Temperature");
                    });
                });
                ui.checkbox(
                    &mut self.settings.close_to_menu_bar,
                    "Keep the menu bar widget running when the window closes",
                );
            });
            ui.label(
                RichText::new(
                    "Temperature is displayed only when a valid SMART reading is available.",
                )
                .small()
                .color(MUTED),
            );
        });
        if before != self.settings {
            if before.menu_bar_enabled != self.settings.menu_bar_enabled {
                self.tray = if self.settings.menu_bar_enabled {
                    match statusbar::StatusBar::new(self.settings.widget_enabled) {
                        Ok(tray) => Some(tray),
                        Err(error) => {
                            self.error = true;
                            self.log = error.to_string();
                            None
                        }
                    }
                } else {
                    None
                };
            }
            if let Err(error) = self.settings.save(&self.settings_path) {
                self.error = true;
                self.log = format!("Could not save settings: {error:#}");
            }
            self.last_sample = Instant::now() - Duration::from_secs(2);
        }
    }
    fn drive(&self) -> Option<Drive> {
        let mut drive = self
            .inventory
            .drives
            .iter()
            .find(|d| d.id == self.selected)
            .cloned()?;
        if let Some(health) = self
            .smart_reports
            .get(&drive.id)
            .and_then(|r| r.health.as_ref())
        {
            drive.health = health.clone();
        }
        Some(drive)
    }
    fn overview(&mut self, ui: &mut egui::Ui, drive: &Drive) {
        let health = &drive.health;
        ui.horizontal(|ui| {
            metric(ui, "CAPACITY", &disks::bytes(drive.capacity), ACCENT);
            metric(ui, "CONNECTION", &drive.bus, TEXT);
            metric(
                ui,
                "TEMPERATURE",
                &health
                    .temperature_c
                    .map(|n| format!("{n:.0} °C"))
                    .unwrap_or("Unavailable".into()),
                AMBER,
            );
        });
        ui.add_space(8.0);
        section(ui, "Activity & capacity", |ui| {
            ui.columns(2, |columns| {
                charts::activity(&mut columns[0], self.histories.get(&drive.id));
                if let Some(volume) = drive.volumes.first()
                    && let Some(free) = volume.free
                {
                    columns[1].vertical_centered(|ui| {
                        ui.label(RichText::new(&volume.name).strong());
                        charts::capacity(ui, volume.capacity, free);
                    });
                }
            });
        });
        ui.add_space(8.0);
        section(ui, "Drive health", |ui| {
            if health.status == "Verified" {
                ui.label(
                    RichText::new("SMART status: Verified")
                        .color(GREEN)
                        .size(20.0),
                );
            } else {
                ui.label(
                    RichText::new(format!("SMART status: {}", health.status))
                        .color(AMBER)
                        .size(20.0),
                );
            }
            if let Some(life) = health.remaining_life_estimate() {
                ui.add(
                    egui::ProgressBar::new(life as f32 / 100.0)
                        .text(format!("{life:.0}% estimated endurance remaining")),
                );
                ui.label(RichText::new("Derived from NVMe Percentage Used; this is not a guarantee of drive health.").small().color(MUTED));
            } else {
                ui.label("macOS does not expose endurance data for this connection.");
            }
            if health.status == "Not Supported" {
                ui.label("The current macOS driver did not return SMART data. Open S.M.A.R.T. → Check USB SMART for the bridge diagnosis. This does not mean the SSD lacks SMART.");
            }
        });
        ui.add_space(8.0);
        section(ui, "Mounted volumes", |ui| {
            if drive.volumes.is_empty() {
                ui.label("No accessible mounted data volumes. Unlock or mount the drive in Disk Utility, then Refresh.");
            }
            for volume in &drive.volumes {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&volume.name).strong().size(18.0));
                    ui.label(
                        RichText::new(format!("{} · {}", volume.filesystem, volume.id))
                            .color(MUTED),
                    );
                    if ui.small_button("Show in Finder").clicked() {
                        reveal(Path::new(&volume.mount));
                    }
                });
                ui.label(
                    RichText::new(&volume.mount)
                        .monospace()
                        .small()
                        .color(MUTED),
                );
                if let Some(free) = volume.free {
                    let used = volume.capacity.saturating_sub(free);
                    let fraction = if volume.capacity == 0 {
                        0.0
                    } else {
                        used as f32 / volume.capacity as f32
                    };
                    ui.add(egui::ProgressBar::new(fraction).text(format!(
                        "{} used · {} free",
                        disks::bytes(used),
                        disks::bytes(free)
                    )));
                    if volume.filesystem == "APFS" {
                        ui.label(
                            RichText::new("APFS volumes share container capacity.")
                                .small()
                                .color(MUTED),
                        );
                    }
                }
                ui.separator();
            }
        });
    }
    fn smart(&mut self, ui: &mut egui::Ui, drive: &Drive, ctx: &egui::Context) {
        let health = &drive.health;
        section(ui, "S.M.A.R.T. / NVMe telemetry", |ui| {
            egui::Grid::new("smart-metrics")
                .num_columns(2)
                .spacing([50.0, 15.0])
                .show(ui, |ui| {
                    for (key, value) in [
                        ("macOS SMART status", health.status.clone()),
                        (
                            "Temperature",
                            health
                                .temperature_c
                                .map(|v| format!("{v:.1} °C"))
                                .unwrap_or("Unavailable".into()),
                        ),
                        (
                            "Percentage used",
                            health
                                .percentage_used
                                .map(|v| format!("{v:.0}%"))
                                .unwrap_or("Unavailable".into()),
                        ),
                        ("Power-on hours", optional_number(health.power_on_hours)),
                        ("Power cycles", optional_number(health.power_cycles)),
                        ("Unsafe shutdowns", optional_number(health.unsafe_shutdowns)),
                        ("Media errors", optional_number(health.media_errors)),
                        (
                            "Total data read",
                            health
                                .bytes_read
                                .map(|v| disks::bytes(v as u64))
                                .unwrap_or("Unavailable".into()),
                        ),
                        (
                            "Total data written",
                            health
                                .bytes_written
                                .map(|v| disks::bytes(v as u64))
                                .unwrap_or("Unavailable".into()),
                        ),
                    ] {
                        ui.label(RichText::new(key).color(MUTED));
                        ui.label(value);
                        ui.end_row();
                    }
                });
        });
        ui.add_space(8.0);
        section(ui, "USB SMART connection", |ui| {
            ui.label("Experimental USB SMART support uses bundled smartctl with auto, ATA and SAT read modes. Direct USB access is not implemented. No self-tests, firmware commands or disk writes are sent.");
            if smart::helper_path().is_some() {
                if ui.button("Check USB SMART").clicked() {
                    let id = drive.id.clone();
                    self.start(ctx, "Reading extended SMART…", move |control| {
                        Ok(Event::Smart(Box::new(smart::diagnose(&id, &control)?)))
                    });
                }
            } else {
                ui.label(
                    RichText::new("Bundled smartctl is missing. Rebuild with scripts/package.sh.")
                        .color(AMBER),
                );
                ui.hyperlink_to("smartmontools project", "https://www.smartmontools.org/");
            }
            if let Some(report) = self.smart_reports.get(&drive.id) {
                ui.separator();
                if let Some(bridge) = &report.bridge {
                    ui.label(RichText::new(bridge.label()).strong());
                    ui.label(format!("USB {} · {}", bridge.usb_id(), bridge.driver));
                }
                ui.label(
                    RichText::new(&report.explanation).color(if report.health.is_some() {
                        GREEN
                    } else {
                        AMBER
                    }),
                );
                for attempt in &report.attempts {
                    ui.collapsing(
                        format!("{} · exit {}", attempt.mode, attempt.exit_code),
                        |ui| {
                            for line in &attempt.messages {
                                ui.label(line);
                            }
                        },
                    );
                }
                if report.conclusion == "transport_unavailable" {
                    ui.hyperlink_to(
                        "SAT SMART driver option and compatibility information",
                        "https://binaryfruit.com/drivedx/usb-drive-support",
                    );
                    ui.label(RichText::new("The app does not install kernel drivers or change your Mac's security settings.").small().color(MUTED));
                }
            }
        });
        if !health.raw.is_empty() {
            ui.collapsing("Raw values reported by macOS", |ui| {
                for (key, value) in &health.raw {
                    ui.monospace(format!("{key}: {value}"));
                }
            });
        }
    }
    fn tools(&mut self, ui: &mut egui::Ui, drive: &Drive, ctx: &egui::Context) {
        section(ui, "File system verification", |ui| {
            ui.label("Run macOS verifyVolume on a selected volume. This checks the file system, not SSD cells, and may pause access to the volume. It does not request repairs.");
            for volume in &drive.volumes {
                if ui.button(format!("Verify {}", volume.name)).clicked() {
                    let volume = volume.clone();
                    self.start(ctx, "Verifying file system…", move |control| {
                        Ok(Event::Text(disks::verify_volume(&volume, &control)?))
                    });
                }
            }
        });
        ui.add_space(8.0);
        section(ui, "Sequential speed test", |ui| {
            ui.label("Writes a new 256 MiB temporary file in a folder you choose, then reads and verifies it. The file is removed afterward. OS caching is disabled for the test file; hardware caching still affects results.");
            if ui.button("Choose test folder…").clicked() {
                let mut dialog =
                    rfd::FileDialog::new().set_title("Choose a folder on the drive to test");
                if let Some(volume) = drive.volumes.first() {
                    dialog = dialog.set_directory(&volume.mount);
                }
                if let Some(path) = dialog.pick_folder() {
                    self.confirmation = Some(Confirmation::Benchmark(path));
                }
            }
            if let Some(result) = &self.benchmark {
                ui.label(format!("Test folder: {}", result.directory.display()));
                ui.horizontal(|ui| {
                    metric(
                        ui,
                        "LAST WRITE",
                        &format!("{:.1} MB/s", result.write_mbps),
                        ACCENT,
                    );
                    metric(
                        ui,
                        "LAST READ + VERIFY",
                        &format!("{:.1} MB/s", result.read_mbps),
                        GREEN,
                    );
                });
            }
        });
        ui.add_space(8.0);
        section(ui, "Firmware / TRIM / Secure erase", |ui| {
            ui.label("Vendor firmware flashing, forced TRIM, secure erase, raw disk restore and bootable system cloning are not implemented. The Windows package and its drivers cannot provide these operations on macOS.");
            ui.hyperlink_to(
                "Transcend SSD Scope support",
                "https://transcend-info.com/support/software/ssd-scope",
            );
        });
    }
    fn backup(&mut self, ui: &mut egui::Ui, drive: &Drive) {
        section(ui, "Back up data files", |ui| {
            ui.label("Copies a folder or mounted external volume into a NEW backup folder. Existing destination files are never merged or erased. This does not create a bootable clone or an APFS snapshot. Close apps that change source files before starting.");
            ui.add_space(5.0);
            ui.label(RichText::new("SOURCE").small().color(MUTED));
            ui.horizontal(|ui| {
                if ui.button("Choose source…").clicked() {
                    self.source = rfd::FileDialog::new().pick_folder().or(self.source.take());
                }
                if let Some(volume) = drive.volumes.first()
                    && !drive.internal
                    && ui.button(format!("Use {}", volume.name)).clicked()
                {
                    self.source = Some(PathBuf::from(&volume.mount));
                }
            });
            ui.label(
                self.source
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or("No source selected".into()),
            );
            ui.label(
                RichText::new("DESTINATION PARENT FOLDER")
                    .small()
                    .color(MUTED),
            );
            if ui.button("Choose destination…").clicked() {
                self.destination = rfd::FileDialog::new()
                    .pick_folder()
                    .or(self.destination.take());
            }
            ui.label(
                self.destination
                    .as_deref()
                    .map(|p| p.display().to_string())
                    .unwrap_or("No destination selected".into()),
            );
            ui.checkbox(
                &mut self.verify,
                "Verify every regular file with SHA-256 after copying",
            );
            ui.label(RichText::new("Permissions, extended attributes and resource forks are copied by macOS ditto. Verification checks file contents and symbolic links. Insufficient permissions or a changed source will be reported.").small().color(MUTED));
            if let (Some(source), Some(destination)) = (&self.source, &self.destination) {
                match operations::validate_backup(source, destination) {
                    Ok(_) => {
                        if ui
                            .button(RichText::new("Review and start backup…").color(ACCENT))
                            .clicked()
                        {
                            self.confirmation = Some(Confirmation::Backup {
                                source: source.clone(),
                                destination: destination.clone(),
                                verify: self.verify,
                            });
                        }
                    }
                    Err(error) => {
                        ui.colored_label(AMBER, format!("{error:#}"));
                    }
                }
            }
        });
        ui.add_space(8.0);
        section(ui, "Recover files from a Scope backup", |ui| {
            ui.label("Choose a completed Scope Backup folder. Its Files folder becomes the source above. Select a new destination, then start a verified copy. Existing data is kept intact.");
            if ui.button("Choose completed backup…").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                match operations::restore_source(&path) {
                    Ok(files) => {
                        self.source = Some(files);
                        self.destination = None;
                        self.verify = true;
                        self.error = false;
                    }
                    Err(error) => {
                        self.error = true;
                        self.log = format!("Cannot open backup: {error:#}");
                    }
                }
            }
        });
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.confirmation.as_ref() else {
            return;
        };
        let details = match plan {
            Confirmation::Benchmark(path) => format!(
                "Create, write and read a new 256 MiB test file in:\n{}\n\nOnly the temporary test file will be removed afterward.",
                path.display()
            ),
            Confirmation::Backup {
                source,
                destination,
                verify,
            } => format!(
                "Source:\n{}\n\nCreate a NEW backup folder inside:\n{}\n\nSHA-256 content verification: {}\nThis is a file backup, not a bootable clone.",
                source.display(),
                destination.display(),
                if *verify { "on" } else { "off" }
            ),
        };
        let mut start = false;
        let mut cancel = false;
        egui::Window::new("Review operation")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(500.0);
                ui.label(details);
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    start = ui.button(RichText::new("Start").color(ACCENT)).clicked();
                });
            });
        if cancel {
            self.confirmation = None;
        }
        if start && let Some(plan) = self.confirmation.take() {
            match plan {
                Confirmation::Benchmark(path) => self.start(
                    ctx,
                    "Writing, reading and verifying test file…",
                    move |control| {
                        Ok(Event::Benchmark(operations::benchmark(
                            &path,
                            256 * 1024 * 1024,
                            &control,
                        )?))
                    },
                ),
                Confirmation::Backup {
                    source,
                    destination,
                    verify,
                } => self.start(
                    ctx,
                    "Copying files, then verifying SHA-256…",
                    move |control| {
                        Ok(Event::Backup(operations::backup(
                            &source,
                            &destination,
                            verify,
                            &control,
                        )?))
                    },
                ),
            }
        }
    }
}

impl eframe::App for ScopeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        while let Some(action) = self
            .native_menu
            .as_ref()
            .and_then(|menu| menu.next_action())
        {
            match action.as_str() {
                native_menu::SHOW => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                native_menu::REFRESH => {
                    if self.receiver.is_none() {
                        self.refresh(ctx);
                    }
                }
                native_menu::QUIT => {
                    if self.receiver.is_some() {
                        self.control.cancel();
                        self.log =
                            "Cancelling the active operation. Choose Quit again after cleanup."
                                .into();
                    } else {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                native_menu::SETTINGS => self.tab = 4,
                "scope.overview" => self.tab = 0,
                "scope.smart" => self.tab = 1,
                "scope.tools" => self.tab = 2,
                "scope.backup" => self.tab = 3,
                "scope.close" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                "scope.help" => {
                    let _ = std::process::Command::new("/usr/bin/open")
                        .arg("https://github.com/ChakkritGit/Transcend-SSD-Scope-for-Mac/blob/main/docs/USB-SMART.md")
                        .spawn();
                }
                "scope.project" => {
                    let _ = std::process::Command::new("/usr/bin/open")
                        .arg("https://github.com/ChakkritGit/Transcend-SSD-Scope-for-Mac")
                        .spawn();
                }
                _ => {}
            }
        }
        if self.last_sample.elapsed() >= Duration::from_secs(1) {
            self.last_sample = Instant::now();
            if let Ok(snapshot) = native::inspect(&self.selected)
                && let Some(counters) = snapshot.counters
            {
                self.histories
                    .entry(self.selected.clone())
                    .or_default()
                    .push(counters, self.started.elapsed().as_secs_f64());
            }
            if let Some(tray) = &self.tray {
                let drive = self.drive();
                let (read, write) = self
                    .histories
                    .get(&self.selected)
                    .and_then(|h| h.points.back())
                    .map(|p| (p.read_mbps, p.write_mbps))
                    .unwrap_or_default();
                let state = self
                    .smart_reports
                    .get(&self.selected)
                    .map(|r| r.conclusion.as_str());
                tray.update(
                    drive.as_ref(),
                    read,
                    write,
                    self.settings.widget_mode,
                    self.settings.widget_enabled,
                    state,
                );
            }
        }
        if self.last_refresh.elapsed() > Duration::from_secs(60)
            && self.receiver.is_none()
            && self.confirmation.is_none()
        {
            self.refresh(ctx);
        }
        if ctx.input(|input| input.viewport().close_requested()) && self.receiver.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.log =
                "An operation is running. Cancel it and wait for cleanup before closing the app."
                    .into();
        }
        if ctx.input(|input| input.viewport().close_requested())
            && self.receiver.is_none()
            && !self.quitting
            && self.tray.is_some()
            && self.settings.close_to_menu_bar
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let busy = self.receiver.is_some();
        egui::SidePanel::left("drives").exact_width(235.0).show(ctx, |ui| {
            ui.add_space(20.0);
            ui.horizontal(|ui|{charts::glyph(ui,0,ACCENT);ui.label(RichText::new("SSD Scope").size(24.0).strong().color(TEXT));});
            ui.label(RichText::new("A little clarity for your storage.").small().color(MUTED));
            ui.add_space(24.0);
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (index,title) in ["Overview","S.M.A.R.T.","Tools","File backup","Settings"].iter().enumerate(){
                    if charts::navigation(ui,index,title,self.tab==index).clicked() {
                        self.tab=index;
                    }
                }
            });
            ui.add_space(24.0);
            ui.label(RichText::new("PHYSICAL DRIVES").small().color(MUTED));
            for drive in &self.inventory.drives {
                let volume_names = drive.volumes.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", ");
                let title = if volume_names.is_empty() { &drive.name } else { &volume_names };
                let label = format!("{}\n{} · {}", title, drive.bus, disks::bytes(drive.capacity));
                if ui.add_enabled(!busy, egui::Button::new(label).selected(self.selected == drive.id).min_size(egui::vec2(210.0, 64.0))).clicked() {
                    self.selected = drive.id.clone();
                    self.benchmark = None;
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                ui.label(RichText::new("Experimental utility · v0.3.0\nApple Silicon · macOS 14+\nNot affiliated with Transcend").small().color(MUTED));
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if busy {
                    ui.spinner();
                    ui.label(&self.busy);
                    if ui.button("Cancel operation").clicked() {
                        self.control.cancel();
                    }
                } else {
                    ui.colored_label(
                        if self.error { AMBER } else { GREEN },
                        if self.error {
                            "Operation needs attention"
                        } else {
                            "Ready"
                        },
                    );
                    ui.label(RichText::new("Data stays on this Mac.").color(MUTED));
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(14.0);
            let drive = self.drive();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading(drive.as_ref().and_then(|d|d.volumes.first()).map(|v|v.name.as_str()).unwrap_or("Your storage, at a glance"));
                    if let Some(drive) = &drive { ui.label(RichText::new(format!("{} · {} · {}", drive.id, if drive.internal { "Internal" } else { "External" }, if drive.ssd == Some(true) { "Solid state drive" } else { "Storage device" })).color(MUTED)); }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Refresh")).clicked() { self.refresh(ctx); }
                    if ui.add_enabled(!busy && !self.inventory.drives.is_empty(), egui::Button::new("Export report")).clicked()
                        && let Some(path) = rfd::FileDialog::new().set_file_name("ssd-scope-report.json").add_filter("JSON", &["json"]).save_file() {
                            match serde_json::to_vec_pretty(&self.inventory).map_err(anyhow::Error::from).and_then(|data| Ok(std::fs::write(path, data)?)) {
                                Ok(()) => { self.log = "Drive report exported.".into(); self.error = false; },
                                Err(error) => { self.log = format!("{error:#}"); self.error = true; },
                            }
                        }
                });
            });
            if self.stale && !self.inventory.drives.is_empty() { ui.colored_label(AMBER, "Showing the previous capture. Refresh is pending or failed."); }
            ui.add_space(10.0);
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_enabled_ui(!busy && self.confirmation.is_none(), |ui| {
                    if let Some(drive) = drive {
                        match self.tab { 0 => self.overview(ui, &drive), 1 => self.smart(ui, &drive, ctx), 2 => self.tools(ui, &drive, ctx), 3 => self.backup(ui, &drive), _=>self.settings_page(ui) }
                    } else if self.tab==4 { self.settings_page(ui); } else if !busy {
                        ui.label("No drives could be read. Press Refresh. If macOS denies disk access, check the operation output below.");
                    }
                });
                if !self.log.is_empty() {
                    ui.add_space(12.0);
                    ui.colored_label(if self.error { AMBER } else { ACCENT }, "Operation output");
                    egui::ScrollArea::both().max_height(230.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(&mut self.log).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(false));
                    });
                    if let Some(path) = &self.result_folder
                        && ui.button("Show completed backup").clicked() { reveal(path); }
                }
            });
        });
        self.confirmation(ctx);
        ctx.request_repaint_after(if busy {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        });
    }
}

fn optional_number(value: Option<f64>) -> String {
    value
        .map(|n| format!("{n:.0}"))
        .unwrap_or("Unavailable".into())
}
fn metric(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(228, 234, 229)))
        .corner_radius(12)
        .inner_margin(18)
        .show(ui, |ui| {
            ui.set_min_width(155.0);
            ui.label(RichText::new(label).small().color(MUTED));
            ui.label(RichText::new(value).size(24.0).strong().color(color));
        });
}
fn section(ui: &mut egui::Ui, title: &str, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(228, 234, 229)))
        .corner_radius(12)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(19.0).strong());
            contents(ui);
        });
}
fn reveal(path: &Path) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn();
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 820.0])
            .with_min_inner_size([1020.0, 700.0])
            .with_icon(egui::IconData {
                rgba: ssd_scope_mac::icons::rgba(128, false),
                width: 128,
                height: 128,
            }),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "SSD Scope for Mac",
        options,
        Box::new(|cc| Ok(Box::new(ScopeApp::new(cc)))),
    )
}
