//! Page layouts. Operations and state live in main.rs; this module only renders and dispatches.
use super::{
    chart,
    i18n::{tr, trf},
    theme::Theme,
    widgets::{self, Glyph},
};
use crate::{Confirmation, Event, ScopeApp, VERSION, statusbar};
use eframe::egui::{self, Color32, RichText, vec2};
use ssd_scope_mac::{
    disks::{self, Drive, Health},
    operations, smart, usb_cache, usb_live,
    usb_report::Snapshot,
};
use std::path::PathBuf;

const PAGES: [(&str, Glyph); 5] = [
    ("Dashboard", Glyph::Dashboard),
    ("Health", Glyph::Health),
    ("Performance", Glyph::Performance),
    ("Backup", Glyph::Backup),
    ("Settings", Glyph::Settings),
];

fn count(n: f64) -> String {
    let digits = (n.max(0.0) as u64).to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Relative capture age, e.g. "2h ago" / "2 ชม.ที่แล้ว".
fn ago(captured_unix: u64) -> String {
    let seconds = disks::unix_now().saturating_sub(captured_unix);
    let (value, unit) = match seconds {
        0..60 => (seconds, "{} s ago"),
        60..3600 => (seconds / 60, "{} min ago"),
        3600..86400 => (seconds / 3600, "{} h ago"),
        _ => (seconds / 86400, "{} days ago"),
    };
    trf(unit, &[value.to_string()])
}

fn bytes(n: Option<f64>) -> String {
    n.map(|n| disks::bytes(n as u64)).unwrap_or("—".into())
}

fn raw(health: &Health, name: &str) -> Option<f64> {
    health.raw.get(name).copied()
}

/// Headline wording and color for the overall health state.
fn verdict(t: &Theme, health: &Health) -> (&'static str, Color32) {
    match (health.remaining_life_estimate(), health.status.as_str()) {
        (_, "Failing") => ("SMART failing", t.bad),
        (Some(_), _) if health.percentage_used.is_some_and(|u| u >= 100.0) => {
            ("Beyond rated endurance", t.bad)
        }
        (Some(life), _) if life >= 70.0 => ("Good", t.good),
        (Some(life), _) if life >= 30.0 => ("Fair", t.warn),
        (Some(_), _) => ("Low endurance", t.bad),
        (None, "Verified") => ("SMART passed", t.good),
        _ => ("Health unknown", t.muted),
    }
}

fn life_color(t: &Theme, life: f64) -> Color32 {
    if life >= 70.0 {
        t.good
    } else if life >= 30.0 {
        t.warn
    } else {
        t.bad
    }
}

fn temperature_color(t: &Theme, celsius: f64) -> Color32 {
    if celsius < 50.0 {
        t.good
    } else if celsius < 70.0 {
        t.warn
    } else {
        t.bad
    }
}

fn attribute_note(id: u8) -> &'static str {
    match id {
        1 => "Read errors corrected or reported by the controller.",
        5 => "Blocks retired and replaced from the spare pool.",
        9 => "Total hours the drive has been powered on.",
        12 => "Number of power on/off cycles.",
        148..=151 => "Erase counts for the SLC cache area.",
        159 => "Single-bit errors corrected in the DRAM cache.",
        160 => "Errors the controller could not correct.",
        161 => "Spare blocks still available for replacing worn blocks.",
        163 => "Bad blocks present when the drive left the factory.",
        164 => "Total block erase operations performed.",
        165 | 166 => "Highest / lowest erase count of any block.",
        167 => "Average erase count per block. Health is calculated from this value.",
        168 => "Rated erase cycles per block (endurance specification).",
        169 => "Remaining lifetime reported by the firmware.",
        177 => "Wear-levelling indicator.",
        181 | 182 => "Program / erase operations that failed.",
        192 => "Unexpected power loss events.",
        194 => "Drive temperature in °C.",
        195 => "Errors recovered by error-correction.",
        196 => "Remapping events after a block failure.",
        199 => "Transfer errors on the link to the USB bridge (cable/enclosure).",
        232 => "Reserved space still available.",
        241 => "Data written by the computer, in 32 MiB units.",
        242 => "Data read by the computer, in 32 MiB units.",
        245 => "Data written to flash including internal writes, in 32 MiB units.",
        _ => "Vendor-specific attribute.",
    }
}

impl ScopeApp {
    pub(crate) fn sidebar(&mut self, ctx: &egui::Context) {
        let t = Theme::of(ctx);
        let busy = self.receiver.is_some();
        egui::SidePanel::left("sidebar")
            .exact_width(232.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(t.sidebar).inner_margin(egui::Margin {
                left: 10,
                right: 10,
                top: 40,
                bottom: 12,
            }))
            .show(ctx, |ui| {
                window_drag(ui, 40.0);
                // Logo and name sit below the traffic lights, as in Finder-style sidebars.
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.add(egui::Image::new(&self.logo).fit_to_exact_size(vec2(32.0, 32.0)));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            RichText::new("SSD Scope")
                                .size(15.5)
                                .strong()
                                .color(t.sidebar_text),
                        );
                        ui.label(RichText::new(tr("for Mac")).size(11.0).color(t.sidebar_muted));
                    });
                });
                ui.add_space(14.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                for (index, (label, glyph)) in PAGES.into_iter().enumerate() {
                    if widgets::nav_item(ui, glyph, label, self.tab == index).clicked() {
                        self.tab = index;
                    }
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(tr("Drives"))
                            .size(11.0)
                            .strong()
                            .color(t.sidebar_muted),
                    );
                });
                let drives = self.inventory.drives.clone();
                for drive in &drives {
                    ui.add_space(0.0);
                    let names = drive
                        .volumes
                        .iter()
                        .map(|v| v.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let title = if names.is_empty() {
                        drive.name.clone()
                    } else {
                        names
                    };
                    let detail = format!("{} · {}", drive.bus, disks::bytes(drive.capacity));
                    let health = if self.selected == drive.id {
                        self.drive().map(|d| d.health)
                    } else {
                        self.usb_snapshots
                            .get(&drive.id)
                            .map(|s| s.health.clone())
                            .or(Some(drive.health.clone()))
                    };
                    let dot = health.map(|h| verdict(&t, &h).1).filter(|c| *c != t.muted);
                    let response = ui.add_enabled_ui(!busy, |ui| {
                        widgets::drive_item(
                            ui,
                            &title,
                            &detail,
                            drive.internal,
                            dot,
                            self.selected == drive.id,
                        )
                    });
                    if response.inner.clicked() {
                        self.selected = drive.id.clone();
                        self.benchmark = None;
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(format!(
                                "v{VERSION} · Apple Silicon\nIndependent, not affiliated with Transcend"
                            ))
                            .size(10.5)
                            .color(t.sidebar_muted),
                        );
                    });
                });
            });
    }

    pub(crate) fn content(&mut self, ctx: &egui::Context) {
        let t = Theme::of(ctx);
        let busy = self.receiver.is_some();
        egui::TopBottomPanel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(t.background)
                    .inner_margin(egui::Margin::symmetric(24, 8)),
            )
            .show_separator_line(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if busy {
                        ui.spinner();
                        ui.label(RichText::new(&self.busy).color(t.text));
                        if widgets::secondary(ui, "Cancel").clicked() {
                            self.control.cancel();
                        }
                    } else {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            rect.center(),
                            4.0,
                            if self.error { t.warn } else { t.good },
                        );
                        ui.label(
                            RichText::new(tr(if self.error {
                                "Needs attention"
                            } else {
                                "Ready"
                            }))
                            .size(12.0)
                            .color(t.muted),
                        );
                        ui.label(
                            RichText::new(tr("· Read-only monitoring · data stays on this Mac"))
                                .size(12.0)
                                .color(t.faint),
                        );
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(t.background).inner_margin(egui::Margin { left: 28, right: 28, top: 18, bottom: 0 }))
            .show(ctx, |ui| {
                window_drag(ui, 18.0);
                let drive = self.drive();
                self.header(ui, drive.as_ref(), busy);
                ui.add_space(10.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_enabled_ui(!busy && self.confirmation.is_none(), |ui| {
                        match (&drive, self.tab) {
                            (_, 4) => self.settings_page(ui),
                            (Some(drive), 0) => self.dashboard(ui, drive),
                            (Some(drive), 1) => self.health_page(ui, drive, ctx),
                            (Some(drive), 2) => self.performance(ui, drive, ctx),
                            (Some(drive), _) => self.backup(ui, drive),
                            (None, _) if !busy => {
                                widgets::card(ui, |ui| {
                                    ui.label(tr("No drives could be read. Press Refresh. If macOS denies disk access, allow it in System Settings → Privacy & Security."));
                                });
                            }
                            _ => {}
                        }
                    });
                    self.output(ui);
                    ui.add_space(20.0);
                });
            });
    }

    fn header(&mut self, ui: &mut egui::Ui, drive: Option<&Drive>, busy: bool) {
        let t = Theme::of_ui(ui);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(
                    RichText::new(tr(PAGES[self.tab.min(4)].0))
                        .size(12.0)
                        .color(t.muted),
                );
                let title = drive
                    .and_then(|d| {
                        d.volumes
                            .first()
                            .map(|v| v.name.clone())
                            .or(Some(d.name.clone()))
                    })
                    .unwrap_or("Your storage".into());
                ui.label(RichText::new(title).size(26.0).color(t.text));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::secondary(ui, &format!("⟳  {}", tr("Refresh")))
                    })
                    .inner
                    .clicked()
                {
                    self.refresh(ui.ctx());
                }
                let export = ui.add_enabled_ui(!busy && !self.inventory.drives.is_empty(), |ui| {
                    widgets::secondary(ui, "Export report")
                });
                if export.inner.clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name("ssd-scope-report.json")
                        .add_filter("JSON", &["json"])
                        .save_file()
                {
                    match serde_json::to_vec_pretty(&self.inventory)
                        .map_err(anyhow::Error::from)
                        .and_then(|data| Ok(std::fs::write(path, data)?))
                    {
                        Ok(()) => {
                            self.log = "Drive report exported.".into();
                            self.error = false;
                        }
                        Err(error) => {
                            self.log = format!("{error:#}");
                            self.error = true;
                        }
                    }
                }
                if self.stale && !self.inventory.drives.is_empty() {
                    widgets::badge(ui, "Refreshing…", t.warn);
                }
            });
        });
    }

    fn output(&mut self, ui: &mut egui::Ui) {
        if self.log.is_empty() {
            return;
        }
        let t = Theme::of_ui(ui);
        ui.add_space(12.0);
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if self.error {
                        "Needs attention"
                    } else {
                        "Activity log"
                    })
                    .strong()
                    .color(if self.error { t.warn } else { t.text }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Dismiss").clicked() {
                        self.log.clear();
                        self.error = false;
                    }
                    if let Some(path) = &self.result_folder
                        && ui.small_button("Show backup in Finder").clicked()
                    {
                        let _ = std::process::Command::new("/usr/bin/open")
                            .arg(path)
                            .spawn();
                    }
                });
            });
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(&self.log)
                            .monospace()
                            .size(12.0)
                            .color(t.muted),
                    );
                });
        });
    }

    fn dashboard(&mut self, ui: &mut egui::Ui, drive: &Drive) {
        let t = Theme::of_ui(ui);
        let health = &drive.health;
        let snapshot = self.snapshot(&drive.id).cloned();
        let life = health.remaining_life_estimate();
        let (state, state_color) = verdict(&t, health);
        widgets::panel(ui, "Drive Information", "", |ui| {
            ui.horizontal_top(|ui| {
                widgets::drive_picture(ui, vec2(150.0, 104.0));
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    ui.set_width(330.0);
                    let source = match &snapshot {
                        Some(s) => trf("Read {}", &[ago(s.captured_unix)]),
                        None => tr("current"),
                    };
                    egui::Grid::new("drive-info")
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            for (key, value) in [
                                (
                                    "Model Name",
                                    snapshot
                                        .as_ref()
                                        .map(|s| s.model.clone())
                                        .unwrap_or(drive.name.clone()),
                                ),
                                ("Capacity", disks::bytes(drive.capacity)),
                                ("Interface", drive.bus.clone()),
                                (
                                    "Location",
                                    tr(if drive.internal {
                                        "Internal"
                                    } else {
                                        "External"
                                    }),
                                ),
                                ("Disk", drive.id.clone()),
                                ("SMART data", source),
                            ] {
                                ui.label(RichText::new(tr(key)).color(t.muted));
                                ui.label(RichText::new(value).color(t.text));
                                ui.end_row();
                            }
                        });
                });
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(tr(state))
                            .size(22.0)
                            .strong()
                            .color(state_color),
                    );
                    ui.label(
                        RichText::new(self.summary(drive, snapshot.as_ref()))
                            .size(12.5)
                            .color(t.muted),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if widgets::secondary(ui, "S.M.A.R.T.").clicked() {
                            self.tab = 1;
                        }
                        if health.percentage_used.is_some_and(|u| u >= 100.0)
                            && widgets::primary(ui, "Back up this drive").clicked()
                        {
                            self.tab = 3;
                            if let Some(volume) = drive.volumes.first() {
                                self.source = Some(PathBuf::from(&volume.mount));
                            }
                        }
                    });
                });
            });
        });
        ui.add_space(4.0);
        ui.columns(3, |c| {
            widgets::panel(&mut c[0], "Capacity", "", |ui| {
                ui.set_min_height(196.0);
                match drive
                    .volumes
                    .first()
                    .and_then(|v| v.free.map(|f| (v.capacity, f)))
                {
                    Some((capacity, free)) => {
                        let used = capacity.saturating_sub(free);
                        egui::Grid::new("capacity")
                            .spacing([24.0, 8.0])
                            .show(ui, |ui| {
                                for (key, value) in
                                    [("Available", free), ("Used", used), ("Total", capacity)]
                                {
                                    ui.label(RichText::new(tr(key)).color(t.muted));
                                    ui.label(
                                        RichText::new(disks::bytes(value)).strong().color(t.text),
                                    );
                                    ui.end_row();
                                }
                            });
                        ui.add_space(8.0);
                        let fraction = used as f32 / capacity.max(1) as f32;
                        ui.horizontal(|ui| {
                            ui.allocate_ui(vec2(ui.available_width() - 44.0, 14.0), |ui| {
                                widgets::bar(ui, fraction, t.accent_alt, None, 12.0);
                            });
                            ui.label(
                                RichText::new(format!("{:.0}%", fraction * 100.0)).color(t.text),
                            );
                        });
                    }
                    None => {
                        ui.label(RichText::new(tr("No mounted volumes.")).color(t.muted));
                    }
                }
            });
            widgets::panel(&mut c[1], "Temperature", "", |ui| {
                ui.set_min_height(196.0);
                ui.vertical_centered(|ui| {
                    match health.temperature_c {
                        Some(v) => widgets::arc_gauge(
                            ui,
                            Some((v / 100.0) as f32),
                            &format!("{v:.0}°C"),
                            "",
                            temperature_color(&t, v),
                            150.0,
                        ),
                        None => widgets::arc_gauge(ui, None, "—", "unavailable", t.muted, 150.0),
                    }
                    if let Some(s) = &snapshot {
                        ui.label(
                            RichText::new(trf("snapshot · {}", &[ago(s.captured_unix)]))
                                .size(11.5)
                                .color(t.muted),
                        );
                    }
                });
            });
            widgets::panel(&mut c[2], "Lifetime", "", |ui| {
                ui.set_min_height(196.0);
                ui.vertical_centered(|ui| {
                    match life {
                        Some(l) => widgets::arc_gauge(
                            ui,
                            Some(l as f32 / 100.0),
                            &format!("{l:.0}%"),
                            "",
                            life_color(&t, l),
                            150.0,
                        ),
                        None => widgets::arc_gauge(ui, None, "—", "unavailable", t.muted, 150.0),
                    }
                    if let Some(used) = health.percentage_used {
                        ui.label(
                            RichText::new(trf(
                                "{}% of rated endurance used.",
                                &[format!("{used:.0}")],
                            ))
                            .size(11.5)
                            .color(t.muted),
                        );
                    }
                });
            });
        });
        ui.add_space(4.0);
        ui.columns(3, |c| {
            widgets::stat(
                &mut c[0],
                "Power-on time",
                &health
                    .power_on_hours
                    .map(|h| format!("{} h", count(h)))
                    .unwrap_or("—".into()),
                &health
                    .power_on_hours
                    .map(|h| trf("≈ {} years", &[format!("{:.1}", h / 8766.0)]))
                    .unwrap_or_default(),
                t.text,
            );
            widgets::stat(
                &mut c[1],
                "Power cycles",
                &health.power_cycles.map(count).unwrap_or("—".into()),
                &raw(health, "Power-Off_Retract_Count")
                    .map(|n| trf("{} unexpected", &[count(n)]))
                    .unwrap_or_default(),
                t.text,
            );
            widgets::stat(
                &mut c[2],
                "Data written",
                &bytes(health.bytes_written),
                &health
                    .bytes_read
                    .map(|n| trf("{} read", &[disks::bytes(n as u64)]))
                    .unwrap_or_default(),
                t.text,
            );
        });
        ui.add_space(4.0);
        let history = self.histories.get(&drive.id);
        widgets::panel(ui, "Live activity", "Hover the chart for details", |ui| {
            ui.horizontal(|ui| chart::legend(ui, history));
            chart::io(ui, history, 200.0);
        });
        ui.add_space(4.0);
        widgets::panel(ui, "Insights", "", |ui| {
            self.insights(ui, drive);
        });
    }

    fn summary(&self, drive: &Drive, snapshot: Option<&Snapshot>) -> String {
        let health = &drive.health;
        match (
            health.percentage_used,
            raw(health, "Average_Erase_Count"),
            raw(health, "Max_Erase_Count_of_Spec"),
        ) {
            (Some(used), Some(avg), Some(spec)) if used >= 100.0 => trf(
                "Flash cells have averaged {} erase cycles against a rating of {} ({}% of rated endurance). Transcend SSD Scope reports this as 0% health. The drive can keep working, but back up anything important.",
                &[count(avg), count(spec), format!("{used:.0}")],
            ),
            (Some(used), Some(avg), Some(spec)) => trf(
                "{} of {} rated erase cycles used ({}%). Calculated the same way as Transcend SSD Scope.",
                &[count(avg), count(spec), format!("{used:.0}")],
            ),
            (Some(used), _, _) => trf("{}% of rated endurance used.", &[format!("{used:.0}")]),
            _ if snapshot.is_none() && !drive.internal && drive.bus == "USB" => tr(
                "macOS cannot read SMART live through this USB bridge. Use Read SMART now on the Health page.",
            ),
            _ => tr("This drive did not report wear information."),
        }
    }

    fn insights(&self, ui: &mut egui::Ui, drive: &Drive) {
        let t = Theme::of_ui(ui);
        let health = &drive.health;
        let mut any = false;
        if health.percentage_used.is_some_and(|u| u >= 100.0) {
            widgets::advisory(
                ui,
                t.bad,
                "Rated endurance exceeded",
                "Keep a current backup and plan a replacement.",
            );
            any = true;
        }
        if let Some(crc) = raw(health, "UDMA_CRC_Error_Count").filter(|n| *n > 0.0) {
            widgets::advisory(
                ui,
                t.warn,
                &trf("{} link CRC errors", &[count(crc)]),
                "Usually the USB enclosure, cable or port, not flash wear. Try another cable or port.",
            );
            any = true;
        }
        if let (Some(written), Some(nand)) = (health.bytes_written, raw(health, "TLC_Writes_32MiB"))
        {
            let factor = nand * 32.0 * 1024.0 * 1024.0 / written.max(1.0);
            if factor > 4.0 {
                widgets::advisory(
                    ui,
                    t.warn,
                    &trf("Write amplification ≈ {}×", &[format!("{factor:.1}")]),
                    "The flash wrote far more than the computer sent, usually from many small random writes on a nearly full drive. Keeping more free space helps.",
                );
                any = true;
            }
        }
        if let Some(t_c) = health.temperature_c.filter(|c| *c >= 70.0) {
            widgets::advisory(
                ui,
                t.bad,
                &trf("Running hot ({} °C)", &[format!("{t_c:.0}")]),
                "Improve airflow around the enclosure.",
            );
            any = true;
        }
        if self.snapshot(&drive.id).is_some() {
            widgets::advisory(
                ui,
                t.accent_alt,
                "Values are from a saved SMART reading",
                "This USB bridge does not expose SMART to macOS while mounted. Use Read SMART now on the Health page to update.",
            );
            any = true;
        }
        if !any {
            ui.label(RichText::new(tr("Nothing needs your attention.")).color(t.muted));
        }
    }

    fn health_page(&mut self, ui: &mut egui::Ui, drive: &Drive, ctx: &egui::Context) {
        let t = Theme::of_ui(ui);
        let health = drive.health.clone();
        let snapshot = self.snapshot(&drive.id).cloned();
        ui.columns(4, |c| {
            let life = health.remaining_life_estimate();
            widgets::stat(
                &mut c[0],
                "Health",
                &life.map(|l| format!("{l:.0}%")).unwrap_or("—".into()),
                "endurance remaining",
                life.map(|l| life_color(&t, l)).unwrap_or(t.muted),
            );
            widgets::stat(
                &mut c[1],
                "Overall SMART",
                &tr(&health.status),
                "self-assessment",
                match health.status.as_str() {
                    "Verified" => t.good,
                    "Failing" => t.bad,
                    _ => t.muted,
                },
            );
            widgets::stat(
                &mut c[2],
                "Spare blocks",
                &raw(&health, "Valid_Spare_Block_Cnt")
                    .map(count)
                    .unwrap_or("—".into()),
                &raw(&health, "Reallocated_Sector_Ct")
                    .map(|n| trf("{} reallocated", &[count(n)]))
                    .unwrap_or_default(),
                t.text,
            );
            widgets::stat(
                &mut c[3],
                "Uncorrectable",
                &raw(&health, "Uncorrectable_Error_Cnt")
                    .or(health.media_errors)
                    .map(count)
                    .unwrap_or("—".into()),
                "media errors",
                t.text,
            );
        });
        ui.add_space(6.0);
        if let (Some(avg), Some(spec)) = (
            raw(&health, "Average_Erase_Count"),
            raw(&health, "Max_Erase_Count_of_Spec"),
        ) {
            widgets::panel(
                ui,
                "Endurance",
                "Average erase count vs. rated cycles",
                |ui| {
                    let scale = (avg / spec).max(1.0) * 1.1;
                    let color = if avg >= spec { t.bad } else { t.accent };
                    widgets::bar(
                        ui,
                        (avg / spec / scale) as f32,
                        color,
                        Some((1.0 / scale) as f32),
                        14.0,
                    );
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(trf("{} average erases", &[count(avg)]))
                                .strong()
                                .color(color),
                        );
                        ui.label(
                            RichText::new(trf(
                                "· rated {} · marker = 100% of rating",
                                &[count(spec)],
                            ))
                            .color(t.muted),
                        );
                    });
                    if let (Some(min), Some(max)) = (
                        raw(&health, "Min_Erase_Count"),
                        raw(&health, "Max_Erase_Count"),
                    ) {
                        ui.label(
                            RichText::new(trf(
                                "Per-block range {} – {} (even wear levelling)",
                                &[count(min), count(max)],
                            ))
                            .size(11.5)
                            .color(t.muted),
                        );
                    }
                    if let Some(nand) = raw(&health, "TLC_Writes_32MiB") {
                        ui.label(
                            RichText::new(trf(
                                "Flash writes {} · host writes {}",
                                &[
                                    disks::bytes((nand * 32.0 * 1024.0 * 1024.0) as u64),
                                    bytes(health.bytes_written),
                                ],
                            ))
                            .size(11.5)
                            .color(t.muted),
                        );
                    }
                },
            );
            ui.add_space(6.0);
        }
        if let Some(snapshot) = &snapshot {
            widgets::panel(
                ui,
                "SMART attributes",
                &trf("Read {}", &[snapshot.captured_at()]),
                |ui| {
                    let (checked, failed) = snapshot.threshold_counts();
                    ui.label(RichText::new(trf("{} attributes have thresholds, {} at or below. Hover a row for its meaning.", &[checked.to_string(), failed.to_string()])).size(12.0).color(t.muted));
                    egui::Grid::new("attributes")
                        .striped(true)
                        .spacing([18.0, 7.0])
                        .min_col_width(40.0)
                        .show(ui, |ui| {
                            for title in ["ID", "Attribute", "Value", "Worst", "Threshold", "Raw"] {
                                ui.label(RichText::new(title).size(11.5).color(t.muted));
                            }
                            ui.end_row();
                            for a in &snapshot.ata_attributes {
                                let note = attribute_note(a.id);
                                let highlight = matches!(a.id, 167 | 168 | 169 | 194 | 199);
                                ui.label(
                                    RichText::new(format!("{:02X}", a.id))
                                        .monospace()
                                        .color(t.muted),
                                )
                                .on_hover_text(note);
                                ui.label(
                                    RichText::new(a.name.replace('_', " ")).color(if highlight {
                                        t.text
                                    } else {
                                        t.muted
                                    }),
                                )
                                .on_hover_text(note);
                                for v in [a.normalized, a.worst, a.threshold] {
                                    ui.label(v.map(|v| v.to_string()).unwrap_or("—".into()));
                                }
                                ui.label(RichText::new(&a.raw).monospace().color(t.text))
                                    .on_hover_text(note);
                                ui.end_row();
                            }
                        });
                    ui.collapsing(tr("Capture notes"), |ui| {
                        for warning in &snapshot.warnings {
                            ui.label(RichText::new(warning).size(12.0).color(t.muted));
                        }
                    });
                },
            );
            ui.add_space(6.0);
        }
        widgets::panel(ui, "SMART source", "", |ui| {
            ui.label(RichText::new(tr("macOS does not pass SMART commands through this USB bridge while the drive is mounted (Windows uses a kernel pass-through that macOS does not offer). Read SMART now briefly unmounts the drive, asks for your administrator password, reads SMART directly over USB, then mounts it again.")).color(t.muted));
            ui.horizontal(|ui| {
                if !drive.internal && drive.bus == "USB" {
                    if widgets::primary(ui, "Read SMART now")
                        .on_hover_text(tr("Close apps and files using the drive first."))
                        .clicked()
                    {
                        let selected = drive.clone();
                        self.start(ctx, &tr("Reading SMART from the drive…"), move |control| {
                            usb_live::read_now(&selected, &control)?;
                            crate::inventory_event(&control)
                        });
                    }
                    if widgets::secondary(ui, "Import capture…").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .set_title(format!("Import capture.json for {}", drive.name))
                            .add_filter("USB capture JSON", &["json"])
                            .pick_file()
                    {
                        let selected = drive.clone();
                        self.start(ctx, "Importing USB SMART snapshot…", move |control| {
                            let snapshot = usb_cache::import(
                                &path,
                                &selected,
                                &usb_cache::default_path(),
                                &control,
                            )?;
                            Ok(Event::UsbSnapshot(selected.id.clone(), Box::new(snapshot)))
                        });
                    }
                    if self.usb_snapshots.contains_key(&drive.id)
                        && widgets::secondary(ui, "Remove saved reading").clicked()
                    {
                        let selected = drive.clone();
                        self.start(ctx, "Removing saved snapshot…", move |_| {
                            let path = usb_cache::default_path();
                            let mut store = usb_cache::Store::load(&path)?;
                            store.remove(&selected);
                            store.save(&path)?;
                            Ok(Event::SnapshotRemoved(selected.id))
                        });
                    }
                }
                if smart::helper_path().is_some()
                    && widgets::secondary(ui, "Check live SMART").clicked()
                {
                    let id = drive.id.clone();
                    self.start(ctx, "Reading SMART…", move |control| {
                        Ok(Event::Smart(Box::new(smart::diagnose(&id, &control)?)))
                    });
                }
            });
            if let Some(report) = self.smart_reports.get(&drive.id) {
                ui.separator();
                if let Some(bridge) = &report.bridge {
                    ui.label(RichText::new(bridge.label()).strong());
                    ui.label(
                        RichText::new(format!("USB {} · {}", bridge.usb_id(), bridge.driver))
                            .color(t.muted),
                    );
                }
                ui.label(
                    RichText::new(&report.explanation).color(if report.health.is_some() {
                        t.good
                    } else {
                        t.warn
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
            }
        });
    }

    fn performance(&mut self, ui: &mut egui::Ui, drive: &Drive, ctx: &egui::Context) {
        let t = Theme::of_ui(ui);
        let history = self.histories.get(&drive.id);
        let (peak_read, peak_write) = chart::peak(history);
        ui.columns(3, |c| {
            let last = history.and_then(|h| h.points.back());
            widgets::stat(
                &mut c[0],
                "Read now",
                &chart::rate(last.map(|s| s.read_mbps).unwrap_or(0.0)),
                &trf("peak {}", &[chart::rate(peak_read)]),
                t.read,
            );
            widgets::stat(
                &mut c[1],
                "Write now",
                &chart::rate(last.map(|s| s.write_mbps).unwrap_or(0.0)),
                &trf("peak {}", &[chart::rate(peak_write)]),
                t.write,
            );
            widgets::stat(
                &mut c[2],
                "Connection",
                &drive.bus,
                if drive.internal {
                    "internal"
                } else {
                    "external"
                },
                t.text,
            );
        });
        ui.add_space(6.0);
        widgets::panel(
            ui,
            "Live activity",
            "IORegistry counters · last 2 minutes",
            |ui| {
                ui.horizontal(|ui| chart::legend(ui, history));
                chart::io(ui, history, 260.0);
            },
        );
        ui.add_space(6.0);
        widgets::panel(ui, "Speed test", "256 MiB sequential", |ui| {
            ui.label(RichText::new(tr("Writes a new temporary file in a folder you choose, reads it back and verifies it, then removes it. The OS cache is bypassed; the drive's own cache still affects results.")).color(t.muted));
            if let Some(result) = &self.benchmark {
                let top = result.write_mbps.max(result.read_mbps).max(1.0);
                for (name, value, color) in [
                    ("Write", result.write_mbps, t.write),
                    ("Read + verify", result.read_mbps, t.read),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(name).color(t.muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{value:.1} MB/s"))
                                    .size(18.0)
                                    .color(color),
                            );
                        });
                    });
                    widgets::bar(ui, (value / top) as f32, color, None, 10.0);
                }
                ui.label(
                    RichText::new(trf("Folder: {}", &[result.directory.display().to_string()]))
                        .size(11.5)
                        .color(t.faint),
                );
            }
            if widgets::primary(ui, "Choose folder and run…").clicked() {
                let mut dialog =
                    rfd::FileDialog::new().set_title("Choose a folder on the drive to test");
                if let Some(volume) = drive.volumes.first() {
                    dialog = dialog.set_directory(&volume.mount);
                }
                if let Some(path) = dialog.pick_folder() {
                    self.confirmation = Some(Confirmation::Benchmark(path));
                }
            }
        });
        ui.add_space(6.0);
        widgets::panel(ui, "File system check", "macOS verifyVolume", |ui| {
            ui.label(RichText::new(tr("Checks the file system structure (not the flash cells). Access may pause briefly. Nothing is repaired.")).color(t.muted));
            ui.horizontal_wrapped(|ui| {
                for volume in &drive.volumes {
                    if widgets::secondary(ui, &trf("Verify {}", &[&volume.name])).clicked() {
                        let volume = volume.clone();
                        self.start(ctx, "Verifying file system…", move |control| {
                            Ok(Event::Text(disks::verify_volume(&volume, &control)?))
                        });
                    }
                }
            });
        });
    }

    fn backup(&mut self, ui: &mut egui::Ui, drive: &Drive) {
        let t = Theme::of_ui(ui);
        let step = |ui: &mut egui::Ui, n: &str, title: &str| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 13.0, t.accent.gamma_multiply(0.16));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    n,
                    egui::FontId::proportional(13.0),
                    t.accent,
                );
                ui.label(RichText::new(title).size(15.0).strong());
            });
        };
        let path_label = |ui: &mut egui::Ui, path: &Option<PathBuf>, empty: &str| {
            ui.label(
                RichText::new(
                    path.as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or(empty.into()),
                )
                .monospace()
                .size(12.0)
                .color(if path.is_some() { t.text } else { t.faint }),
            );
        };
        widgets::card(ui, |ui| {
            ui.label(RichText::new(tr("Copy a folder or a whole external volume into a NEW backup folder. Existing files are never overwritten or merged. Not a bootable clone.")).color(t.muted));
            ui.add_space(6.0);
            step(ui, "1", "Source");
            ui.horizontal(|ui| {
                if widgets::secondary(ui, "Choose folder…").clicked() {
                    self.source = rfd::FileDialog::new().pick_folder().or(self.source.take());
                }
                if let Some(volume) = drive.volumes.first()
                    && !drive.internal
                    && widgets::secondary(ui, &trf("Use {}", &[&volume.name])).clicked()
                {
                    self.source = Some(PathBuf::from(&volume.mount));
                }
            });
            path_label(ui, &self.source, "No source selected");
            ui.separator();
            step(ui, "2", "Destination");
            if widgets::secondary(ui, "Choose destination…").clicked() {
                self.destination = rfd::FileDialog::new()
                    .pick_folder()
                    .or(self.destination.take());
            }
            path_label(ui, &self.destination, "No destination selected");
            ui.separator();
            step(ui, "3", "Verify and start");
            ui.checkbox(
                &mut self.verify,
                tr("Verify every file with SHA-256 after copying"),
            );
            ui.label(RichText::new(tr("Permissions, extended attributes and resource forks are copied by macOS ditto.")).size(11.5).color(t.muted));
            if let (Some(source), Some(destination)) = (&self.source, &self.destination) {
                match operations::validate_backup(source, destination) {
                    Ok(_) => {
                        if widgets::primary(ui, "Review and start backup…").clicked() {
                            self.confirmation = Some(Confirmation::Backup {
                                source: source.clone(),
                                destination: destination.clone(),
                                verify: self.verify,
                            });
                        }
                    }
                    Err(error) => {
                        widgets::advisory(ui, t.warn, "Cannot start", &format!("{error:#}"));
                    }
                }
            }
        });
        ui.add_space(6.0);
        widgets::panel(ui, "Restore from a Scope backup", "", |ui| {
            ui.label(RichText::new(tr("Choose a completed Scope Backup folder; its Files folder becomes the source above. Then pick a new destination.")).color(t.muted));
            if widgets::secondary(ui, "Choose completed backup…").clicked()
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

    fn settings_page(&mut self, ui: &mut egui::Ui) {
        let t = Theme::of_ui(ui);
        let before = self.settings.clone();
        widgets::panel(ui, "Language", "", |ui| {
            widgets::segmented(
                ui,
                &mut self.settings.language,
                &["Automatic", "English", "ไทย"],
            );
        });
        ui.add_space(6.0);
        widgets::panel(ui, "Appearance", "", |ui| {
            widgets::segmented(
                ui,
                &mut self.settings.appearance,
                &["System", "Light", "Dark"],
            );
        });
        ui.add_space(6.0);
        widgets::panel(ui, "Menu bar", "Saved automatically", |ui| {
            ui.checkbox(
                &mut self.settings.menu_bar_enabled,
                tr("Show SSD Scope in the menu bar"),
            );
            ui.add_enabled_ui(self.settings.menu_bar_enabled, |ui| {
                ui.checkbox(
                    &mut self.settings.widget_enabled,
                    tr("Show a value next to the icon"),
                );
                ui.add_enabled_ui(self.settings.widget_enabled, |ui| {
                    widgets::segmented(
                        ui,
                        &mut self.settings.widget_mode,
                        &["Free space", "Read / write", "Temperature"],
                    );
                });
                ui.checkbox(
                    &mut self.settings.close_to_menu_bar,
                    tr("Keep running in the menu bar when the window closes"),
                );
            });
            ui.label(
                RichText::new(tr(
                    "Temperature from a saved SMART reading is labelled with its reading time.",
                ))
                .size(11.5)
                .color(t.muted),
            );
        });
        ui.add_space(6.0);
        widgets::panel(ui, "About", "", |ui| {
            ui.label(RichText::new(format!("SSD Scope for Mac {VERSION}")).strong());
            ui.label(RichText::new(tr("Independent open-source utility (MIT). Not affiliated with or endorsed by Transcend. Health follows the formula used by Transcend SSD Scope for Windows: (rated erase cycles − average erase count) ÷ rated erase cycles.")).color(t.muted));
            ui.hyperlink_to(
                tr("Source code on GitHub"),
                "https://github.com/ChakkritGit/Transcend-SSD-Scope-for-Mac",
            );
        });
        if before.appearance != self.settings.appearance {
            super::theme::set_appearance(ui.ctx(), self.settings.appearance);
        }
        if before.language != self.settings.language {
            super::i18n::set(self.settings.language);
        }
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
            self.last_sample = std::time::Instant::now() - std::time::Duration::from_secs(2);
        }
    }
}

/// The window has no title bar; let the top strip of a panel move the window.
fn window_drag(ui: &egui::Ui, height: f32) {
    let mut rect = ui.max_rect();
    rect.min.y -= height;
    rect.max.y = rect.min.y + height;
    let response = ui.interact(
        rect,
        ui.id().with("window-drag"),
        egui::Sense::click_and_drag(),
    );
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let zoomed = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!zoomed));
    }
}
