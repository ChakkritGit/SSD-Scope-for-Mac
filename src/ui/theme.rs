//! Color tokens in the macOS style: a neutral sidebar and canvas, white cards, and the
//! SSD Scope red as the accent colour. Light and dark follow the system unless overridden.
use eframe::egui::{self, Color32};

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    pub background: Color32,
    pub sidebar: Color32,
    pub sidebar_text: Color32,
    pub sidebar_muted: Color32,
    pub sidebar_selected: Color32,
    pub sidebar_selected_text: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_alt: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub read: Color32,
    pub write: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const LIGHT: Theme = Theme {
    dark: false,
    background: rgb(0xf5f5f7),
    sidebar: rgb(0xe9e8ec),
    sidebar_text: rgb(0x1d1d1f),
    sidebar_muted: rgb(0x86868b),
    sidebar_selected: rgb(0xd5d4da),
    sidebar_selected_text: rgb(0x1d1d1f),
    surface: rgb(0xffffff),
    raised: rgb(0xf2f2f5),
    border: rgb(0xe2e2e6),
    text: rgb(0x1d1d1f),
    muted: rgb(0x6e6e73),
    faint: rgb(0xa1a1a6),
    accent: rgb(0xd0191f),
    accent_alt: rgb(0x0a84ff),
    good: rgb(0x28a745),
    warn: rgb(0xe08600),
    bad: rgb(0xe3141b),
    read: rgb(0x0a84ff),
    write: rgb(0xe3141b),
};

pub const DARK: Theme = Theme {
    dark: true,
    background: rgb(0x1e1e20),
    sidebar: rgb(0x2a292d),
    sidebar_text: rgb(0xe8e8ea),
    sidebar_muted: rgb(0x8e8e93),
    sidebar_selected: rgb(0x3f3e44),
    sidebar_selected_text: rgb(0xffffff),
    surface: rgb(0x2a2a2d),
    raised: rgb(0x343438),
    border: rgb(0x3a3a3e),
    text: rgb(0xececee),
    muted: rgb(0xa0a0a5),
    faint: rgb(0x6e6e73),
    accent: rgb(0xff453a),
    accent_alt: rgb(0x409cff),
    good: rgb(0x32d74b),
    warn: rgb(0xffa020),
    bad: rgb(0xff453a),
    read: rgb(0x409cff),
    write: rgb(0xff6961),
};

impl Theme {
    pub fn of(ctx: &egui::Context) -> Self {
        if ctx.style().visuals.dark_mode {
            DARK
        } else {
            LIGHT
        }
    }
    pub fn of_ui(ui: &egui::Ui) -> Self {
        if ui.visuals().dark_mode { DARK } else { LIGHT }
    }
    fn visuals(&self) -> egui::Visuals {
        let mut v = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        v.panel_fill = self.background;
        v.window_fill = self.surface;
        v.window_stroke = egui::Stroke::new(1.0_f32, self.border);
        v.window_corner_radius = 12.into();
        v.extreme_bg_color = self.raised;
        v.faint_bg_color = self.raised;
        v.override_text_color = Some(self.text);
        v.hyperlink_color = self.accent_alt;
        v.selection.bg_fill = self.accent.gamma_multiply(0.25);
        v.selection.stroke = egui::Stroke::new(1.0_f32, self.accent);
        for (state, fill) in [
            (&mut v.widgets.inactive, self.raised),
            (&mut v.widgets.hovered, self.border),
            (&mut v.widgets.active, self.border),
        ] {
            state.weak_bg_fill = fill;
            state.bg_fill = fill;
            state.corner_radius = 7.into();
            state.bg_stroke = egui::Stroke::new(1.0_f32, self.border);
            state.fg_stroke.color = self.text;
        }
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, self.border);
        v.widgets.noninteractive.fg_stroke.color = self.text;
        v
    }
}

/// 0 follows the system, 1 light, 2 dark.
pub fn set_appearance(ctx: &egui::Context, appearance: usize) {
    ctx.set_theme(match appearance {
        1 => egui::ThemePreference::Light,
        2 => egui::ThemePreference::Dark,
        _ => egui::ThemePreference::System,
    });
}

pub fn install(ctx: &egui::Context) {
    ctx.set_visuals_of(egui::Theme::Light, LIGHT.visuals());
    ctx.set_visuals_of(egui::Theme::Dark, DARK.visuals());
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(26.0));
    });
    let mut fonts = egui::FontDefinitions::default();
    // The system San Francisco faces are read at runtime only; nothing is redistributed.
    for (name, path, family) in [
        (
            "sf",
            "/System/Library/Fonts/SFNS.ttf",
            egui::FontFamily::Proportional,
        ),
        (
            "sf-mono",
            "/System/Library/Fonts/SFNSMono.ttf",
            egui::FontFamily::Monospace,
        ),
    ] {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                name.into(),
                std::sync::Arc::new(egui::FontData::from_owned(data)),
            );
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(0, name.into());
            }
        }
    }
    // Thai glyphs come from a system Thai face placed after San Francisco. Sukhumvit Set
    // positions vowel and tone marks with zero advance, which reads correctly without shaping.
    let thai = [
        ("/System/Library/Fonts/Supplemental/SukhumvitSet.ttc", 2),
        ("/System/Library/Fonts/Supplemental/Ayuthaya.ttf", 0),
    ]
    .iter()
    .find_map(|(path, index)| std::fs::read(path).ok().map(|data| (data, *index)));
    if let Some((data, index)) = thai {
        let mut font = egui::FontData::from_owned(data);
        font.index = index;
        fonts
            .font_data
            .insert("thai".into(), std::sync::Arc::new(font));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(1.min(list.len()), "thai".into());
            }
        }
    }
    ctx.set_fonts(fonts);
}
