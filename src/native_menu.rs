use eframe::egui;
use muda::{
    AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Modifiers},
};
use std::sync::mpsc::{self, Receiver};

pub const SHOW: &str = "scope.show";
pub const SETTINGS: &str = "scope.settings";
pub const REFRESH: &str = "scope.refresh";
pub const QUIT: &str = "scope.quit";

pub struct NativeMenu {
    _menu: Menu,
    events: Receiver<String>,
}

impl NativeMenu {
    pub fn new(ctx: &egui::Context) -> anyhow::Result<Self> {
        let menu = Menu::new();
        let app = Submenu::new("SSD Scope", true);
        app.append_items(&[
            &PredefinedMenuItem::about(
                Some("About SSD Scope"),
                Some(AboutMetadata {
                    name: Some("SSD Scope for Mac".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    copyright: Some("Independent open source utility · MIT".into()),
                    ..Default::default()
                }),
            ),
            &PredefinedMenuItem::separator(),
            &item(SETTINGS, "Settings…", Some(Code::Comma)),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("Hide SSD Scope")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &item(QUIT, "Quit SSD Scope", Some(Code::KeyQ)),
        ])?;
        let file = Submenu::with_items(
            "File",
            true,
            &[
                &item(REFRESH, "Refresh drives", Some(Code::KeyR)),
                &PredefinedMenuItem::separator(),
                &item("scope.close", "Close window", Some(Code::KeyW)),
            ],
        )?;
        let view = Submenu::with_items(
            "View",
            true,
            &[
                &item("scope.overview", "Overview", Some(Code::Digit1)),
                &item("scope.smart", "S.M.A.R.T.", Some(Code::Digit2)),
                &item("scope.tools", "Tools", Some(Code::Digit3)),
                &item("scope.backup", "File backup", Some(Code::Digit4)),
            ],
        )?;
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(Some("Zoom")),
                &PredefinedMenuItem::separator(),
                &item(SHOW, "Open dashboard", None),
            ],
        )?;
        let help = Submenu::with_items(
            "Help",
            true,
            &[
                &item("scope.help", "USB SMART support", None),
                &item("scope.project", "Project on GitHub", None),
            ],
        )?;
        menu.append_items(&[&app, &file, &view, &window, &help])?;
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        help.set_as_help_menu_for_nsapp();

        // muda installs its handler once. Keep one router for the entire app,
        // including status icons that are later disabled and recreated.
        let (tx, events) = mpsc::sync_channel(32);
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if tx.try_send(event.id.0).is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                ctx.request_repaint();
            }
        }));
        Ok(Self {
            _menu: menu,
            events,
        })
    }

    pub fn next_action(&self) -> Option<String> {
        self.events.try_recv().ok()
    }
}

fn item(id: &str, title: &str, key: Option<Code>) -> MenuItem {
    MenuItem::with_id(
        id,
        title,
        true,
        key.map(|key| Accelerator::new(Modifiers::META, key)),
    )
}
