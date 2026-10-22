use anyhow::Result;
use eframe::egui::{self, RichText};
use ssd_scope_mac::{
    command::Control,
    disks::{self, Drive, Inventory},
    native,
    operations::{self, Benchmark},
    smart::SmartReport,
    telemetry::History,
    usb_cache,
    usb_report::Snapshot,
};
mod native_menu;
mod statusbar;
mod ui;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use ui::{theme::Theme, widgets};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

enum Event {
    Inventory(Inventory, HashMap<String, Snapshot>),
    Benchmark(Benchmark),
    Text(String),
    Backup(PathBuf),
    Smart(Box<SmartReport>),
    UsbSnapshot(String, Box<Snapshot>),
    SnapshotRemoved(String),
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
    usb_snapshots: HashMap<String, Snapshot>,
    histories: HashMap<String, History>,
    tray: Option<statusbar::StatusBar>,
    native_menu: Option<native_menu::NativeMenu>,
    settings: ssd_scope_mac::settings::Settings,
    settings_path: PathBuf,
    logo: egui::TextureHandle,
    started: Instant,
    last_sample: Instant,
    last_refresh: Instant,
    quitting: bool,
}

impl ScopeApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::theme::install(&cc.egui_ctx);
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied(
                [128, 128],
                &ssd_scope_mac::icons::rgba(128, false),
            ),
            egui::TextureOptions::LINEAR,
        );
        let settings_path = ssd_scope_mac::settings::default_path();
        let settings = ssd_scope_mac::settings::Settings::load(&settings_path).unwrap_or_default();
        ui::i18n::set(settings.language);
        ui::theme::set_appearance(&cc.egui_ctx, settings.appearance);
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
            usb_snapshots: HashMap::new(),
            histories: HashMap::new(),
            tray,
            native_menu,
            settings,
            settings_path,
            logo,
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
        self.busy = ui::i18n::tr(label);
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
        self.start(ctx, "Reading drive information…", |control| {
            inventory_event(&control)
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
            Ok(Event::Inventory(inventory, snapshots)) => {
                if !inventory.drives.iter().any(|d| d.id == self.selected) {
                    self.selected = inventory
                        .drives
                        .iter()
                        .find(|d| !d.internal)
                        .or_else(|| inventory.drives.first())
                        .map(|d| d.id.clone())
                        .unwrap_or_default();
                }
                self.log = inventory.warnings.join("\n");
                self.inventory = inventory;
                self.usb_snapshots = snapshots;
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
            Ok(Event::UsbSnapshot(device, snapshot)) => {
                self.log = snapshot.warnings.join("\n");
                self.usb_snapshots.insert(device, *snapshot);
            }
            Ok(Event::SnapshotRemoved(device)) => {
                self.usb_snapshots.remove(&device);
                self.log = "Saved SMART snapshot removed.".into();
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
        } else if let Some(snapshot) = self.usb_snapshots.get(&drive.id) {
            drive.health = snapshot.health.clone();
        }
        Some(drive)
    }
    fn snapshot(&self, id: &str) -> Option<&Snapshot> {
        if self
            .smart_reports
            .get(id)
            .is_some_and(|r| r.health.is_some())
        {
            None
        } else {
            self.usb_snapshots.get(id)
        }
    }
    fn handle_menu(&mut self, ctx: &egui::Context) {
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
                "scope.help" => open_url(
                    "https://github.com/ChakkritGit/SSD-Scope-for-Mac/blob/main/docs/USB-SMART.md",
                ),
                "scope.project" => open_url("https://github.com/ChakkritGit/SSD-Scope-for-Mac"),
                _ => {}
            }
        }
    }
    fn sample(&mut self) {
        if self.last_sample.elapsed() < Duration::from_secs(1) {
            return;
        }
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
                (read, write),
                self.settings.widget_mode,
                self.settings.widget_enabled,
                state,
                self.snapshot(&self.selected),
            );
        }
    }
    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.confirmation.as_ref() else {
            return;
        };
        let (title, details) = match plan {
            Confirmation::Benchmark(path) => (
                "Run speed test?",
                format!(
                    "A new 256 MiB test file will be written, read back and verified in:\n{}\n\nOnly the temporary test file is removed afterward.",
                    path.display()
                ),
            ),
            Confirmation::Backup {
                source,
                destination,
                verify,
            } => (
                "Start file backup?",
                format!(
                    "Source:\n{}\n\nA NEW backup folder is created inside:\n{}\n\nSHA-256 verification: {}\nThis is a file backup, not a bootable clone.",
                    source.display(),
                    destination.display(),
                    if *verify { "on" } else { "off" }
                ),
            ),
        };
        let mut start = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            let t = Theme::of_ui(ui);
            ui.set_width(440.0);
            ui.label(RichText::new(ui::i18n::tr(title)).size(18.0).strong());
            ui.label(RichText::new(details).color(t.muted));
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                start = widgets::primary(ui, "Start").clicked();
                cancel = widgets::secondary(ui, "Cancel").clicked();
            });
        });
        if cancel || modal.should_close() && !start {
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
        self.handle_menu(ctx);
        self.sample();
        if self.last_refresh.elapsed() > Duration::from_secs(60)
            && self.receiver.is_none()
            && self.confirmation.is_none()
        {
            self.refresh(ctx);
        }
        let close = ctx.input(|input| input.viewport().close_requested());
        if close && self.receiver.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.log =
                "An operation is running. Cancel it and wait for cleanup before closing the app."
                    .into();
        } else if close && !self.quitting && self.tray.is_some() && self.settings.close_to_menu_bar
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.sidebar(ctx);
        self.content(ctx);
        self.confirmation(ctx);
        ctx.request_repaint_after(if self.receiver.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        });
    }
}

fn inventory_event(control: &Control) -> Result<Event> {
    let mut inventory = disks::inventory()?;
    let snapshots = match usb_cache::Store::load(&usb_cache::default_path()) {
        Ok(store) => {
            let (snapshots, warnings) = store.resolve(&inventory, control);
            inventory.warnings.extend(warnings);
            snapshots
        }
        Err(e) => {
            inventory
                .warnings
                .push(format!("Saved SMART snapshot: {e:#}"));
            HashMap::new()
        }
    };
    Ok(Event::Inventory(inventory, snapshots))
}

pub(crate) fn open_url(url: &str) {
    let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([980.0, 680.0])
            .with_title("SSD Scope")
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
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
