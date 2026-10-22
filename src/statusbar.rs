use crate::native_menu;
use anyhow::Context;
use muda::{ContextMenu, Menu, MenuItem, PredefinedMenuItem};
use objc2::{AllocAnyThread, MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSCellImagePosition, NSImage, NSMenu, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSData, NSSize, NSString};
use ssd_scope_mac::{
    disks::{self, Drive},
    icons,
};

pub struct StatusBar {
    item: Retained<NSStatusItem>,
    _menu: Menu,
    main_thread: MainThreadMarker,
    drive: MenuItem,
    capacity: MenuItem,
    health: MenuItem,
}
impl StatusBar {
    pub fn new(widget_enabled: bool) -> anyhow::Result<Self> {
        let main_thread =
            MainThreadMarker::new().context("Menu bar must be created on the main thread")?;
        let menu = Menu::new();
        let drive = MenuItem::new("SSD Scope", false, None);
        let capacity = MenuItem::new("Reading drives…", false, None);
        let health = MenuItem::new("SMART: waiting", false, None);
        let show = MenuItem::with_id(native_menu::SHOW, "Open dashboard", true, None);
        let settings = MenuItem::with_id(native_menu::SETTINGS, "Settings…", true, None);
        let refresh = MenuItem::with_id(native_menu::REFRESH, "Refresh drives", true, None);
        let quit = MenuItem::with_id(native_menu::QUIT, "Quit SSD Scope", true, None);
        menu.append_items(&[
            &drive,
            &capacity,
            &health,
            &PredefinedMenuItem::separator(),
            &show,
            &settings,
            &refresh,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::write_buffer_with_format(
            &mut png,
            &icons::rgba(32, true),
            32,
            32,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )?;
        let data = NSData::with_bytes(png.get_ref());
        let icon = NSImage::initWithData(NSImage::alloc(), &data)
            .context("Could not create menu bar icon")?;
        icon.setSize(NSSize::new(18.0, 18.0));
        icon.setTemplate(true);
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        // Keep the menu attached to the actual NSStatusItem. AppKit owns menu
        // tracking, outside clicks, Escape, and clicking the status item again.
        let native_menu = unsafe { &*menu.ns_menu().cast::<NSMenu>() };
        item.setMenu(Some(native_menu));
        let button = item
            .button(main_thread)
            .context("Could not create menu bar button")?;
        button.setImage(Some(&icon));
        button.setImagePosition(NSCellImagePosition::ImageLeft);
        button.setToolTip(Some(&NSString::from_str(
            "SSD Scope · disk activity and health",
        )));
        button.setTitle(&NSString::from_str(if widget_enabled { "SSD" } else { "" }));
        Ok(Self {
            item,
            _menu: menu,
            main_thread,
            drive,
            capacity,
            health,
        })
    }
    pub fn update(
        &self,
        drive: Option<&Drive>,
        read: f64,
        write: f64,
        mode: usize,
        widget_enabled: bool,
        smart_state: Option<&str>,
    ) {
        let Some(drive) = drive else {
            self.set_title(if widget_enabled { "SSD —" } else { "" });
            self.drive.set_text("No connected drive");
            self.capacity.set_text("Capacity unavailable");
            self.health.set_text("SMART: unavailable");
            return;
        };
        let volume = drive.volumes.first();
        let name = volume.map(|v| v.name.as_str()).unwrap_or(&drive.name);
        self.drive.set_text(format!("{name} · {}", drive.name));
        let free = volume.and_then(|v| v.free);
        self.capacity.set_text(
            free.map(|n| {
                format!(
                    "{} free of {}",
                    disks::bytes(n),
                    disks::bytes(drive.capacity)
                )
            })
            .unwrap_or("Capacity unavailable".into()),
        );
        self.health.set_text(format!(
            "SMART: {}",
            smart_state.unwrap_or(&drive.health.status)
        ));
        let title = match mode {
            1 => format!("↓{read:.0} ↑{write:.0}"),
            2 => drive
                .health
                .temperature_c
                .map(|v| format!("{v:.0}°C"))
                .unwrap_or("— °C".into()),
            _ => free
                .map(|v| format!("{:.0} GB", v as f64 / 1e9))
                .unwrap_or("SSD".into()),
        };
        self.set_title(if widget_enabled { &title } else { "" });
    }

    fn set_title(&self, title: &str) {
        if let Some(button) = self.item.button(self.main_thread) {
            button.setTitle(&NSString::from_str(title));
        }
    }
}

impl Drop for StatusBar {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}
