//! The settings window (`queijo --settings`, or the gear in the editor).

use std::path::Path;
use std::thread::JoinHandle;

use eframe::egui::{
    self, pos2, vec2, Align, Color32, CornerRadius, Event, FontId, Id, Key, Layout, Margin, ResizeDirection, RichText,
    Sense, Stroke, StrokeKind, Ui, ViewportCommand,
};

use crate::config::{self, Config, ImageFormat, StartArea, StartKind, Target};
use crate::i18n::{self, t, tf};
use crate::paint::{self, ACCENT, PALETTE, TEXT, TEXT_DIM};
use crate::shortcut::{self, Action};

const BG: Color32 = Color32::from_rgb(20, 20, 24);
const CARD: Color32 = Color32::from_rgb(30, 30, 35);
const FIELD: Color32 = Color32::from_rgb(40, 40, 47);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Shortcuts,
    Files,
    Recording,
    Editor,
}

impl Section {
    const ALL: [Section; 5] =
        [Section::General, Section::Shortcuts, Section::Files, Section::Recording, Section::Editor];

    fn title(self) -> &'static str {
        match self {
            Section::General => t("settings.general"),
            Section::Shortcuts => t("settings.shortcuts"),
            Section::Files => t("settings.files"),
            Section::Recording => t("settings.recording"),
            Section::Editor => t("settings.editor"),
        }
    }

    fn icon(self) -> paint::Icon {
        match self {
            Section::General => paint::Icon::Selection,
            Section::Shortcuts => paint::Icon::Keyboard,
            Section::Files => paint::Icon::Folder,
            Section::Recording => paint::Icon::Video,
            Section::Editor => paint::Icon::Pen,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Folder {
    Screenshots,
    Videos,
    Gifs,
}

struct SettingsApp {
    logo: egui::TextureHandle,
    cfg: Config,
    saved: Config,
    section: Section,
    bindings: Vec<(Action, Vec<String>)>,
    /// (shortcut, who else uses it, taken over automatically)
    conflicts: Vec<(String, String, bool)>,
    /// Waiting for a key combination for (action, slot).
    capturing: Option<(Action, usize)>,
    browsing: Option<(Folder, JoinHandle<Option<String>>)>,
    status: Option<(String, f64, bool)>,
    reset_armed: Option<f64>,
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t("settings.window_title"))
            .with_app_id("queijo-settings")
            .with_icon(crate::icon::window_icon())
            .with_inner_size([860.0, 640.0])
            .with_min_inner_size([680.0, 480.0])
            // GNOME leaves decorations to the app and winit's fallback frame is a bare
            // grey bar, so we draw our own (see `title_bar`).
            .with_decorations(false),
        ..Default::default()
    };
    eframe::run_native(
        "queijo-settings",
        options,
        Box::new(|cc| {
            crate::app::setup_style(&cc.egui_ctx);
            style(&cc.egui_ctx);
            let bindings = shortcut::current();
            let mut app = SettingsApp {
                logo: crate::icon::texture(&cc.egui_ctx, 96),
                cfg: Config::load(),
                saved: Config::load(),
                section: Section::General,
                bindings,
                conflicts: Vec::new(),
                capturing: None,
                browsing: None,
                status: None,
                reset_armed: None,
            };
            app.refresh_conflicts();
            Ok(Box::new(app))
        }),
    )
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = FIELD;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(8);
    }
    v.widgets.inactive.weak_bg_fill = FIELD;
    v.widgets.inactive.bg_fill = FIELD;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(52, 52, 60);
    v.widgets.hovered.bg_fill = Color32::from_rgb(52, 52, 60);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(62, 62, 72);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(10.0, 10.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
    });
}

impl eframe::App for SettingsApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.capture_keys(&ctx);
        self.finish_browse();

        let window = ui.max_rect();
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let edge = if maximized { None } else { ctx.input(|i| i.pointer.hover_pos()).and_then(|p| edge_at(window, p)) };

        egui::Panel::top("titlebar")
            .exact_size(TITLE_H)
            .resizable(false)
            .frame(egui::Frame::new().fill(TITLE_BG))
            .show(ui, |ui| title_bar(ui, maximized, edge.is_some()));

        egui::Panel::left("nav")
            .exact_size(200.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(Color32::from_rgb(16, 16, 19)).inner_margin(Margin::same(16)))
            .show(ui, |ui| self.nav(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 28, right: 28, top: 22, bottom: 16 }))
            .show(ui, |ui| {
                let width = (ui.available_width() - 16.0).min(680.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(width);
                    ui.label(RichText::new(self.section.title()).size(22.0).color(TEXT));
                    ui.add_space(6.0);
                    match self.section {
                        Section::General => self.general(ui),
                        Section::Shortcuts => self.shortcuts(ui, now),
                        Section::Files => self.files(ui),
                        Section::Recording => self.recording(ui),
                        Section::Editor => self.editor(ui, now),
                    }
                    ui.add_space(20.0);
                });
            });

        // Changes apply as you make them.
        if self.cfg != self.saved {
            match self.cfg.save() {
                Ok(()) => self.status = Some((t("settings.saved").into(), now, true)),
                Err(e) => self.status = Some((tf("settings.save_failed", &[("error", &e)]), now, false)),
            }
            self.saved = self.cfg.clone();
        }
        self.paint_status(&ctx, now);

        // Undecorated windows get no resize border from the compositor.
        if let Some(dir) = edge {
            ctx.set_cursor_icon(resize_cursor(dir));
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
    }
}

impl SettingsApp {
    fn nav(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            ui.painter().image(
                self.logo.id(),
                r,
                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.label(RichText::new("Queijo").size(19.0).color(TEXT));
        });
        ui.add_space(14.0);
        for section in Section::ALL {
            let selected = self.section == section;
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
            let h =
                ui.ctx().animate_bool_with_time(Id::unique(("nav", section.title())), resp.hovered() || selected, 0.12);
            if h > 0.0 {
                let fill =
                    if selected { ACCENT.gamma_multiply(0.16) } else { Color32::from_white_alpha((14.0 * h) as u8) };
                ui.painter().rect_filled(r, 8, fill);
            }
            let color = if selected { ACCENT } else { paint::lerp(TEXT_DIM, TEXT, h) };
            paint::icon(ui.painter(), section.icon(), pos2(r.left() + 20.0, r.center().y), color);
            ui.painter().text(
                pos2(r.left() + 40.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                section.title(),
                FontId::proportional(14.0),
                color,
            );
            if resp.clicked() {
                self.section = section;
                self.capturing = None;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).size(11.0).color(TEXT_DIM));
        });
    }

    // ---------------------------------------------------------------------

    fn general(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            row(ui, t("settings.start_in"), t("settings.start_in_desc"), |ui| {
                segmented(
                    ui,
                    &mut self.cfg.start_area,
                    &[
                        (StartArea::Window, t("mode.window")),
                        (StartArea::Screen, t("mode.screen")),
                        (StartArea::Selection, t("mode.selection")),
                    ],
                );
            });
            sep(ui);
            row(ui, t("settings.start_capture"), t("settings.start_capture_desc"), |ui| {
                segmented(
                    ui,
                    &mut self.cfg.start_kind,
                    &[
                        (StartKind::Gif, t("settings.kind_gif")),
                        (StartKind::Video, t("settings.kind_video")),
                        (StartKind::Screenshot, t("settings.kind_screenshot")),
                    ],
                );
            });
            sep(ui);
            row(ui, t("settings.pointer"), t("settings.pointer_desc"), |ui| {
                toggle(ui, &mut self.cfg.show_pointer);
            });
        });
        card(ui, |ui| {
            row(ui, t("settings.copy_saves"), t("settings.copy_saves_desc"), |ui| {
                toggle(ui, &mut self.cfg.copy_also_saves);
            });
            sep(ui);
            row(ui, t("settings.save_copies"), t("settings.save_copies_desc"), |ui| {
                toggle(ui, &mut self.cfg.save_also_copies);
            });
            sep(ui);
            row(ui, t("settings.notifications"), t("settings.notifications_desc"), |ui| {
                toggle(ui, &mut self.cfg.notifications);
            });
            sep(ui);
            row(ui, t("settings.language"), t("settings.language_desc"), |ui| {
                self.language_picker(ui);
            });
        });
    }

    fn language_picker(&mut self, ui: &mut Ui) {
        let label = match i18n::LANGUAGES.iter().find(|l| l.code == self.cfg.language) {
            Some(language) => language.name,
            None => t("settings.language_auto"),
        };
        let before = self.cfg.language.clone();
        egui::ComboBox::from_id_salt("language").selected_text(label).width(190.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut self.cfg.language, i18n::AUTO.to_string(), t("settings.language_auto"));
            for language in i18n::LANGUAGES {
                ui.selectable_value(&mut self.cfg.language, language.code.to_string(), language.name);
            }
        });
        if self.cfg.language != before {
            i18n::set_language(&self.cfg.language);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(t("settings.window_title").into()));
        }
    }

    fn shortcuts(&mut self, ui: &mut Ui, now: f64) {
        hint(ui, t("settings.shortcuts_hint"));
        card(ui, |ui| {
            for (n, action) in Action::ALL.into_iter().enumerate() {
                if n > 0 {
                    sep(ui);
                }
                let keys = self.bindings.iter().find(|(a, _)| *a == action).map(|(_, k)| k.clone()).unwrap_or_default();
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(action.title()).size(14.0).color(TEXT));
                ui.label(RichText::new(action.description()).size(12.0).color(TEXT_DIM));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for slot in 0..2 {
                        let key = keys.get(slot).cloned();
                        let capturing = self.capturing == Some((action, slot));
                        if keycap_slot(ui, key.as_deref(), capturing, now).clicked() {
                            self.capturing = if capturing { None } else { Some((action, slot)) };
                        }
                    }
                    ui.menu_button(RichText::new(t("settings.presets")).size(13.0).color(TEXT_DIM), |ui| {
                        for (label, accel) in [
                            ("Print Screen", "Print"),
                            ("Shift+Print Screen", "<Shift>Print"),
                            ("Ctrl+Print Screen", "<Primary>Print"),
                            ("Alt+Print Screen", "<Alt>Print"),
                            ("Super+Shift+S", "<Super><Shift>s"),
                        ] {
                            if ui.button(label).clicked() {
                                self.set_binding(action, keys.len().min(1), Some(accel.to_string()), now);
                                ui.close();
                            }
                        }
                    });
                });
                ui.spacing_mut().item_spacing.y = 10.0;
                ui.add_space(4.0);
                for key in &keys {
                    if let Some((_, who, auto)) = self.conflicts.iter().find(|(k, _, _)| k == key) {
                        let text = if *auto {
                            tf("settings.took_over", &[("key", &shortcut::pretty(key)), ("who", who)])
                        } else {
                            tf("settings.conflict", &[("key", &shortcut::pretty(key)), ("who", who)])
                        };
                        ui.label(RichText::new(text).size(12.0).color(if *auto { TEXT_DIM } else { ACCENT }));
                    }
                }
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(t("settings.inside_editor")).size(15.0).color(TEXT));
        card(ui, |ui| {
            let rows = [
                (t("settings.keys_copy_save_close"), "Ctrl+C · Ctrl+S · Esc"),
                (t("settings.keys_full_screen"), "Ctrl+A"),
                (t("settings.keys_modes"), "S · C · W"),
                (t("settings.keys_kinds"), "V · G · P"),
                (t("settings.keys_tools"), "P L A R E M T B"),
                (t("settings.keys_undo_redo"), "Ctrl+Z · Ctrl+Shift+Z"),
                (t("settings.keys_tool_size"), t("settings.keys_tool_size_value")),
                (t("settings.keys_move"), t("settings.keys_move_value")),
            ];
            for (n, (what, keys)) in rows.into_iter().enumerate() {
                if n > 0 {
                    sep(ui);
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(what).color(TEXT_DIM));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(keys).monospace().color(TEXT));
                    });
                });
            }
        });
    }

    fn files(&mut self, ui: &mut Ui) {
        let tokens = "{date} {time} {year} {month} {day} {hour} {minute} {second} {width} {height} {timestamp}";
        hint(ui, &tf("settings.placeholders", &[("tokens", tokens)]));
        let ext = self.cfg.image_format.extension();
        for (folder, title, target, ext) in [
            (Folder::Screenshots, t("settings.screenshots"), self.cfg.screenshots.clone(), ext),
            (Folder::Videos, t("settings.videos"), self.cfg.videos.clone(), "webm"),
            (Folder::Gifs, t("settings.gifs"), self.cfg.gifs.clone(), "gif"),
        ] {
            let mut target = target;
            ui.label(RichText::new(title).size(15.0).color(TEXT));
            card(ui, |ui| {
                ui.label(RichText::new(t("settings.folder")).color(TEXT_DIM));
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 100.0;
                    ui.add(egui::TextEdit::singleline(&mut target.folder).desired_width(w).margin(vec2(8.0, 6.0)));
                    let busy = self.browsing.is_some();
                    if ui.add_enabled(!busy, egui::Button::new(t("settings.browse"))).clicked() {
                        let start = target.dir();
                        self.browsing = Some((folder, std::thread::spawn(move || pick_folder(&start))));
                    }
                });
                ui.label(RichText::new(t("settings.file_name")).color(TEXT_DIM));
                ui.add(
                    egui::TextEdit::singleline(&mut target.name)
                        .desired_width(ui.available_width())
                        .margin(vec2(8.0, 6.0)),
                );
                let example = format!("{}.{ext}", config::render(&target.name, 1280, 720));
                let path = Target { folder: target.folder.clone(), name: target.name.clone() }.dir().join(example);
                ui.label(
                    RichText::new(tf("settings.saved_as", &[("path", &crate::output::tilde(&path))]))
                        .size(12.0)
                        .color(TEXT_DIM),
                );
                if folder == Folder::Screenshots {
                    sep(ui);
                    row(ui, t("settings.format"), t("settings.format_desc"), |ui| {
                        segmented(
                            ui,
                            &mut self.cfg.image_format,
                            &[(ImageFormat::Jpeg, "JPEG"), (ImageFormat::Png, "PNG")],
                        );
                    });
                    if self.cfg.image_format == ImageFormat::Jpeg {
                        row(ui, t("settings.jpeg_quality"), "", |ui| {
                            ui.add(egui::Slider::new(&mut self.cfg.jpeg_quality, 50..=100));
                        });
                    }
                }
            });
            match folder {
                Folder::Screenshots => self.cfg.screenshots = target,
                Folder::Videos => self.cfg.videos = target,
                Folder::Gifs => self.cfg.gifs = target,
            }
        }
    }

    fn recording(&mut self, ui: &mut Ui) {
        card(ui, |ui| {
            row(ui, t("settings.countdown"), t("settings.countdown_desc"), |ui| {
                ui.add(egui::Slider::new(&mut self.cfg.countdown, 0..=10).suffix(" s"));
            });
            sep(ui);
            row(ui, t("settings.video_fps"), t("settings.video_fps_desc"), |ui| {
                segmented(ui, &mut self.cfg.video_fps, &[(60, "60"), (30, "30"), (24, "24")]);
            });
        });
        card(ui, |ui| {
            row(ui, t("settings.gif_fps"), t("settings.gif_fps_desc"), |ui| {
                segmented(ui, &mut self.cfg.gif_fps, &[(50, "50"), (25, "25"), (20, "20"), (10, "10")]);
            });
            sep(ui);
            row(ui, t("settings.gif_width"), t("settings.gif_width_desc"), |ui| {
                segmented(
                    ui,
                    &mut self.cfg.gif_max_width,
                    &[(640, "640"), (960, "960"), (1280, "1280"), (0, t("settings.original"))],
                );
            });
        });
        hint(ui, t("settings.recording_note"));
    }

    fn editor(&mut self, ui: &mut Ui, now: f64) {
        card(ui, |ui| {
            row(ui, t("settings.remember"), t("settings.remember_desc"), |ui| {
                toggle(ui, &mut self.cfg.remember_tools);
            });
            sep(ui);
            row(ui, t("settings.color"), t("settings.color_desc"), |ui| {
                for c in PALETTE.iter().rev() {
                    let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                    let selected = self.cfg.color == [c.r(), c.g(), c.b()];
                    if selected {
                        ui.painter().circle_stroke(r.center(), 12.0, Stroke::new(2.0, Color32::WHITE));
                    }
                    let grow = if resp.hovered() { 1.5 } else { 0.0 };
                    ui.painter().circle_filled(r.center(), 8.5 + grow, *c);
                    ui.painter().circle_stroke(r.center(), 8.5 + grow, Stroke::new(1.0, Color32::from_white_alpha(40)));
                    if resp.clicked() {
                        self.cfg.color = [c.r(), c.g(), c.b()];
                    }
                }
            });
        });
        card(ui, |ui| {
            row(ui, t("settings.reset"), t("settings.reset_desc"), |ui| {
                let armed = self.reset_armed.is_some_and(|t| now - t < 3.0);
                let label = if armed { t("settings.reset_confirm") } else { t("settings.reset_button") };
                if ui.button(label).clicked() {
                    if armed {
                        self.cfg = Config::default();
                        self.reset_armed = None;
                    } else {
                        self.reset_armed = Some(now);
                    }
                }
            });
        });
        ui.label(
            RichText::new(tf("settings.file_location", &[("path", &crate::output::tilde(&Config::path()))]))
                .size(12.0)
                .color(TEXT_DIM),
        );
    }

    // ---------------------------------------------------------------------

    fn set_binding(&mut self, action: Action, slot: usize, key: Option<String>, now: f64) {
        if let Some((_, keys)) = self.bindings.iter_mut().find(|(a, _)| *a == action) {
            match key {
                Some(k) if slot < keys.len() => keys[slot] = k,
                Some(k) => keys.push(k),
                None if slot < keys.len() => {
                    keys.remove(slot);
                }
                None => {}
            }
            keys.dedup();
        }
        // A key belongs to one action only.
        if let Some(k) = self.bindings.iter().find(|(a, _)| *a == action).and_then(|(_, keys)| keys.get(slot).cloned())
        {
            for (a, keys) in &mut self.bindings {
                if *a != action {
                    keys.retain(|x| x != &k);
                }
            }
        }
        self.refresh_conflicts();
        match shortcut::apply(&self.bindings) {
            Ok(()) => self.status = Some((t("settings.shortcut_saved").into(), now, true)),
            Err(e) => self.status = Some((tf("settings.shortcut_failed", &[("error", &e)]), now, false)),
        }
        self.bindings = shortcut::current();
    }

    fn refresh_conflicts(&mut self) {
        self.conflicts = self
            .bindings
            .iter()
            .flat_map(|(_, keys)| keys.iter())
            .filter_map(|k| shortcut::conflict(k).map(|(who, auto)| (k.clone(), who, auto)))
            .collect();
    }

    fn capture_keys(&mut self, ctx: &egui::Context) {
        let Some((action, slot)) = self.capturing else { return };
        let now = ctx.input(|i| i.time);
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let Event::Key { key, pressed: true, modifiers, .. } = event else { continue };
            match key {
                Key::Escape => self.capturing = None,
                Key::Backspace | Key::Delete if !modifiers.any() => {
                    self.capturing = None;
                    self.set_binding(action, slot, None, now);
                }
                _ => {
                    let Some(name) = keysym(key) else { continue };
                    let plain = !(modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd);
                    // Bare letters would fire while typing anywhere.
                    if plain && name.chars().count() == 1 {
                        self.status = Some((t("settings.need_modifier").into(), now, false));
                        continue;
                    }
                    let mut accel = String::new();
                    if modifiers.ctrl || modifiers.command {
                        accel.push_str("<Primary>");
                    }
                    if modifiers.alt {
                        accel.push_str("<Alt>");
                    }
                    if modifiers.shift {
                        accel.push_str("<Shift>");
                    }
                    accel.push_str(&name);
                    self.capturing = None;
                    self.set_binding(action, slot, Some(accel), now);
                }
            }
        }
        ctx.request_repaint();
    }

    fn finish_browse(&mut self) {
        if !self.browsing.as_ref().is_some_and(|(_, h)| h.is_finished()) {
            return;
        }
        let (folder, handle) = self.browsing.take().unwrap();
        if let Ok(Some(path)) = handle.join() {
            let path = crate::output::tilde(Path::new(&path));
            match folder {
                Folder::Screenshots => self.cfg.screenshots.folder = path,
                Folder::Videos => self.cfg.videos.folder = path,
                Folder::Gifs => self.cfg.gifs.folder = path,
            }
        }
    }

    fn paint_status(&mut self, ctx: &egui::Context, now: f64) {
        let Some((text, since, ok)) = &self.status else { return };
        let age = (now - since) as f32;
        let duration = if *ok { 1.6 } else { 4.0 };
        if age > duration {
            self.status = None;
            return;
        }
        ctx.request_repaint();
        let a = paint::ease_out(age / 0.15).min(1.0 - paint::ease_out((age - duration + 0.3) / 0.3));
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("status")));
        let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(13.0), TEXT.gamma_multiply(a));
        let screen = ctx.content_rect();
        let size = galley.size() + vec2(36.0, 16.0);
        let r = egui::Rect::from_min_size(
            pos2(screen.right() - size.x - 20.0, screen.bottom() - size.y - 18.0 + (1.0 - a) * 8.0),
            size,
        );
        paint::panel(&painter, r, 10, a);
        let dot = if *ok { ACCENT } else { Color32::from_rgb(255, 69, 58) };
        painter.circle_filled(pos2(r.left() + 14.0, r.center().y), 3.5, dot.gamma_multiply(a));
        painter.galley(pos2(r.left() + 26.0, r.center().y - galley.size().y / 2.0), galley, TEXT);
    }
}

// -------------------------------------------------------------------------
// Window frame

const TITLE_H: f32 = 40.0;
const TITLE_BG: Color32 = Color32::from_rgb(16, 16, 19);
/// How close to the window edge the pointer must be to resize it.
const EDGE: f32 = 6.0;
const CORNER: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum TitleButton {
    Minimize,
    Maximize,
    Close,
}

/// Drag to move, double-click to maximize, and GNOME-style round buttons on the right.
fn title_bar(ui: &mut Ui, maximized: bool, on_edge: bool) {
    let ctx = ui.ctx().clone();
    let rect = ui.max_rect();
    let bar = ui.interact(rect, Id::unique("titlebar"), Sense::click_and_drag());
    if bar.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if bar.drag_started_by(egui::PointerButton::Primary) && !on_edge {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
    let painter = ui.painter();
    painter.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, Color32::from_white_alpha(14)));
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        t("settings.window_title"),
        FontId::proportional(13.5),
        if focused { TEXT } else { TEXT_DIM },
    );

    const SIZE: f32 = 24.0;
    const SPACING: f32 = 12.0;
    let mut x = rect.right() - 10.0 - SIZE / 2.0;
    for button in [TitleButton::Close, TitleButton::Maximize, TitleButton::Minimize] {
        let r = egui::Rect::from_center_size(pos2(x, rect.center().y), vec2(SIZE, SIZE));
        x -= SIZE + SPACING;
        let resp = ui.interact(r.expand(SPACING / 2.0), Id::unique(("titlebar-button", button as u8)), Sense::click());
        let h = ctx.animate_bool_with_time(resp.id, resp.hovered(), 0.1);
        let fill = if button == TitleButton::Close && h > 0.0 {
            paint::lerp(Color32::from_white_alpha(20), Color32::from_rgb(255, 69, 58).gamma_multiply(0.8), h)
        } else {
            Color32::from_white_alpha((20.0 + 18.0 * h) as u8)
        };
        let painter = ui.painter();
        painter.circle_filled(r.center(), SIZE / 2.0, fill);
        let ink = Stroke::new(1.4, if focused || h > 0.0 { TEXT } else { TEXT_DIM });
        let c = r.center();
        match button {
            TitleButton::Minimize => {
                painter.line_segment([c + vec2(-4.0, 3.5), c + vec2(4.0, 3.5)], ink);
            }
            TitleButton::Maximize if maximized => {
                // Restore: two overlapping squares.
                painter.rect_stroke(
                    egui::Rect::from_min_size(c + vec2(-4.5, -2.5), vec2(7.0, 7.0)),
                    1,
                    ink,
                    StrokeKind::Middle,
                );
                painter.line_segment([c + vec2(-2.0, -4.5), c + vec2(4.5, -4.5)], ink);
                painter.line_segment([c + vec2(4.5, -4.5), c + vec2(4.5, 2.0)], ink);
            }
            TitleButton::Maximize => {
                painter.rect_stroke(egui::Rect::from_center_size(c, vec2(8.0, 8.0)), 1, ink, StrokeKind::Middle);
            }
            TitleButton::Close => {
                painter.line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], ink);
                painter.line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], ink);
            }
        }
        if resp.clicked() {
            ctx.send_viewport_cmd(match button {
                TitleButton::Minimize => ViewportCommand::Minimized(true),
                TitleButton::Maximize => ViewportCommand::Maximized(!maximized),
                TitleButton::Close => ViewportCommand::Close,
            });
        }
    }
}

/// Which way the window resizes when grabbed at `p`, if `p` is on its border.
fn edge_at(window: egui::Rect, p: egui::Pos2) -> Option<ResizeDirection> {
    if !window.contains(p) {
        return None;
    }
    let near = |d: f32, reach: f32| d <= reach;
    let (l, r) = (p.x - window.left(), window.right() - p.x);
    let (t, b) = (p.y - window.top(), window.bottom() - p.y);
    let corner = |a: f32, b: f32| near(a, CORNER) && near(b, CORNER) && (near(a, EDGE) || near(b, EDGE));
    Some(match () {
        _ if corner(t, l) => ResizeDirection::NorthWest,
        _ if corner(t, r) => ResizeDirection::NorthEast,
        _ if corner(b, l) => ResizeDirection::SouthWest,
        _ if corner(b, r) => ResizeDirection::SouthEast,
        _ if near(t, EDGE) => ResizeDirection::North,
        _ if near(b, EDGE) => ResizeDirection::South,
        _ if near(l, EDGE) => ResizeDirection::West,
        _ if near(r, EDGE) => ResizeDirection::East,
        _ => return None,
    })
}

fn resize_cursor(dir: ResizeDirection) -> egui::CursorIcon {
    use egui::CursorIcon as C;
    match dir {
        ResizeDirection::North => C::ResizeNorth,
        ResizeDirection::South => C::ResizeSouth,
        ResizeDirection::East => C::ResizeEast,
        ResizeDirection::West => C::ResizeWest,
        ResizeDirection::NorthEast => C::ResizeNorthEast,
        ResizeDirection::NorthWest => C::ResizeNorthWest,
        ResizeDirection::SouthEast => C::ResizeSouthEast,
        ResizeDirection::SouthWest => C::ResizeSouthWest,
    }
}

// -------------------------------------------------------------------------
// Widgets

fn card(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(CARD).corner_radius(12).inner_margin(Margin::symmetric(18, 14)).show(ui, |ui| {
        // The frame's margins come on top of whatever width the content takes.
        ui.set_width(ui.available_width() - 36.0);
        content(ui);
    });
    ui.add_space(6.0);
}

/// Room kept for the control on the right of a settings row.
const CONTROL_RESERVE: f32 = 260.0;

fn row(ui: &mut Ui, title: &str, description: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        // The text wraps instead of running under the control, whatever the language.
        let text_width = (ui.available_width() - CONTROL_RESERVE).max(160.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_width);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(RichText::new(title).size(14.0).color(TEXT));
            if !description.is_empty() {
                ui.label(RichText::new(description).size(12.0).color(TEXT_DIM));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            control(ui);
        });
    });
}

fn sep(ui: &mut Ui) {
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.top() + 1.0, Stroke::new(1.0, Color32::from_white_alpha(14)));
    ui.add_space(4.0);
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(TEXT_DIM));
    ui.add_space(2.0);
}

/// iOS-style switch with a sliding knob.
fn toggle(ui: &mut Ui, on: &mut bool) -> egui::Response {
    let (r, mut resp) = ui.allocate_exact_size(vec2(42.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.16);
    let track = paint::lerp(Color32::from_rgb(64, 64, 72), ACCENT, t);
    ui.painter().rect_filled(r, 12, track);
    let x = egui::lerp(r.left() + 12.0..=r.right() - 12.0, t);
    ui.painter().circle_filled(pos2(x, r.center().y + 0.5), 9.5, Color32::from_black_alpha(50));
    ui.painter().circle_filled(pos2(x, r.center().y), 9.0, Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A row of options with a highlight under the chosen one (laid out right to left).
fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    egui::Frame::new().fill(FIELD).corner_radius(9).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (option, label) in options {
            let selected = *value == *option;
            let text =
                RichText::new(*label).size(13.0).color(if selected { Color32::from_rgb(25, 25, 28) } else { TEXT });
            let button = egui::Button::new(text)
                .fill(if selected { ACCENT } else { Color32::TRANSPARENT })
                .stroke(Stroke::NONE)
                .corner_radius(7)
                .min_size(vec2(0.0, 26.0));
            if ui.add(button).clicked() {
                *value = *option;
            }
        }
    });
}

fn keycap_slot(ui: &mut Ui, key: Option<&str>, capturing: bool, now: f64) -> egui::Response {
    let text = match (capturing, key) {
        (true, _) => t("settings.press_keys").to_string(),
        (false, Some(k)) => shortcut::pretty(k),
        (false, None) => t("settings.not_set").to_string(),
    };
    let galley = ui.painter().layout_no_wrap(text, FontId::proportional(13.0), TEXT);
    let size = vec2((galley.size().x + 24.0).max(96.0), 30.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let h = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let fill = paint::lerp(FIELD, Color32::from_rgb(54, 54, 62), h);
    ui.painter().rect_filled(r, 8, fill);
    let stroke = if capturing {
        let pulse = (0.6 + 0.4 * ((now * 4.0).sin() as f32)).clamp(0.2, 1.0);
        ui.ctx().request_repaint();
        Stroke::new(1.5, ACCENT.gamma_multiply(pulse))
    } else {
        Stroke::new(1.0, Color32::from_white_alpha(18))
    };
    ui.painter().rect_stroke(r, 8, stroke, StrokeKind::Inside);
    let color = if key.is_none() && !capturing { TEXT_DIM } else { TEXT };
    ui.painter().galley(r.center() - galley.size() / 2.0, galley, color);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn pick_folder(start: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("zenity")
        .args(["--file-selection", "--directory"])
        .arg(format!("--title={}", t("settings.choose_folder")))
        .arg(format!("--filename={}/", start.display()))
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !path.is_empty()).then_some(path)
}

/// egui key → GNOME keysym name.
fn keysym(key: Key) -> Option<String> {
    let name = key.name();
    Some(match key {
        Key::Space => "space".into(),
        Key::Enter => "Return".into(),
        Key::Tab => "Tab".into(),
        Key::ArrowLeft => "Left".into(),
        Key::ArrowRight => "Right".into(),
        Key::ArrowUp => "Up".into(),
        Key::ArrowDown => "Down".into(),
        Key::PageUp => "Page_Up".into(),
        Key::PageDown => "Page_Down".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::Insert => "Insert".into(),
        Key::Delete => "Delete".into(),
        Key::Backspace => "BackSpace".into(),
        Key::Minus => "minus".into(),
        Key::Equals => "equal".into(),
        Key::Comma => "comma".into(),
        Key::Period => "period".into(),
        Key::Slash => "slash".into(),
        Key::Semicolon => "semicolon".into(),
        Key::Escape => return None,
        _ if name.len() == 1 => name.to_lowercase(),
        _ if name.starts_with('F') && name[1..].parse::<u8>().is_ok() => name.to_string(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_become_gnome_names() {
        let cases = [
            (Key::A, "a"),
            (Key::Z, "z"),
            (Key::Num5, "5"),
            (Key::F1, "F1"),
            (Key::F12, "F12"),
            (Key::Space, "space"),
            (Key::Enter, "Return"),
            (Key::Tab, "Tab"),
            (Key::ArrowLeft, "Left"),
            (Key::ArrowDown, "Down"),
            (Key::PageUp, "Page_Up"),
            (Key::Delete, "Delete"),
            (Key::Backspace, "BackSpace"),
            (Key::Minus, "minus"),
            (Key::Equals, "equal"),
            (Key::Slash, "slash"),
        ];
        for (key, name) in cases {
            assert_eq!(keysym(key).as_deref(), Some(name), "{key:?}");
        }
    }

    #[test]
    fn unusable_keys_are_refused() {
        assert_eq!(keysym(Key::Escape), None, "Escape cancels the capture");
        assert_eq!(keysym(Key::Copy), None);
    }

    #[test]
    fn every_section_has_a_title_and_an_icon() {
        for section in Section::ALL {
            assert!(!section.title().is_empty());
            let _ = section.icon();
        }
        let titles: std::collections::HashSet<_> = Section::ALL.iter().map(|s| s.title()).collect();
        assert_eq!(titles.len(), Section::ALL.len());
    }

    #[test]
    fn the_window_border_resizes_the_right_way() {
        let w = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        assert_eq!(edge_at(w, pos2(2.0, 2.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(edge_at(w, pos2(798.0, 10.0)), Some(ResizeDirection::NorthEast), "corners reach along the edge");
        assert_eq!(edge_at(w, pos2(400.0, 598.0)), Some(ResizeDirection::South));
        assert_eq!(edge_at(w, pos2(1.0, 300.0)), Some(ResizeDirection::West));
        assert_eq!(edge_at(w, pos2(10.0, 10.0)), None, "inside the corner zone but off both edges");
        assert_eq!(edge_at(w, pos2(400.0, 300.0)), None);
        assert_eq!(edge_at(w, pos2(-5.0, 300.0)), None);
    }
}
