use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use eframe::egui::{
    self, emath::easing, pos2, vec2, Color32, CursorIcon, Event, FontId, Id, Key, LayerId, Modifiers, Order, Painter,
    Pos2, Rect, Stroke, StrokeKind, TextureHandle, TextureOptions, ViewportBuilder, ViewportCommand, ViewportId,
};
use image::RgbaImage;

use crate::config::{self, Config};
use crate::i18n::t;
use crate::monitors::Monitor;
use crate::output;
use crate::paint::{self, Annotation, Icon, Tool, ACCENT, PALETTE, TEXT, TEXT_DIM};
use crate::record;
use crate::windows::{self, WindowShot};

const BTN: f32 = 36.0;
const PAD: f32 = 5.0;
const SEP: f32 = 9.0;
const GAP: f32 = 10.0;
const MARGIN: f32 = 8.0;
const DIM: f32 = 0.5;
/// Darker backdrop behind the window picker.
const PICKER_DIM: f32 = 0.72;
const FADE_OUT: f64 = 0.15;
/// One second per countdown number.
const COUNTDOWN_STEP: f64 = 1.0;

const RECORD_W: f32 = 128.0;
const MODE_W: f32 = 104.0;

const SWATCH: f32 = 24.0;
const SWATCH_GAP: f32 = 8.0;
const POP_PAD: f32 = 12.0;

pub struct Screen {
    pub viewport: ViewportBuilder,
    monitor: Monitor,
    image: RgbaImage,
    /// The same frame with the mouse pointer drawn in.
    pointer_image: Option<RgbaImage>,
    texture: Option<TextureHandle>,
    pointer_texture: Option<TextureHandle>,
    opened_at: Option<f64>,
    rect: Rect,
}

impl Screen {
    pub fn new(viewport: ViewportBuilder, monitor: Monitor, image: RgbaImage) -> Self {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(monitor.width as f32, monitor.height as f32));
        Self {
            viewport,
            monitor,
            image,
            pointer_image: None,
            texture: None,
            pointer_texture: None,
            opened_at: None,
            rect,
        }
    }

    /// This monitor in desktop coordinates (logical pixels).
    fn global(&self) -> Rect {
        let m = &self.monitor;
        Rect::from_min_size(pos2(m.x as f32, m.y as f32), vec2(m.width as f32, m.height as f32))
    }

    /// Desktop coordinates + offset = this window's coordinates.
    fn offset(&self) -> egui::Vec2 {
        self.rect.min - self.global().min
    }

    /// Image pixels per logical pixel.
    fn px_scale(&self) -> f32 {
        self.image.width() as f32 / self.global().width().max(1.0)
    }
}

/// What the background capture hands over once it is done.
pub struct Captured {
    pub desktop: Arc<RgbaImage>,
    /// One picture per screen, in the same order as the screens.
    pub images: Vec<RgbaImage>,
    /// The focused window in desktop coordinates (fallback without the Shell extension).
    pub window: Option<[i32; 4]>,
    pub pointer_job: Option<PointerJob>,
}

/// The screen capture, running while our windows start up.
pub type CaptureJob = JoinHandle<Result<Captured, String>>;

/// Grabs the desktop again with the pointer drawn in, returning it per screen.
/// It flags the bool once the pixels are taken.
pub type PointerJob = Box<dyn FnOnce(&AtomicBool) -> Vec<Option<RgbaImage>> + Send>;

fn load_texture(ctx: &egui::Context, name: &str, img: &RgbaImage) -> TextureHandle {
    let color_image =
        egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    ctx.load_texture(name, color_image, TextureOptions::NEAREST)
}

/// Which region a capture covers, as in GNOME's screenshot UI.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Area {
    Selection,
    Screen,
    Window,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Photo,
    Video,
    Gif,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Item {
    Tool(Tool),
    Color,
    Undo,
    Redo,
    Save,
    Copy,
    Close,
    Record,
    Mode(Area),
    Kind(Kind),
    Pointer,
    Settings,
    Sep,
}

impl Item {
    fn len(self) -> f32 {
        match self {
            Item::Sep => SEP,
            Item::Record => RECORD_W,
            Item::Mode(_) => MODE_W,
            _ => BTN,
        }
    }

    fn tooltip(self) -> (String, &'static str) {
        match self {
            Item::Tool(t) => (t.name().to_string(), key_label(t.key())),
            Item::Color => (t("tip.color").into(), t("tip.scroll")),
            Item::Undo => (t("tip.undo").into(), "Ctrl+Z"),
            Item::Redo => (t("tip.redo").into(), "Ctrl+Shift+Z"),
            Item::Save => (t("tip.save").into(), "Ctrl+S"),
            Item::Copy => (t("tip.copy").into(), "Ctrl+C"),
            Item::Close => (t("tip.close").into(), "Esc"),
            Item::Record => (t("tip.record").into(), "Enter"),
            Item::Mode(Area::Selection) => (t("tip.select_area").into(), "S"),
            Item::Mode(Area::Screen) => (t("tip.capture_screen").into(), "C"),
            Item::Mode(Area::Window) => (t("tip.capture_window").into(), "W"),
            Item::Kind(Kind::Photo) => (t("tip.screenshot").into(), "V"),
            Item::Kind(Kind::Video) => (t("tip.screencast").into(), "V"),
            Item::Kind(Kind::Gif) => (t("tip.gif").into(), "G"),
            Item::Pointer => (t("tip.pointer").into(), "P"),
            Item::Settings => (t("tip.settings").into(), ""),
            Item::Sep => (String::new(), ""),
        }
    }
}

fn key_label(key: Key) -> &'static str {
    match key {
        Key::P => "P",
        Key::L => "L",
        Key::A => "A",
        Key::R => "R",
        Key::E => "E",
        Key::M => "M",
        Key::T => "T",
        Key::B => "B",
        _ => "",
    }
}

const VBAR: [Item; 13] = [
    Item::Tool(Tool::Pen),
    Item::Tool(Tool::Line),
    Item::Tool(Tool::Arrow),
    Item::Tool(Tool::Rect),
    Item::Tool(Tool::Ellipse),
    Item::Tool(Tool::Marker),
    Item::Tool(Tool::Text),
    Item::Tool(Tool::Pixelate),
    Item::Sep,
    Item::Color,
    Item::Sep,
    Item::Undo,
    Item::Redo,
];
const HBAR: [Item; 4] = [Item::Save, Item::Copy, Item::Sep, Item::Close];
const HBAR_VIDEO: [Item; 3] = [Item::Record, Item::Sep, Item::Close];
const MODEBAR: [Item; 11] = [
    Item::Mode(Area::Selection),
    Item::Mode(Area::Screen),
    Item::Mode(Area::Window),
    Item::Sep,
    Item::Kind(Kind::Photo),
    Item::Kind(Kind::Video),
    Item::Kind(Kind::Gif),
    Item::Sep,
    Item::Pointer,
    Item::Sep,
    Item::Settings,
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    fn cursor(self) -> CursorIcon {
        match self {
            Handle::N | Handle::S => CursorIcon::ResizeVertical,
            Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
            Handle::NW | Handle::SE => CursorIcon::ResizeNwSe,
            Handle::NE | Handle::SW => CursorIcon::ResizeNeSw,
        }
    }
}

enum Drag {
    None,
    /// `previous` is restored if this turns out to be a click rather than a drag.
    Selecting {
        origin: Pos2,
        previous: Option<Rect>,
    },
    Moving {
        grab: egui::Vec2,
    },
    Resizing {
        handle: Handle,
        start: Rect,
    },
    Drawing,
    /// The press landed on UI chrome (or closed a popover); ignore it until release.
    Swallow,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Copy,
    Save,
}

struct Export {
    action: Action,
    /// Screens the selection touches; each reads back its own frame.
    involved: Vec<usize>,
    frames: Vec<Arc<Mutex<Option<Arc<egui::ColorImage>>>>>,
    requested: Vec<bool>,
    started: f64,
}

struct Closing {
    started: f64,
    /// Finishing a copy/save: no dimming, just a flash.
    export: bool,
}

struct Countdown {
    started: f64,
    request: record::Request,
}

struct TextDraft {
    pos: Pos2,
    text: String,
    color: Color32,
    size: f32,
    started: f64,
}

#[derive(Clone, Copy, Default)]
struct PointerIn {
    pos: Option<Pos2>,
    pressed: bool,
    down: bool,
    released: bool,
    origin: Option<Pos2>,
    shift: bool,
}

struct Chrome {
    vbar: Rect,
    hbar: Rect,
    palette: Rect,
    /// Popovers and tooltips for the vertical bar open to its right.
    opens_right: bool,
    hbar_below: bool,
    /// Screencast mode: no drawing tools, just "Record".
    video: bool,
}

impl Chrome {
    fn new(sel: Rect, screen: Rect, video: bool) -> Self {
        let inner = screen.shrink(MARGIN);
        let thick = BTN + 2.0 * PAD;
        let vlen = bar_len(&VBAR);
        let hlen = bar_len(Self::hbar_items(video));

        let mut vx = sel.right() + GAP;
        let mut opens_right = true;
        if vx + thick > inner.right() {
            vx = sel.left() - GAP - thick;
            opens_right = false;
            if vx < inner.left() {
                vx = sel.right() - GAP - thick;
            }
        }
        let vy = (sel.bottom() - vlen).min(inner.bottom() - vlen).max(inner.top());
        let vbar = Rect::from_min_size(pos2(vx, vy), vec2(thick, vlen));

        let mut hy = sel.bottom() + GAP;
        let mut hbar_below = true;
        if hy + thick > inner.bottom() {
            hy = sel.top() - GAP - thick;
            hbar_below = false;
            if hy < inner.top() {
                hy = sel.bottom() - GAP - thick;
            }
        }
        let hx = (sel.right() - hlen).min(inner.right() - hlen).max(inner.left());
        let mut hbar = Rect::from_min_size(pos2(hx, hy), vec2(hlen, thick));
        if !video && hbar.intersects(vbar.expand(GAP / 2.0)) {
            let dx = vbar.left() - GAP - hbar.right();
            hbar = hbar.translate(vec2(dx, 0.0));
            if hbar.left() < inner.left() {
                hbar = hbar.translate(vec2(vbar.right() + GAP - hbar.left(), 0.0));
            }
        }

        let psize = vec2(5.0 * SWATCH + 4.0 * SWATCH_GAP + 2.0 * POP_PAD, palette_height());
        let color_btn = items(vbar, &VBAR, true).find(|(i, _)| *i == Item::Color).map(|(_, r)| r).unwrap();
        let mut px = if opens_right { vbar.right() + GAP } else { vbar.left() - GAP - psize.x };
        if px + psize.x > inner.right() || px < inner.left() {
            px = if opens_right { vbar.left() - GAP - psize.x } else { vbar.right() + GAP };
        }
        let py = (color_btn.center().y - psize.y / 2.0).min(inner.bottom() - psize.y).max(inner.top());
        let palette = Rect::from_min_size(pos2(px, py), psize);

        Self { vbar, hbar, palette, opens_right: px > vbar.center().x, hbar_below, video }
    }

    fn hbar_items(video: bool) -> &'static [Item] {
        if video {
            &HBAR_VIDEO
        } else {
            &HBAR
        }
    }

    fn hit(&self, p: Pos2, palette_open: bool) -> bool {
        (!self.video && self.vbar.contains(p))
            || self.hbar.contains(p)
            || (!self.video && palette_open && self.palette.contains(p))
    }

    fn translate(&self, d: egui::Vec2) -> Self {
        Self {
            vbar: self.vbar.translate(d),
            hbar: self.hbar.translate(d),
            palette: self.palette.translate(d),
            opens_right: self.opens_right,
            hbar_below: self.hbar_below,
            video: self.video,
        }
    }
}

fn palette_height() -> f32 {
    POP_PAD + 2.0 * SWATCH + SWATCH_GAP + POP_PAD + 1.0 + POP_PAD + SWATCH + POP_PAD
}

fn bar_len(list: &[Item]) -> f32 {
    2.0 * PAD + list.iter().map(|i| i.len()).sum::<f32>()
}

/// The GNOME-style mode switcher at the top of every screen.
fn modebar_rect(screen: Rect) -> Rect {
    let size = vec2(bar_len(&MODEBAR), BTN + 2.0 * PAD);
    Rect::from_min_size(pos2(screen.center().x - size.x / 2.0, screen.top() + 16.0), size)
}

fn items(bar: Rect, list: &[Item], vertical: bool) -> impl Iterator<Item = (Item, Rect)> + '_ {
    let mut offset = PAD;
    list.iter().map(move |item| {
        let len = item.len();
        let r = if vertical {
            Rect::from_min_size(bar.min + vec2(PAD, offset), vec2(BTN, len))
        } else {
            Rect::from_min_size(bar.min + vec2(offset, PAD), vec2(len, BTN))
        };
        offset += len;
        (*item, r)
    })
}

pub struct QueijoApp {
    screens: Vec<Screen>,
    cfg: Config,
    /// The full capture and where it sits on the desktop (pixelate samples it).
    /// Empty until `capture_job` is done.
    desktop: Arc<RgbaImage>,
    capture_job: Option<CaptureJob>,
    /// Without a compositor: the windows are opaque and show the captured
    /// frame, dimmed, instead of the live screen.
    frozen: bool,
    desktop_rect: Rect,
    /// The part of the desktop our windows cover.
    bounds: Rect,

    // Everything below is in desktop coordinates (logical pixels), so a
    // selection or a drawing can span monitors. Each window paints it shifted
    // by `Screen::offset`.
    selection: Option<Rect>,
    sel_done_at: f64,
    drag: Drag,
    /// Where the primary button went down: (screen, window coordinates).
    press_origin: Option<(usize, Pos2)>,

    tool: Option<Tool>,
    color: Color32,
    sizes: [f32; Tool::ALL.len()],
    annotations: Vec<Annotation>,
    redo: Vec<Annotation>,
    current: Option<Annotation>,
    text: Option<TextDraft>,

    palette_open: bool,
    /// Hovered button (screen, item, since) for tooltip delays.
    hover: Option<(usize, Item, f64)>,
    highlight: Option<f32>,
    scroll_acc: f32,
    size_flash: Option<(f64, Pos2)>,

    area: Area,
    kind: Kind,
    pointer: bool,
    /// The focused window when we were launched (fallback without the Shell extension).
    window: Option<Rect>,
    /// With the Queijo Shell extension: every window, for the picker.
    picker: bool,
    shots: Vec<WindowShot>,
    thumbs: Vec<Option<TextureHandle>>,
    shots_job: Option<JoinHandle<Vec<WindowShot>>>,
    picker_since: Option<f64>,
    picker_hover: Option<usize>,
    /// Windows picked from the picker. They may have been covered, so inside
    /// the selection we show their captured pixels instead of the live screen.
    front: Vec<Rect>,
    /// Set once the Shell has the focused window's picture.
    window_taken: Arc<AtomicBool>,
    pointer_job: Option<PointerJob>,
    pointer_thread: Option<JoinHandle<Vec<Option<RgbaImage>>>>,
    /// Set once the pointer capture is requested; until it has had time to grab
    /// the screen our windows stay fully transparent.
    pointer_requested: Arc<AtomicBool>,
    hold_until: Option<f64>,
    /// The shortcut that launches Queijo; pressing it again stops a recording.
    stop_key: Arc<Mutex<Option<String>>>,
    countdown: Option<Countdown>,

    export: Option<Export>,
    closing: Option<Closing>,
    /// Encoding/copying runs on a worker so the windows can go away at once.
    worker_done: Arc<AtomicBool>,
    faded: Arc<AtomicBool>,
    hidden: bool,
}

pub fn setup_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Ubuntu's own font when it is there; egui's built-in one otherwise.
    if let Ok(bytes) = std::fs::read("/usr/share/fonts/truetype/ubuntu/Ubuntu-M.ttf") {
        fonts.font_data.insert("ui".into(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_visuals(egui::Visuals::dark());
}

impl QueijoApp {
    pub fn new(
        screens: Vec<Screen>,
        cfg: Config,
        (capture_job, desktop_rect): (CaptureJob, Rect),
        window_taken: Arc<AtomicBool>,
        shots_job: Option<JoinHandle<Vec<WindowShot>>>,
        frozen: bool,
    ) -> Self {
        let bounds = screens.iter().map(Screen::global).reduce(|a, b| a.union(b)).unwrap_or(desktop_rect);
        let stop_key = Arc::new(Mutex::new(None));
        let slot = stop_key.clone();
        std::thread::spawn(move || *slot.lock().unwrap() = crate::shortcut::find());
        Self {
            screens,
            desktop: Arc::new(RgbaImage::new(0, 0)),
            capture_job: Some(capture_job),
            frozen,
            desktop_rect,
            bounds,
            selection: None,
            sel_done_at: 0.0,
            drag: Drag::None,
            press_origin: None,
            tool: None,
            color: Color32::from_rgb(cfg.color[0], cfg.color[1], cfg.color[2]),
            sizes: Tool::ALL.map(|t| match cfg.sizes.get(t.index()) {
                Some(&s) if cfg.remember_tools => s.clamp(t.size_range().0, t.size_range().1),
                _ => t.default_size(),
            }),
            annotations: Vec::new(),
            redo: Vec::new(),
            current: None,
            text: None,
            palette_open: false,
            hover: None,
            highlight: None,
            scroll_acc: 0.0,
            size_flash: None,
            area: match cfg.start_area {
                config::StartArea::Selection => Area::Selection,
                config::StartArea::Screen => Area::Screen,
                config::StartArea::Window => Area::Window,
            },
            kind: match cfg.start_kind {
                config::StartKind::Screenshot => Kind::Photo,
                config::StartKind::Video => Kind::Video,
                config::StartKind::Gif => Kind::Gif,
            },
            pointer: cfg.show_pointer,
            cfg,
            window: None,
            picker: shots_job.is_some(),
            shots: Vec::new(),
            thumbs: Vec::new(),
            shots_job,
            picker_since: None,
            picker_hover: None,
            front: Vec::new(),
            window_taken,
            pointer_requested: Arc::new(AtomicBool::new(false)),
            pointer_job: None,
            pointer_thread: None,
            hold_until: None,
            stop_key,
            countdown: None,
            export: None,
            closing: None,
            worker_done: Arc::new(AtomicBool::new(true)),
            faded: Arc::new(AtomicBool::new(false)),
            hidden: false,
        }
    }

    /// Screencast or GIF: both record instead of taking a still.
    fn video(&self) -> bool {
        self.kind != Kind::Photo
    }

    fn stop_key(&self) -> String {
        self.stop_key.lock().unwrap().clone().unwrap_or_else(|| t("record.shortcut_fallback").into())
    }

    fn has_window(&self) -> bool {
        self.picker
            // Still being located: allow the mode, the highlight shows up when ready.
            || self.capture_job.is_some()
            || self.visible_window().is_some()
    }

    fn set_area(&mut self, area: Area) {
        if area == Area::Window && !self.has_window() {
            return;
        }
        self.commit_text();
        self.area = area;
        self.selection = None;
        self.palette_open = false;
    }

    fn set_kind(&mut self, kind: Kind) {
        self.commit_text();
        self.kind = kind;
        self.palette_open = false;
        if kind != Kind::Photo {
            self.tool = None;
        }
    }

    /// What a click on screen `idx` would select in Screen/Window mode.
    fn target(&self, idx: usize) -> Option<Rect> {
        match self.area {
            Area::Selection => None,
            Area::Screen => Some(self.screens[idx].global()),
            Area::Window if self.picker => None,
            Area::Window => self.visible_window(),
        }
    }

    fn picker_visible(&self) -> bool {
        self.picker
            && self.area == Area::Window
            && self.selection.is_none()
            && matches!(self.drag, Drag::None | Drag::Swallow)
            && self.countdown.is_none()
            && self.closing.is_none()
            && self.export.is_none()
    }

    /// Picker cards on screen `idx` (window coordinates): (index into `shots`, card).
    /// Each window is listed on the monitor holding its center, like GNOME does.
    fn picker_cards(&self, idx: usize, rect: Rect) -> Vec<(usize, Rect)> {
        let home = |frame: Rect| {
            self.screens.iter().position(|s| s.global().contains(frame.center())).or_else(|| {
                let area = |i: &usize| self.screens[*i].global().intersect(frame).area().max(0.0);
                (0..self.screens.len()).filter(|i| area(i) > 0.0).max_by(|a, b| area(a).total_cmp(&area(b)))
            })
        };
        let mine: Vec<usize> =
            (0..self.shots.len()).filter(|&i| home(shot_rect(&self.shots[i])) == Some(idx)).collect();
        if mine.is_empty() {
            return Vec::new();
        }
        const CARD_GAP: f32 = 28.0;
        const LABEL_H: f32 = 34.0;
        let area = Rect::from_min_max(
            pos2(rect.left() + 80.0, modebar_rect(rect).bottom() + 72.0),
            pos2(rect.right() - 80.0, rect.bottom() - 60.0),
        );
        let n = mine.len();
        let (mut cols, mut width) = (1, 0.0f32);
        for c in 1..=n {
            let rows = n.div_ceil(c);
            let cw = (area.width() - CARD_GAP * (c - 1) as f32) / c as f32;
            let ch = (area.height() - CARD_GAP * (rows - 1) as f32) / rows as f32 - LABEL_H;
            let w = cw.min(ch * 1.6).min(340.0);
            if w > width {
                (cols, width) = (c, w);
            }
        }
        let rows = n.div_ceil(cols);
        let cell = vec2(width, width / 1.6 + LABEL_H);
        let grid_h = rows as f32 * cell.y + (rows - 1) as f32 * CARD_GAP;
        let top = (area.center().y - grid_h / 2.0).max(area.top());
        mine.into_iter()
            .enumerate()
            .map(|(k, i)| {
                let (row, col) = (k / cols, k % cols);
                let in_row = if row == rows - 1 { n - row * cols } else { cols };
                let row_w = in_row as f32 * cell.x + (in_row - 1) as f32 * CARD_GAP;
                let x = area.center().x - row_w / 2.0 + col as f32 * (cell.x + CARD_GAP);
                let y = top + row as f32 * (cell.y + CARD_GAP);
                (i, Rect::from_min_size(pos2(x, y), cell))
            })
            .collect()
    }

    /// Picks a window from the picker: puts its own pixels on top of the frozen
    /// desktop (it may have been covered) and selects it.
    fn pick(&mut self, i: usize, now: f64) {
        let frame = shot_rect(&self.shots[i]);
        if let Some(img) = self.shots[i].image.take() {
            self.bring_to_front(frame, &img);
            self.shots[i].image = Some(img);
            self.front.push(frame);
        }
        self.selection = Some(frame.intersect(self.bounds));
        self.sel_done_at = now;
    }

    fn bring_to_front(&mut self, frame: Rect, img: &RgbaImage) {
        let paste = |target: &mut RgbaImage, origin: Pos2, scale: f32| {
            let (w, h) = ((frame.width() * scale).round() as u32, (frame.height() * scale).round() as u32);
            let resized;
            let src = if img.dimensions() == (w, h) {
                img
            } else {
                resized = image::imageops::resize(img, w.max(1), h.max(1), image::imageops::FilterType::Triangle);
                &resized
            };
            let at = (frame.min - origin) * scale;
            image::imageops::overlay(target, src, at.x.round() as i64, at.y.round() as i64);
        };
        for screen in &mut self.screens {
            if !screen.global().intersects(frame) {
                continue;
            }
            let (origin, scale) = (screen.global().min, screen.px_scale());
            paste(&mut screen.image, origin, scale);
            if let Some(p) = &mut screen.pointer_image {
                paste(p, origin, scale);
            }
            screen.texture = None;
            screen.pointer_texture = None;
        }
        let scale = self.desktop.width() as f32 / self.desktop_rect.width();
        paste(Arc::make_mut(&mut self.desktop), self.desktop_rect.min, scale);
    }

    /// The screen that shows the tool bars: the one under the selection's
    /// bottom-right corner, or else the one holding most of it.
    fn owner(&self) -> Option<usize> {
        let sel = self.selection?;
        let corner = sel.max - vec2(1.0, 1.0);
        self.screens.iter().position(|s| s.global().contains(corner)).or_else(|| {
            let area = |i: &usize| self.screens[*i].global().intersect(sel).area().max(0.0);
            (0..self.screens.len()).filter(|i| area(i) > 0.0).max_by(|a, b| area(a).total_cmp(&area(b)))
        })
    }

    /// The focused window clipped to our screens, if a meaningful part of it is on them.
    fn visible_window(&self) -> Option<Rect> {
        let visible = self.window?.intersect(self.bounds);
        (visible.width() >= 8.0 && visible.height() >= 8.0).then_some(visible)
    }

    /// Window mode, nothing selected yet: select the hovered window in the picker
    /// (or the active one), or without the extension the active window.
    fn select_window_target(&mut self, now: f64) {
        if self.picker {
            let choice = self.picker_hover.or_else(|| self.shots.iter().position(|w| w.focused));
            if let Some(i) = choice.or((!self.shots.is_empty()).then_some(0)) {
                self.pick(i, now);
            }
        } else if let Some(w) = self.visible_window() {
            self.selection = Some(w);
            self.sel_done_at = now;
        }
    }

    /// Enter: capture what is selected, or what the current mode points at.
    fn confirm(&mut self, idx: usize, now: f64) {
        if self.selection.is_none() {
            if self.area == Area::Window {
                self.select_window_target(now);
            } else {
                self.selection = Some(self.screens[idx].global());
                self.sel_done_at = now;
            }
        }
        if self.selection.is_none() {
            // Window mode with no window found: don't silently capture something else.
            return;
        }
        if self.video() {
            self.start_record(now);
        } else {
            self.start_export(Action::Copy, idx, now);
        }
    }

    /// Counts down on screen, then hands the area to a separate recorder process.
    fn start_record(&mut self, now: f64) {
        let Some(sel) = self.selection else { return };
        if self.closing.is_some() || self.countdown.is_some() {
            return;
        }
        self.commit_text();
        self.palette_open = false;
        let sel = sel.round();
        let request = record::Request {
            x: sel.min.x as i32,
            y: sel.min.y as i32,
            width: sel.width() as i32,
            height: sel.height() as i32,
            pointer: self.pointer,
            gif: self.kind == Kind::Gif,
        };
        self.countdown = Some(Countdown { started: now, request });
        self.remember_tools();
    }

    fn countdown_steps(&self) -> u32 {
        // When the desktop asks which screen to record, its dialog comes next: no countdown before it.
        if record::asks_first() {
            0
        } else {
            self.cfg.countdown.min(10)
        }
    }

    fn tick_countdown(&mut self, ctx: &egui::Context, now: f64) {
        let Some(c) = &self.countdown else { return };
        if now - c.started < self.countdown_steps() as f64 * COUNTDOWN_STEP {
            ctx.request_repaint();
            return;
        }
        let request = c.request;
        self.countdown = None;
        match record::spawn(request) {
            Ok(()) => self.closing = Some(Closing { started: now, export: false }),
            Err(e) => output::notify_error(t("notify.start_failed"), &e),
        }
    }

    fn size_tool(&self) -> Tool {
        self.tool.unwrap_or(Tool::Pen)
    }

    fn size(&self) -> f32 {
        self.sizes[self.size_tool().index()]
    }

    fn set_size(&mut self, size: f32) {
        let tool = self.size_tool();
        let (lo, hi) = tool.size_range();
        let size = size.clamp(lo, hi);
        self.sizes[tool.index()] = size;
        if let Some(draft) = &mut self.text {
            if tool == Tool::Text {
                draft.size = size;
            }
        }
    }

    fn set_color(&mut self, color: Color32) {
        self.color = color;
        if let Some(draft) = &mut self.text {
            draft.color = color;
        }
    }

    fn toggle_tool(&mut self, tool: Tool) {
        self.commit_text();
        self.tool = if self.tool == Some(tool) { None } else { Some(tool) };
    }

    fn commit_text(&mut self) {
        if let Some(d) = self.text.take() {
            let a = Annotation::Text { pos: d.pos, text: d.text, color: d.color, size: d.size };
            if a.is_meaningful() {
                self.annotations.push(a);
                self.redo.clear();
            }
        }
    }

    fn undo(&mut self) {
        if self.text.is_some() {
            self.text = None;
        } else if let Some(a) = self.annotations.pop() {
            self.redo.push(a);
        }
    }

    fn redo(&mut self) {
        if let Some(a) = self.redo.pop() {
            self.annotations.push(a);
        }
    }

    fn close(&mut self, now: f64) {
        if self.closing.is_none() {
            self.remember_tools();
            self.closing = Some(Closing { started: now, export: false });
        }
    }

    /// Saves the colour and tool sizes for next time (if the user wants that).
    fn remember_tools(&self) {
        if !self.cfg.remember_tools {
            return;
        }
        // Re-read so changes made in the settings window meanwhile aren't lost.
        let mut cfg = Config::load();
        cfg.color = [self.color.r(), self.color.g(), self.color.b()];
        cfg.sizes = self.sizes.to_vec();
        if cfg != Config::load() {
            let _ = cfg.save();
        }
    }

    /// The gear: closes the editor and opens the settings window.
    fn open_settings(&mut self, now: f64) {
        use std::os::unix::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).arg("--settings").process_group(0).spawn();
        }
        self.close(now);
    }

    // -----------------------------------------------------------------------
    // Export

    fn involved(&self, sel: Rect) -> Vec<usize> {
        (0..self.screens.len()).filter(|&i| self.screens[i].global().intersect(sel).is_positive()).collect()
    }

    fn start_export(&mut self, action: Action, idx: usize, now: f64) {
        if self.export.is_some() || self.closing.is_some() || self.video() {
            return;
        }
        self.commit_text();
        self.current = None;
        self.drag = Drag::None;
        self.palette_open = false;
        if self.selection.is_none() && self.area == Area::Window {
            // The window, or nothing — never the whole screen by accident.
            self.select_window_target(now);
            if self.selection.is_none() {
                return;
            }
        }
        // Nothing selected: take the whole screen the shortcut was pressed on.
        let sel = *self.selection.get_or_insert(self.screens[idx].global());

        if self.annotations.is_empty() {
            // Nothing drawn: the frozen capture already is the result, no need to render.
            let image = self.stitch(sel, |screen, inter| {
                let img = match &screen.pointer_image {
                    Some(img) if self.pointer => img,
                    _ => &screen.image,
                };
                let s = screen.px_scale();
                let r = inter.translate(-screen.global().min.to_vec2());
                let (x, y) = ((r.min.x * s).round() as u32, (r.min.y * s).round() as u32);
                let (w, h) = ((r.width() * s).round() as u32, (r.height() * s).round() as u32);
                Some(image::imageops::crop_imm(img, x, y, w.max(1), h.max(1)).to_image())
            });
            self.finish_export(action, image, now);
            return;
        }

        // Drawings: read back each involved window's (clean) frame and stitch them.
        let n = self.screens.len();
        self.export = Some(Export {
            action,
            involved: self.involved(sel),
            frames: (0..n).map(|_| Arc::new(Mutex::new(None))).collect(),
            requested: vec![false; n],
            started: now,
        });
    }

    fn request_capture(&mut self, ctx: &egui::Context, idx: usize) {
        let Some(export) = &mut self.export else { return };
        if std::mem::replace(&mut export.requested[idx], true) {
            return;
        }
        let slot = export.frames[idx].clone();
        let repaint = ctx.clone();
        ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
            *slot.lock().unwrap() = Some(image);
            repaint.request_repaint();
        })));
    }

    fn poll_export(&mut self, now: f64) {
        let (Some(export), Some(sel)) = (&self.export, self.selection) else { return };
        let ready = export.involved.iter().all(|&i| export.frames[i].lock().unwrap().is_some());
        if !ready {
            if now - export.started > 2.0 {
                // The backend never delivered a frame; export without the drawings.
                eprintln!("queijo: framebuffer readback timed out");
                output::notify_error("Queijo", t("notify.drawings_failed"));
                let action = export.action;
                self.export = None;
                self.annotations.clear();
                self.start_export(action, self.owner().unwrap_or(0), now);
            }
            return;
        }
        let frames: Vec<Option<Arc<egui::ColorImage>>> =
            export.frames.iter().map(|f| f.lock().unwrap().take()).collect();
        let action = export.action;
        self.export = None;
        let image = self.stitch(sel, |screen, inter| {
            let i = self.screens.iter().position(|s| std::ptr::eq(s, screen))?;
            let frame = frames[i].as_ref()?;
            Some(crop_frame(frame, inter.translate(screen.offset()), screen.rect))
        });
        self.finish_export(action, image, now);
    }

    /// Builds the image of `sel` (desktop coordinates) from per-screen pieces.
    /// `piece` returns the part of a screen covered by `inter`, in that screen's pixels.
    fn stitch(&self, sel: Rect, mut piece: impl FnMut(&Screen, Rect) -> Option<RgbaImage>) -> RgbaImage {
        let involved = self.involved(sel);
        let scale = involved.first().map(|&i| self.screens[i].px_scale()).unwrap_or(1.0);
        let px = |v: f32| (v * scale).round().max(1.0) as u32;
        // Parts of the selection outside every monitor stay transparent.
        let mut out = RgbaImage::new(px(sel.width()), px(sel.height()));
        for i in involved {
            let screen = &self.screens[i];
            let inter = sel.intersect(screen.global());
            let Some(mut part) = piece(screen, inter) else { continue };
            let (w, h) = (px(inter.width()), px(inter.height()));
            if part.dimensions() != (w, h) {
                part = image::imageops::resize(&part, w, h, image::imageops::FilterType::Triangle);
            }
            let at = (inter.min - sel.min) * scale;
            image::imageops::replace(&mut out, &part, at.x.round() as i64, at.y.round() as i64);
        }
        out
    }

    /// Hands the image to a worker (encoding a large PNG takes a while) and
    /// lets the windows disappear right away.
    fn finish_export(&mut self, action: Action, image: RgbaImage, now: f64) {
        self.remember_tools();
        let (cfg, done, faded) = (self.cfg.clone(), self.worker_done.clone(), self.faded.clone());
        done.store(false, Ordering::SeqCst);
        std::thread::spawn(move || {
            let copy = action == Action::Copy || cfg.save_also_copies;
            let save = action == Action::Save || cfg.copy_also_saves;
            if copy {
                if let Err(e) = output::encode_png_fast(&image).and_then(|png| output::copy_png(&png)) {
                    output::notify_error(t("notify.copy_failed"), &e);
                }
            }
            if save {
                match output::save_image(&image, &cfg.screenshots, cfg.image_format, cfg.jpeg_quality) {
                    Ok(path) => {
                        let what = if copy { t("notify.saved_copied") } else { t("notify.saved") };
                        output::notify(what, &output::tilde(&path), Some(&path));
                    }
                    Err(e) => output::notify_error(t("notify.save_failed"), &e),
                }
            }
            done.store(true, Ordering::SeqCst);
            if faded.load(Ordering::SeqCst) {
                std::process::exit(0);
            }
        });
        self.closing = Some(Closing { started: now, export: true });
    }

    /// Alpha of everything while closing (1 → 0). Exits, or hides the windows
    /// until the worker is done, once the fade is over.
    fn closing_alpha(&mut self, ctx: &egui::Context, now: f64) -> f32 {
        let Some(c) = &self.closing else { return 1.0 };
        let t = now - c.started;
        if t < FADE_OUT {
            ctx.request_repaint();
            return 1.0 - paint::ease_out((t / FADE_OUT) as f32);
        }
        self.faded.store(true, Ordering::SeqCst);
        if self.worker_done.load(Ordering::SeqCst) {
            std::process::exit(0);
        }
        if !std::mem::replace(&mut self.hidden, true) {
            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(false));
            for idx in 1..self.screens.len() {
                ctx.send_viewport_cmd_to(viewport_id(idx), ViewportCommand::Visible(false));
            }
        }
        0.0
    }

    // -----------------------------------------------------------------------
    // Per-screen frame

    fn screen_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let now = ctx.input(|i| i.time);
        self.screens[idx].rect = rect;

        // The other windows show this state too: keep them in step while something happens.
        let busy = !matches!(self.drag, Drag::None)
            || self.export.is_some()
            || self.closing.is_some()
            || self.countdown.is_some()
            || self.hold_until.is_some_and(|t| now < t);
        if busy || ctx.input(|i| !i.events.is_empty()) {
            self.wake_others(&ctx, idx);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            std::process::exit(0);
        }

        if self.capture_job.is_some() || self.hold_until.is_some_and(|t| now < t) {
            // Still capturing: stay invisible (and keep the normal cursor) so
            // none of it ends up in the picture.
            ctx.set_cursor_icon(CursorIcon::Default);
            ctx.request_repaint_after(Duration::from_millis(15));
            return;
        }
        let screen = &mut self.screens[idx];
        let opened = *screen.opened_at.get_or_insert(now);
        let intro = paint::ease_out(((now - opened) / 0.2) as f32);
        if intro < 1.0 {
            ctx.request_repaint();
        }
        let offset = screen.offset();

        self.poll_export(now);
        let alpha = self.closing_alpha(&ctx, now);
        let exporting_close = self.closing.as_ref().is_some_and(|c| c.export);

        let mut cursor = CursorIcon::Crosshair;
        let input = self.pointer_input(&ctx, idx);
        if self.export.is_none() && self.closing.is_none() {
            if self.countdown.is_some() {
                if ctx.input(|i| i.key_pressed(Key::Escape)) {
                    self.countdown = None;
                }
                cursor = CursorIcon::Default;
            } else {
                self.handle_keys(&ctx, idx, now);
                cursor = self.handle_pointer(&ctx, idx, rect, now, input);
            }
        }

        // The live screen shows through; only the readback for an export (which
        // needs real pixels under the drawings) and picked windows, which may
        // have been covered, show the captured frame, and only inside the selection.
        // Without a compositor there is no live screen to show through, so the
        // captured frame fills the window.
        let painter = ui.painter().clone();
        let sel = self.selection.map(|s| s.translate(offset));
        let exporting = self.export.as_ref().is_some_and(|e| e.involved.contains(&idx));
        let frozen: Vec<Rect> = match sel {
            _ if self.frozen => vec![rect],
            Some(s) if exporting => vec![s],
            Some(s) => {
                self.front.iter().map(|f| f.translate(offset).intersect(s)).filter(|f| f.is_positive()).collect()
            }
            None => Vec::new(),
        };
        if !frozen.is_empty() {
            let texture = self.frozen_texture(&ctx, idx);
            let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
            for r in frozen {
                painter.with_clip_rect(r).image(texture, rect, uv, Color32::WHITE);
            }
        }
        if !self.video() {
            let clip = painter.with_clip_rect(sel.unwrap_or(rect).intersect(rect));
            for a in self.annotations.iter().chain(&self.current) {
                a.translated(offset).paint(&clip);
            }
            self.paint_text_draft(&ctx, &painter, offset, now);
        }

        if exporting {
            // Keep the selection clean (no chrome over it) until the GPU readback arrives.
            if let Some(s) = sel {
                let dim = Color32::from_black_alpha((255.0 * DIM) as u8);
                paint_around(&painter, rect, s.intersect(rect), dim);
            }
            self.request_capture(&ctx, idx);
            ctx.request_repaint();
            ctx.set_cursor_icon(CursorIcon::Default);
            return;
        }

        // In Screen/Window mode, preview what a click would capture.
        let idle =
            matches!(self.drag, Drag::None | Drag::Swallow) && self.closing.is_none() && self.countdown.is_none();
        let target = match self.area {
            _ if sel.is_some() || !idle => None,
            Area::Screen if input.pos.is_none() => None,
            _ => self.target(idx).map(|t| t.translate(offset)),
        };

        // Dim everything outside the selection.
        let dim_alpha = if exporting_close { 0.0 } else { alpha };
        let picker = self.picker_visible();
        let picker_t =
            ctx.animate_bool_with_time_and_easing(Id::unique(("picker-dim", idx)), picker, 0.2, easing::cubic_out);
        let strength = DIM + (PICKER_DIM - DIM) * picker_t;
        let dim = Color32::from_black_alpha((255.0 * strength * intro * dim_alpha) as u8);
        match sel.or(target).map(|s| s.intersect(rect)).filter(|s| s.is_positive()) {
            Some(s) => paint_around(&painter, rect, s, dim),
            None => {
                painter.rect_filled(rect, 0, dim);
            }
        }

        let fg = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("queijo-chrome")));
        let ui_alpha = if exporting_close { 0.0 } else { alpha };
        match (sel, target) {
            (Some(s), _) => self.paint_selection(&ctx, &painter, s, rect, now, ui_alpha),
            (None, Some(t)) => {
                let a = intro * ui_alpha;
                painter.rect_stroke(t, 0, Stroke::new(2.0, ACCENT.gamma_multiply(a)), StrokeKind::Inside);
                self.paint_idle(&ctx, &painter, &fg, idx, rect, input, a);
            }
            (None, None) => self.paint_idle(&ctx, &painter, &fg, idx, rect, input, intro * ui_alpha),
        }
        if picker {
            self.paint_picker(&ctx, &fg, idx, rect, now, input, intro * ui_alpha);
        } else if idx == 0 {
            self.picker_since = None;
        }
        if let Some(over_button) = self.paint_modebar(&ctx, &fg, idx, rect, now, input, intro * ui_alpha) {
            cursor = if over_button { CursorIcon::PointingHand } else { CursorIcon::Default };
        }
        if let Some(over_button) = self.paint_chrome(&ctx, &fg, idx, rect, now, input, ui_alpha) {
            if over_button {
                cursor = CursorIcon::PointingHand;
            } else if matches!(self.drag, Drag::None | Drag::Swallow) {
                cursor = CursorIcon::Default;
            }
        }
        self.paint_size_flash(&ctx, &fg, idx, now);
        self.paint_flash(&fg, idx, now);
        self.paint_countdown(&ctx, &fg, idx, rect, now);
        ctx.set_cursor_icon(cursor);
    }

    /// The captured frame of screen `idx`, with the pointer drawn in when that is wanted.
    fn frozen_texture(&mut self, ctx: &egui::Context, idx: usize) -> egui::TextureId {
        let show_pointer = self.pointer && !self.video();
        let screen = &mut self.screens[idx];
        let name = format!("screen-{}", screen.monitor.name);
        match &screen.pointer_image {
            Some(img) if show_pointer => {
                screen.pointer_texture.get_or_insert_with(|| load_texture(ctx, &name, img)).id()
            }
            _ => screen.texture.get_or_insert_with(|| load_texture(ctx, &name, &screen.image)).id(),
        }
    }

    fn pointer_input(&self, ctx: &egui::Context, idx: usize) -> PointerIn {
        ctx.input(|i| PointerIn {
            pos: i.pointer.latest_pos(),
            pressed: i.pointer.primary_pressed(),
            down: i.pointer.primary_down(),
            released: i.pointer.primary_released(),
            origin: if i.pointer.primary_pressed() {
                i.pointer.press_origin().or(i.pointer.latest_pos())
            } else {
                self.press_origin.filter(|(s, _)| *s == idx).map(|(_, p)| p)
            },
            shift: i.modifiers.shift,
        })
    }

    // -----------------------------------------------------------------------
    // Input

    fn handle_keys(&mut self, ctx: &egui::Context, idx: usize, now: f64) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            match event {
                Event::Copy | Event::Cut if !self.video() => self.start_export(Action::Copy, idx, now),
                Event::Text(t) | Event::Paste(t) => {
                    if let Some(d) = &mut self.text {
                        d.text.push_str(&t);
                        d.started = now;
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } => self.on_key(key, modifiers, idx, now),
                _ => {}
            }
            if self.export.is_some() || self.closing.is_some() || self.countdown.is_some() {
                break;
            }
        }
    }

    fn on_key(&mut self, key: Key, m: Modifiers, idx: usize, now: f64) {
        let cmd = m.command || m.ctrl;

        if let Some(d) = &mut self.text {
            d.started = now;
            match key {
                Key::Escape => return self.commit_text(),
                Key::Enter if m.shift => return d.text.push('\n'),
                Key::Enter => return self.commit_text(),
                Key::Backspace => {
                    d.text.pop();
                    return;
                }
                _ if !cmd => return, // characters arrive as Event::Text
                _ => {}
            }
        }

        let no_selection = self.selection.is_none();
        match key {
            Key::Escape if self.palette_open => self.palette_open = false,
            Key::Escape => self.close(now),
            Key::Enter => self.confirm(idx, now),
            // GNOME screenshot UI keys, while nothing is selected yet.
            Key::S if !cmd && no_selection => self.set_area(Area::Selection),
            Key::C if !cmd && no_selection => self.set_area(Area::Screen),
            Key::W if !cmd && no_selection => self.set_area(Area::Window),
            Key::V if !cmd => self.set_kind(if self.kind == Kind::Video { Kind::Photo } else { Kind::Video }),
            Key::G if !cmd => self.set_kind(if self.kind == Kind::Gif { Kind::Photo } else { Kind::Gif }),
            Key::P if !cmd && (no_selection || self.video()) => self.pointer = !self.pointer,
            Key::S if cmd => self.start_export(Action::Save, idx, now),
            Key::C if cmd => self.start_export(Action::Copy, idx, now),
            Key::Z if cmd && m.shift => self.redo(),
            Key::Z if cmd => self.undo(),
            Key::Y if cmd => self.redo(),
            Key::A if cmd => {
                self.selection = Some(self.screens[idx].global());
                self.sel_done_at = now;
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                let step = if m.shift { 10.0 } else { 1.0 };
                let d = match key {
                    Key::ArrowLeft => vec2(-step, 0.0),
                    Key::ArrowRight => vec2(step, 0.0),
                    Key::ArrowUp => vec2(0.0, -step),
                    _ => vec2(0.0, step),
                };
                if let Some(sel) = self.selection {
                    self.selection = Some(clamp_into(sel.translate(d), self.bounds));
                }
            }
            k if !cmd && !m.alt => {
                if let Some(t) = Tool::ALL.into_iter().find(|t| t.key() == k) {
                    if self.selection.is_some() && !self.video() {
                        self.toggle_tool(t);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_pointer(
        &mut self,
        ctx: &egui::Context,
        idx: usize,
        rect: Rect,
        now: f64,
        input: PointerIn,
    ) -> CursorIcon {
        let chrome = self.chrome_rects(idx, rect);
        let offset = self.screens[idx].offset();
        let desktop = |p: Pos2| p - offset;

        if input.pressed {
            // Use where the button went down, not where the pointer is by the time we see it.
            self.press_origin = input.origin.map(|p| (idx, p));
            if let Some(p) = input.origin {
                let on_modebar = self.modebar_visible() && modebar_rect(rect).contains(p);
                if on_modebar || chrome.as_ref().is_some_and(|c| c.hit(p, self.palette_open)) {
                    self.drag = Drag::Swallow;
                } else if self.palette_open {
                    self.palette_open = false;
                    self.drag = Drag::Swallow;
                } else {
                    self.begin_press(ctx, idx, desktop(p), now);
                }
            }
        }
        if input.down {
            // While the button is held the compositor keeps sending us the pointer even
            // over other monitors, so a drag can cross screens.
            if let Some(p) = input.pos {
                self.drag_to(desktop(p).clamp(self.bounds.min, self.bounds.max), input.shift);
            }
        }
        if input.released {
            self.end_press(ctx, now);
        }

        // The mouse wheel resizes the current tool.
        let scroll: f32 = ctx.input(|i| {
            i.events
                .iter()
                .map(|e| match e {
                    Event::MouseWheel { unit, delta, .. } => match unit {
                        egui::MouseWheelUnit::Point => delta.y,
                        egui::MouseWheelUnit::Line => delta.y * 20.0,
                        egui::MouseWheelUnit::Page => delta.y * 60.0,
                    },
                    _ => 0.0,
                })
                .sum()
        });
        if scroll != 0.0 && self.selection.is_some() && !self.video() {
            if let Some(p) = input.pos {
                if !chrome.as_ref().is_some_and(|c| c.hit(p, true)) {
                    self.scroll_acc += scroll;
                    let steps = (self.scroll_acc / 20.0).trunc();
                    if steps != 0.0 {
                        self.scroll_acc -= steps * 20.0;
                        let step = if self.size_tool() == Tool::Text { 2.0 } else { 1.0 };
                        self.set_size(self.size() + steps * step);
                        self.size_flash = Some((now, desktop(p)));
                    }
                }
            }
        }

        match &self.drag {
            Drag::Moving { .. } => return CursorIcon::Grabbing,
            Drag::Resizing { handle, .. } => return handle.cursor(),
            Drag::Selecting { .. } | Drag::Drawing => return CursorIcon::Crosshair,
            Drag::Swallow | Drag::None => {}
        }
        let Some(p) = input.pos.map(desktop) else { return CursorIcon::Crosshair };
        let Some(sel) = self.selection else {
            if self.picker_visible() {
                let local = p + offset;
                let over = self.picker_cards(idx, rect).iter().any(|(_, r)| r.contains(local));
                return if over { CursorIcon::PointingHand } else { CursorIcon::Default };
            }
            return match self.area {
                Area::Selection => CursorIcon::Crosshair,
                _ if self.target(idx).is_some_and(|t| t.contains(p)) => CursorIcon::PointingHand,
                _ => CursorIcon::Default,
            };
        };
        if let Some(h) = hit_handle(sel, p) {
            return h.cursor();
        }
        if sel.contains(p) {
            return match self.tool {
                None => CursorIcon::Move,
                Some(Tool::Text) => CursorIcon::Text,
                Some(_) => CursorIcon::Crosshair,
            };
        }
        CursorIcon::Crosshair
    }

    /// `p` is in desktop coordinates.
    fn begin_press(&mut self, ctx: &egui::Context, idx: usize, p: Pos2, now: f64) {
        if self.text.is_some() {
            // First click outside the text box just finishes typing.
            self.commit_text();
            self.drag = Drag::Swallow;
            return;
        }
        if let Some(sel) = self.selection {
            if let Some(handle) = hit_handle(sel, p) {
                self.drag = Drag::Resizing { handle, start: sel };
                return;
            }
            if sel.contains(p) {
                match self.tool {
                    None => self.drag = Drag::Moving { grab: p - sel.min },
                    Some(_) if self.video() => self.drag = Drag::Moving { grab: p - sel.min },
                    Some(Tool::Text) => {
                        self.start_text(ctx, p, now);
                        self.drag = Drag::Swallow;
                    }
                    Some(tool) => {
                        self.current = Annotation::start(tool, p, self.color, self.size());
                        self.drag = Drag::Drawing;
                    }
                }
                return;
            }
        }
        if self.picker_visible() {
            let local = p + self.screens[idx].offset();
            let rect = self.screens[idx].rect;
            if let Some((i, _)) = self.picker_cards(idx, rect).into_iter().find(|(_, r)| r.contains(local)) {
                self.pick(i, now);
            }
            self.drag = Drag::Swallow;
            return;
        }
        if self.area != Area::Selection {
            // Screen/Window: a click picks the highlighted target.
            if let Some(t) = self.target(idx).filter(|t| t.contains(p)) {
                self.selection = Some(t);
                self.sel_done_at = now;
            }
            self.drag = Drag::Swallow;
            return;
        }
        let previous = self.selection.take();
        self.drag = Drag::Selecting { origin: p, previous };
    }

    fn start_text(&mut self, ctx: &egui::Context, p: Pos2, now: f64) {
        let painter = ctx.layer_painter(LayerId::background());
        // Clicking an existing label re-opens it for editing.
        let hit =
            self.annotations.iter().rposition(|a| a.text_rect(&painter).is_some_and(|r| r.expand(4.0).contains(p)));
        if let Some(i) = hit {
            if let Annotation::Text { pos, text, color, size } = self.annotations.remove(i) {
                self.color = color;
                self.sizes[Tool::Text.index()] = size;
                self.text = Some(TextDraft { pos, text, color, size, started: now });
                return;
            }
        }
        let size = self.sizes[Tool::Text.index()];
        // Center the caret on the click, like a text cursor.
        let pos = p - vec2(0.0, size * 0.6);
        self.text = Some(TextDraft { pos, text: String::new(), color: self.color, size, started: now });
    }

    fn drag_to(&mut self, p: Pos2, shift: bool) {
        match &self.drag {
            Drag::Selecting { origin, .. } => self.selection = Some(Rect::from_two_pos(origin.round(), p.round())),
            Drag::Moving { grab } => {
                if let Some(sel) = self.selection {
                    let moved = Rect::from_min_size((p - *grab).round(), sel.size());
                    self.selection = Some(clamp_into(moved, self.bounds));
                }
            }
            Drag::Resizing { handle, start } => {
                let p = p.round();
                let (mut x0, mut y0, mut x1, mut y1) = (start.min.x, start.min.y, start.max.x, start.max.y);
                match handle {
                    Handle::N => y0 = p.y,
                    Handle::S => y1 = p.y,
                    Handle::W => x0 = p.x,
                    Handle::E => x1 = p.x,
                    Handle::NW => (x0, y0) = (p.x, p.y),
                    Handle::NE => (x1, y0) = (p.x, p.y),
                    Handle::SW => (x0, y1) = (p.x, p.y),
                    Handle::SE => (x1, y1) = (p.x, p.y),
                }
                self.selection = Some(Rect::from_two_pos(pos2(x0, y0), pos2(x1, y1)));
            }
            Drag::Drawing => {
                if let Some(a) = &mut self.current {
                    a.update(p, shift);
                }
            }
            Drag::None | Drag::Swallow => {}
        }
    }

    fn end_press(&mut self, ctx: &egui::Context, now: f64) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Selecting { previous, .. } => match self.selection {
                Some(s) if s.width() >= 4.0 && s.height() >= 4.0 => self.sel_done_at = now,
                _ => self.selection = previous,
            },
            Drag::Resizing { .. } => {
                if self.selection.is_some_and(|s| s.width() < 2.0 || s.height() < 2.0) {
                    self.selection = None;
                }
            }
            Drag::Drawing => {
                if let Some(mut a) = self.current.take() {
                    if a.is_meaningful() {
                        a.finish();
                        if let Annotation::Pixelate { rect, block, texture } = &mut a {
                            let scale = self.desktop.width() as f32 / self.desktop_rect.width();
                            *texture = paint::pixelate(ctx, &self.desktop, *rect, self.desktop_rect, scale, *block);
                        }
                        self.annotations.push(a);
                        self.redo.clear();
                    }
                }
            }
            Drag::Moving { .. } | Drag::None | Drag::Swallow => {}
        }
        self.press_origin = None;
    }

    // -----------------------------------------------------------------------
    // Painting

    fn chrome_rects(&self, idx: usize, rect: Rect) -> Option<Chrome> {
        let sel = self.selection?;
        if matches!(self.drag, Drag::Selecting { .. }) || self.countdown.is_some() || self.owner() != Some(idx) {
            return None;
        }
        Some(Chrome::new(sel.translate(self.screens[idx].offset()), rect, self.video()))
    }

    /// `sel` is in this window's coordinates.
    fn paint_selection(&self, ctx: &egui::Context, painter: &Painter, sel: Rect, rect: Rect, now: f64, alpha: f32) {
        painter.rect_stroke(
            sel.expand(1.5),
            0,
            Stroke::new(1.0, Color32::from_black_alpha((70.0 * alpha) as u8)),
            StrokeKind::Outside,
        );
        painter.rect_stroke(sel, 0, Stroke::new(1.5, ACCENT.gamma_multiply(alpha)), StrokeKind::Outside);

        let creating = matches!(self.drag, Drag::Selecting { .. });
        if !creating && self.countdown.is_none() {
            let t = ((now - self.sel_done_at) / 0.3) as f32;
            let pop = paint::ease_out_back(t);
            if t < 1.0 {
                ctx.request_repaint();
            }
            let a = alpha * if self.tool.is_some() { 0.6 } else { 1.0 };
            let r = 4.5 * pop;
            for p in handle_points(sel) {
                painter.circle_filled(p + vec2(0.0, 1.0), r + 1.5, Color32::from_black_alpha((60.0 * a) as u8));
                painter.circle_filled(p, r, Color32::WHITE.gamma_multiply(a));
                painter.circle_stroke(p, r, Stroke::new(1.5, ACCENT.gamma_multiply(a)));
            }
        }

        // Size badge, in real pixels.
        let scale = self.owner().map(|i| self.screens[i].px_scale()).unwrap_or(1.0);
        let label = format!("{} × {}", (sel.width() * scale).round(), (sel.height() * scale).round());
        let galley = painter.layout_no_wrap(label, FontId::proportional(12.0), TEXT.gamma_multiply(alpha));
        let size = galley.size() + vec2(14.0, 8.0);
        let mut at = sel.min + vec2(0.0, -size.y - 7.0);
        if at.y < rect.top() + 4.0 {
            at = sel.min + vec2(7.0, 7.0);
        }
        let badge = Rect::from_min_size(at, size);
        painter.rect_filled(badge, 6, Color32::from_rgba_unmultiplied(18, 18, 22, (225.0 * alpha) as u8));
        painter.galley(badge.min + vec2(7.0, 4.0), galley, TEXT);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_idle(
        &self,
        ctx: &egui::Context,
        painter: &Painter,
        fg: &Painter,
        idx: usize,
        rect: Rect,
        input: PointerIn,
        alpha: f32,
    ) {
        // Crosshair guides help line up the first corner.
        if let (Some(p), Drag::None, Area::Selection) = (input.pos, &self.drag, self.area) {
            if rect.contains(p) {
                let s = Stroke::new(1.0, Color32::from_white_alpha((38.0 * alpha) as u8));
                painter.line_segment([pos2(rect.left(), p.y + 0.5), pos2(rect.right(), p.y + 0.5)], s);
                painter.line_segment([pos2(p.x + 0.5, rect.top()), pos2(p.x + 0.5, rect.bottom())], s);
            }
        }

        let show = self.selection.is_none() && self.closing.is_none() && self.countdown.is_none();
        let a = ctx.animate_bool_with_time_and_easing(Id::unique(("hint", idx)), show, 0.25, easing::cubic_out) * alpha;
        if a <= 0.0 {
            return;
        }
        let (title, first_key) = match (self.area, self.video()) {
            (Area::Selection, false) => (t("hint.select"), ("Ctrl+A", t("hint.full_screen"))),
            (Area::Screen, false) => (t("hint.screen"), ("Enter", t("hint.this_screen"))),
            (Area::Window, false) if self.picker => (t("hint.picker"), ("Enter", t("hint.active_window"))),
            (Area::Window, _) if self.visible_window().is_none() && self.capture_job.is_none() => {
                (t("hint.window_missing"), ("S", t("hint.selection_key")))
            }
            (Area::Window, false) => (t("hint.window"), ("Enter", t("hint.capture"))),
            (Area::Selection, true) => (t("hint.record_select"), ("Ctrl+A", t("hint.full_screen"))),
            (Area::Screen, true) => (t("hint.record_screen"), ("Enter", t("hint.this_screen"))),
            (Area::Window, true) if self.picker => (t("hint.record_window"), ("Enter", t("hint.active_window"))),
            (Area::Window, true) => (t("hint.record_window"), ("Enter", t("hint.this_window"))),
        };
        let title = fg.layout_no_wrap(title.into(), FontId::proportional(13.5), TEXT.gamma_multiply(a));
        let keys: Vec<_> = [first_key, ("Esc", t("hint.close"))]
            .into_iter()
            .map(|(k, l)| {
                (
                    fg.layout_no_wrap(k.into(), FontId::proportional(11.5), TEXT.gamma_multiply(a)),
                    fg.layout_no_wrap(l.into(), FontId::proportional(12.5), TEXT_DIM.gamma_multiply(a)),
                )
            })
            .collect();
        let keys_w: f32 = keys.iter().map(|(k, l)| k.size().x + 12.0 + 6.0 + l.size().x + 16.0).sum();
        let size = vec2(16.0 + title.size().x + 18.0 + keys_w, 36.0);
        let top = modebar_rect(rect).bottom() + 10.0 - (1.0 - a) * 6.0;
        let pill = Rect::from_min_size(pos2(rect.center().x - size.x / 2.0, top), size);
        paint::panel(fg, pill, 18, a);
        let y = pill.center().y;
        let mut x = pill.left() + 16.0;
        fg.galley(pos2(x, y - title.size().y / 2.0), title.clone(), TEXT);
        x += title.size().x + 18.0;
        for (k, l) in keys {
            let cap = Rect::from_min_size(pos2(x, y - 10.0), vec2(k.size().x + 12.0, 20.0));
            fg.rect_filled(cap, 5, Color32::from_white_alpha((22.0 * a) as u8));
            fg.galley(cap.center() - k.size() / 2.0, k, TEXT);
            x = cap.right() + 6.0;
            fg.galley(pos2(x, y - l.size().y / 2.0), l.clone(), TEXT_DIM);
            x += l.size().x + 16.0;
        }
    }

    /// Window picker: a grid of live previews, like GNOME's window mode.
    #[allow(clippy::too_many_arguments)]
    fn paint_picker(
        &mut self,
        ctx: &egui::Context,
        fg: &Painter,
        idx: usize,
        rect: Rect,
        now: f64,
        input: PointerIn,
        alpha: f32,
    ) {
        let since = *self.picker_since.get_or_insert(now);
        if self.shots_job.is_some() {
            let msg = fg.layout_no_wrap(
                t("picker.loading").into(),
                FontId::proportional(14.0),
                TEXT_DIM.gamma_multiply(alpha),
            );
            fg.galley(rect.center() - msg.size() / 2.0, msg, TEXT_DIM);
            return;
        }
        let cards = self.picker_cards(idx, rect);
        if cards.is_empty() {
            if self.shots.is_empty() && idx == 0 {
                let msg = fg.layout_no_wrap(
                    t("picker.empty").into(),
                    FontId::proportional(14.0),
                    TEXT_DIM.gamma_multiply(alpha),
                );
                fg.galley(rect.center() - msg.size() / 2.0, msg, TEXT_DIM);
            }
            return;
        }
        if self.thumbs.len() != self.shots.len() {
            self.thumbs = (0..self.shots.len()).map(|_| None).collect();
        }
        let mut hovered = None;
        for (k, &(i, card)) in cards.iter().enumerate() {
            // Cards rise in one after another.
            let t = ((now - since - k as f64 * 0.035) / 0.28) as f32;
            if t < 1.0 {
                ctx.request_repaint();
            }
            let a = paint::ease_out(t) * alpha;
            if a <= 0.0 {
                continue;
            }
            let card = card.translate(vec2(0.0, (1.0 - paint::ease_out(t)) * 14.0));
            let is_hovered = input.pos.is_some_and(|p| card.contains(p)) && t >= 1.0;
            if is_hovered {
                hovered = Some(i);
            }
            let h = ctx.animate_bool_with_time(Id::unique(("card", i)), is_hovered, 0.14);

            // Preview, fitted into the card keeping the window's shape.
            let area = Rect::from_min_size(card.min, vec2(card.width(), card.width() / 1.6));
            let frame = shot_rect(&self.shots[i]);
            let s = (area.width() / frame.width()).min(area.height() / frame.height());
            let preview = Rect::from_center_size(area.center(), frame.size() * s * (1.0 + 0.035 * h));
            fg.add(
                egui::Shadow {
                    offset: [0, 10],
                    blur: 30,
                    spread: 0,
                    color: Color32::from_black_alpha((120.0 * a) as u8),
                }
                .as_shape(preview, 8),
            );
            let texture = match (&self.thumbs[i], &self.shots[i].thumb) {
                (Some(t), _) => Some(t.id()),
                (None, Some(img)) => {
                    let t = load_texture(ctx, "window-thumb", img);
                    let id = t.id();
                    self.thumbs[i] = Some(t);
                    Some(id)
                }
                (None, None) => None,
            };
            match texture {
                Some(tex) => {
                    fg.image(
                        tex,
                        preview,
                        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                        Color32::WHITE.gamma_multiply(a),
                    );
                }
                None => {
                    fg.rect_filled(preview, 8, Color32::from_white_alpha((24.0 * a) as u8));
                    paint::icon(fg, Icon::Window, preview.center(), TEXT_DIM.gamma_multiply(a));
                }
            }
            let ring = if h > 0.0 { ACCENT.gamma_multiply(a * h) } else { Color32::from_white_alpha((40.0 * a) as u8) };
            fg.rect_stroke(preview.expand(h * 3.0), 8, Stroke::new(1.0 + 1.5 * h, ring), StrokeKind::Outside);

            // Title under the preview, cut with an ellipsis.
            let shot = &self.shots[i];
            let label = if shot.title.is_empty() { shot.app.clone() } else { shot.title.clone() };
            let color = paint::lerp(TEXT_DIM, TEXT, 0.6 + 0.4 * h).gamma_multiply(a);
            let mut job = egui::text::LayoutJob::simple_singleline(label, FontId::proportional(13.0), color);
            job.wrap = egui::text::TextWrapping {
                max_width: card.width() - 8.0,
                max_rows: 1,
                break_anywhere: true,
                overflow_character: Some('…'),
            };
            let galley = fg.layout_job(job);
            let y = area.bottom() + 12.0;
            fg.galley(pos2(card.center().x - galley.size().x / 2.0, y), galley, color);
            if shot.focused {
                fg.circle_filled(pos2(card.center().x, y + 24.0), 2.5, ACCENT.gamma_multiply(a));
            }
        }
        if input.pos.is_some() {
            self.picker_hover = hovered;
        }
    }

    fn paint_text_draft(&self, ctx: &egui::Context, painter: &Painter, offset: egui::Vec2, now: f64) {
        let Some(d) = &self.text else { return };
        let pos = d.pos + offset;
        let galley = paint::text_galley(painter, &d.text, d.color, d.size);
        let row_h = paint::text_galley(painter, "Ag", d.color, d.size).size().y;
        let lines = d.text.split('\n').count().max(1);
        let last = d.text.rsplit('\n').next().unwrap_or("");
        let last_w = if last.is_empty() { 0.0 } else { paint::text_galley(painter, last, d.color, d.size).size().x };
        let size = vec2(galley.size().x.max(last_w).max(2.0), row_h * lines as f32);

        let frame = Rect::from_min_size(pos, size).expand2(vec2(6.0, 3.0));
        paint::dashed_rect(painter, frame, Stroke::new(1.0, Color32::from_white_alpha(170)));
        painter.galley(pos, galley, d.color);

        let blink = ((now - d.started) * 1.8) as i64 % 2 == 0;
        if blink {
            let x = pos.x + last_w + 1.5;
            let y = pos.y + row_h * (lines - 1) as f32;
            painter.line_segment([pos2(x, y + row_h * 0.12), pos2(x, y + row_h * 0.88)], Stroke::new(2.0, d.color));
        }
        ctx.request_repaint_after(Duration::from_millis(120));
    }

    /// Draws the tool bars. Returns `Some(hovering_a_button)` when the pointer is over them.
    #[allow(clippy::too_many_arguments)]
    fn paint_chrome(
        &mut self,
        ctx: &egui::Context,
        fg: &Painter,
        idx: usize,
        rect: Rect,
        now: f64,
        input: PointerIn,
        alpha: f32,
    ) -> Option<bool> {
        let chrome = self.chrome_rects(idx, rect);
        let show = chrome.is_some() && self.closing.is_none();
        let vis = ctx.animate_bool_with_time_and_easing(Id::unique(("chrome", idx)), show, 0.22, easing::cubic_out);
        let chrome = chrome?;
        let a = vis * alpha;
        if a <= 0.0 {
            return None;
        }
        let chrome = chrome.translate(vec2(0.0, (1.0 - vis) * 8.0));
        let interactive = vis > 0.6 && self.closing.is_none();

        if !chrome.video {
            paint::panel(fg, chrome.vbar, 12, a);
        }
        paint::panel(fg, chrome.hbar, 12, a);
        if chrome.video {
            self.paint_record_caption(fg, &chrome, rect, a);
        }

        // Sliding highlight behind the active tool.
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let hl = ctx.animate_bool_with_time(Id::unique("tool-highlight"), self.tool.is_some(), 0.15);
        if let Some(tool) = self.tool {
            let target = items(chrome.vbar, &VBAR, true)
                .find(|(i, _)| *i == Item::Tool(tool))
                .map(|(_, r)| r.min.y - chrome.vbar.min.y)
                .unwrap_or(0.0);
            let v = self.highlight.get_or_insert(target);
            *v += (target - *v) * (1.0 - (-dt * 22.0).exp());
            if (target - *v).abs() > 0.3 {
                ctx.request_repaint();
            } else {
                *v = target;
            }
        }
        if let (Some(off), true, false) = (self.highlight, hl > 0.0, chrome.video) {
            let r = Rect::from_min_size(chrome.vbar.min + vec2(PAD, off), vec2(BTN, BTN)).shrink(1.0);
            fg.rect_filled(r, 9, ACCENT.gamma_multiply(0.2 * a * hl));
        }

        let mut clicked = None;
        let mut hovered_item = None;
        let vbar: &[Item] = if chrome.video { &[] } else { &VBAR };
        let bars = [(chrome.vbar, vbar, true), (chrome.hbar, Chrome::hbar_items(chrome.video), false)];
        for (bar, list, vertical) in bars {
            for (item, r) in items(bar, list, vertical) {
                if item == Item::Sep {
                    let line = if vertical {
                        [pos2(r.left() + 8.0, r.center().y), pos2(r.right() - 8.0, r.center().y)]
                    } else {
                        [pos2(r.center().x, r.top() + 8.0), pos2(r.center().x, r.bottom() - 8.0)]
                    };
                    fg.line_segment(line, Stroke::new(1.0, Color32::from_white_alpha((28.0 * a) as u8)));
                    continue;
                }
                let (click, hover) = self.button(ctx, fg, item, r, a, input, interactive, idx);
                if click {
                    clicked = Some(item);
                }
                if hover {
                    hovered_item = Some((item, r, vertical));
                }
            }
        }

        let palette_hover = !chrome.video && self.paint_palette(ctx, fg, &chrome, a, input, &mut clicked);

        if let Some((item, r, vertical)) = hovered_item {
            let t = self.tooltip_progress(ctx, idx, item, now);
            if t > 0.0 && !(item == Item::Color && self.palette_open) {
                let (bar, prefer) =
                    if vertical { (chrome.vbar, chrome.opens_right) } else { (chrome.hbar, chrome.hbar_below) };
                self.paint_tooltip(fg, item, r, vertical, bar, prefer, rect, a * t);
            }
        }

        if let Some(item) = clicked {
            if item != Item::Color {
                self.palette_open = false;
            }
            match item {
                Item::Tool(t) => self.toggle_tool(t),
                Item::Color => self.palette_open = !self.palette_open,
                Item::Undo => self.undo(),
                Item::Redo => self.redo(),
                Item::Save => self.start_export(Action::Save, idx, now),
                Item::Copy => self.start_export(Action::Copy, idx, now),
                Item::Close => self.close(now),
                Item::Record => self.start_record(now),
                Item::Mode(_) | Item::Kind(_) | Item::Pointer | Item::Settings | Item::Sep => {}
            }
        }

        let over = input.pos.is_some_and(|p| chrome.hit(p, self.palette_open));
        over.then_some(hovered_item.is_some() || palette_hover)
    }

    fn modebar_visible(&self) -> bool {
        matches!(self.drag, Drag::None | Drag::Swallow)
            && self.closing.is_none()
            && self.export.is_none()
            && self.countdown.is_none()
    }

    /// Fade-in progress of the tooltip for `item`, which has been hovered on screen `idx`.
    fn tooltip_progress(&mut self, ctx: &egui::Context, idx: usize, item: Item, now: f64) -> f32 {
        let since = match self.hover {
            Some((i, h, t)) if i == idx && h == item => t,
            _ => {
                self.hover = Some((idx, item, now));
                now
            }
        };
        let t = ((now - since - 0.45) / 0.12) as f32;
        if t < 1.0 {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        paint::ease_out(t)
    }

    /// GNOME-style switcher: area mode, screenshot/screencast, pointer.
    /// Returns `Some(hovering_a_button)` when the pointer is over it.
    #[allow(clippy::too_many_arguments)]
    fn paint_modebar(
        &mut self,
        ctx: &egui::Context,
        fg: &Painter,
        idx: usize,
        rect: Rect,
        now: f64,
        input: PointerIn,
        alpha: f32,
    ) -> Option<bool> {
        let vis = ctx.animate_bool_with_time_and_easing(
            Id::unique(("modebar", idx)),
            self.modebar_visible(),
            0.2,
            easing::cubic_out,
        );
        let a = vis * alpha;
        if a <= 0.0 {
            return None;
        }
        let bar = modebar_rect(rect).translate(vec2(0.0, (vis - 1.0) * 8.0));
        let interactive = vis > 0.6;
        paint::panel(fg, bar, 12, a);

        // Sliding pills behind the current area mode and capture kind.
        let list: Vec<(Item, Rect)> = items(bar, &MODEBAR, false).collect();
        let rect_of = |item: Item| list.iter().find(|(i, _)| *i == item).map(|(_, r)| *r).unwrap();
        for (salt, current) in [("area", Item::Mode(self.area)), ("kind", Item::Kind(self.kind))] {
            let target = rect_of(current);
            let x = ctx.animate_value_with_time(Id::unique(("pill", salt, idx)), target.min.x - bar.min.x, 0.16);
            let pill = Rect::from_min_size(pos2(bar.min.x + x, target.min.y), target.size()).shrink(1.0);
            fg.rect_filled(pill, 9, Color32::from_white_alpha((26.0 * a) as u8));
        }

        let mut clicked = None;
        let mut hovered = None;
        for (item, r) in list.iter().copied() {
            if item == Item::Sep {
                let line = [pos2(r.center().x, r.top() + 8.0), pos2(r.center().x, r.bottom() - 8.0)];
                fg.line_segment(line, Stroke::new(1.0, Color32::from_white_alpha((28.0 * a) as u8)));
                continue;
            }
            let (click, hover) = self.button(ctx, fg, item, r, a, input, interactive, idx);
            if click {
                clicked = Some(item);
            }
            if hover {
                hovered = Some((item, r));
            }
        }

        if let Some((item, r)) = hovered {
            let t = self.tooltip_progress(ctx, idx, item, now);
            if t > 0.0 {
                self.paint_tooltip(fg, item, r, false, bar, true, rect, a * t);
            }
        }
        match clicked {
            Some(Item::Mode(area)) => self.set_area(area),
            Some(Item::Kind(kind)) => self.set_kind(kind),
            Some(Item::Pointer) => self.pointer = !self.pointer,
            Some(Item::Settings) => self.open_settings(now),
            _ => {}
        }

        let over = interactive && input.pos.is_some_and(|p| bar.contains(p));
        over.then_some(hovered.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        ctx: &egui::Context,
        fg: &Painter,
        item: Item,
        r: Rect,
        a: f32,
        input: PointerIn,
        interactive: bool,
        screen: usize,
    ) -> (bool, bool) {
        let enabled = match item {
            Item::Undo => !self.annotations.is_empty() || self.text.is_some(),
            Item::Redo => !self.redo.is_empty(),
            Item::Mode(Area::Window) => self.has_window(),
            Item::Pointer => self.video() || self.screens.iter().any(|s| s.pointer_image.is_some()),
            _ => true,
        };
        let idle = matches!(self.drag, Drag::None | Drag::Swallow);
        let hovered = interactive && idle && input.pos.is_some_and(|p| r.contains(p));
        let id = Id::unique(("btn", item, screen));
        let h = ctx.animate_bool_with_time(id, hovered && enabled, 0.12);
        let pressing = hovered && input.down && input.origin.is_some_and(|o| r.contains(o));
        let scale = ctx.animate_value_with_time(id.with("press"), if pressing { 0.9 } else { 1.0 }, 0.08);

        let bg = Rect::from_center_size(r.center(), r.size() * scale).shrink(1.0);
        if item == Item::Record {
            let red = paint::lerp(Color32::from_rgb(235, 52, 52), Color32::from_rgb(255, 82, 82), h);
            fg.rect_filled(bg.shrink2(vec2(2.0, 0.0)), 9, red.gamma_multiply(a));
            let dot = pos2(bg.left() + 22.0, bg.center().y);
            fg.circle_filled(dot, 5.0, Color32::WHITE.gamma_multiply(a));
            let text = if self.kind == Kind::Gif { t("record.button_gif") } else { t("record.button") };
            let label = fg.layout_no_wrap(text.into(), FontId::proportional(14.0), Color32::WHITE.gamma_multiply(a));
            fg.galley(pos2(dot.x + 12.0, bg.center().y - label.size().y / 2.0), label, Color32::WHITE);
        } else if h > 0.0 {
            let fill = if item == Item::Close {
                Color32::from_rgba_unmultiplied(255, 69, 58, (46.0 * h * a) as u8)
            } else {
                Color32::from_white_alpha((20.0 * h * a) as u8)
            };
            fg.rect_filled(bg, 9, fill);
        }

        let active = matches!(item, Item::Tool(t) if self.tool == Some(t))
            || (item == Item::Color && self.palette_open)
            || (item == Item::Pointer && self.pointer)
            || item == Item::Mode(self.area)
            || item == Item::Kind(self.kind);
        let base = if active {
            ACCENT
        } else if item == Item::Close && h > 0.0 {
            paint::lerp(Color32::from_gray(205), Color32::from_rgb(255, 110, 100), h)
        } else {
            paint::lerp(Color32::from_gray(205), Color32::WHITE, h)
        };
        let color = base.gamma_multiply(a * if enabled { 1.0 } else { 0.3 });

        match item {
            Item::Color => {
                let c = r.center();
                let rad = 8.0 * scale;
                fg.circle_filled(c, rad + 2.5, Color32::from_white_alpha((200.0 * a) as u8));
                fg.circle_filled(c, rad, self.color.gamma_multiply(a));
                fg.circle_stroke(c, rad, Stroke::new(1.0, Color32::from_black_alpha((60.0 * a) as u8)));
            }
            Item::Tool(t) => paint::icon(fg, t.icon(), r.center(), color),
            Item::Undo => paint::icon(fg, Icon::Undo, r.center(), color),
            Item::Redo => paint::icon(fg, Icon::Redo, r.center(), color),
            Item::Save => paint::icon(fg, Icon::Save, r.center(), color),
            Item::Copy => paint::icon(fg, Icon::Copy, r.center(), color),
            Item::Close => paint::icon(fg, Icon::Close, r.center(), color),
            Item::Mode(area) => {
                let (icon, label) = match area {
                    Area::Selection => (Icon::Selection, t("mode.selection")),
                    Area::Screen => (Icon::Screen, t("mode.screen")),
                    Area::Window => (Icon::Window, t("mode.window")),
                };
                let ic = pos2(r.left() + 20.0, r.center().y);
                paint::icon(fg, icon, ic, color);
                let g = fg.layout_no_wrap(label.into(), FontId::proportional(13.0), color);
                fg.galley(pos2(ic.x + 15.0, r.center().y - g.size().y / 2.0), g, color);
            }
            Item::Kind(Kind::Photo) => paint::icon(fg, Icon::Photo, r.center(), color),
            Item::Kind(Kind::Video) => paint::icon(fg, Icon::Video, r.center(), color),
            Item::Kind(Kind::Gif) => paint::icon(fg, Icon::Gif, r.center(), color),
            Item::Pointer => paint::icon(fg, Icon::Pointer, r.center(), color),
            Item::Settings => paint::icon(fg, Icon::Gear, r.center(), color),
            Item::Record | Item::Sep => {}
        }

        let clicked = interactive
            && enabled
            && input.released
            && input.origin.is_some_and(|o| r.contains(o))
            && input.pos.is_some_and(|p| r.contains(p));
        (clicked, hovered)
    }

    /// Colour swatches and size presets. Returns whether a cell is hovered.
    fn paint_palette(
        &mut self,
        ctx: &egui::Context,
        fg: &Painter,
        chrome: &Chrome,
        alpha: f32,
        input: PointerIn,
        clicked: &mut Option<Item>,
    ) -> bool {
        let t =
            ctx.animate_bool_with_time_and_easing(Id::unique("palette"), self.palette_open, 0.18, easing::cubic_out);
        if t <= 0.0 {
            return false;
        }
        let a = alpha * t;
        // Grow out of the edge that faces the tool bar.
        let anchor_x = if chrome.opens_right { chrome.palette.left() } else { chrome.palette.right() };
        let s = 0.94 + 0.06 * t;
        let map = |p: Pos2| {
            pos2(anchor_x + (p.x - anchor_x) * s, chrome.palette.center().y + (p.y - chrome.palette.center().y) * s)
        };
        let panel = Rect::from_min_max(map(chrome.palette.min), map(chrome.palette.max));
        paint::panel(fg, panel, 14, a);

        let interactive = self.palette_open && t > 0.5;
        let mut any_hover = false;
        let mut hover = |r: Rect| {
            let h = interactive && input.pos.is_some_and(|p| r.contains(p));
            any_hover |= h;
            h
        };
        let click = |r: Rect| {
            interactive
                && input.released
                && input.origin.is_some_and(|o| r.contains(o))
                && input.pos.is_some_and(|p| r.contains(p))
        };

        let origin = chrome.palette.min + vec2(POP_PAD, POP_PAD);
        let mut picked_color = None;
        for (i, c) in PALETTE.iter().enumerate() {
            let (col, row) = (i % 5, i / 5);
            let cell = Rect::from_min_size(
                origin + vec2(col as f32 * (SWATCH + SWATCH_GAP), row as f32 * (SWATCH + SWATCH_GAP)),
                vec2(SWATCH, SWATCH),
            );
            let cell = Rect::from_min_max(map(cell.min), map(cell.max));
            let h = ctx.animate_bool_with_time(Id::unique(("swatch", i)), hover(cell), 0.1);
            let selected = *c == self.color;
            let rad = SWATCH / 2.0 - 3.0 + h * 1.5;
            if selected {
                fg.circle_stroke(cell.center(), SWATCH / 2.0 + 0.5, Stroke::new(2.0, Color32::WHITE.gamma_multiply(a)));
            }
            fg.circle_filled(cell.center(), rad, c.gamma_multiply(a));
            fg.circle_stroke(cell.center(), rad, Stroke::new(1.0, Color32::from_white_alpha((30.0 * a) as u8)));
            if click(cell) {
                picked_color = Some(*c);
            }
        }

        let sep_y = origin.y + 2.0 * SWATCH + SWATCH_GAP + POP_PAD;
        fg.line_segment(
            [map(pos2(chrome.palette.left() + POP_PAD, sep_y)), map(pos2(chrome.palette.right() - POP_PAD, sep_y))],
            Stroke::new(1.0, Color32::from_white_alpha((26.0 * a) as u8)),
        );

        let tool = self.size_tool();
        let row_top = sep_y + 1.0 + POP_PAD;
        let inner_w = chrome.palette.width() - 2.0 * POP_PAD;
        let mut picked_size = None;
        for (i, preset) in tool.presets().into_iter().enumerate() {
            let cell = Rect::from_min_size(
                pos2(chrome.palette.left() + POP_PAD + i as f32 * inner_w / 4.0, row_top),
                vec2(inner_w / 4.0, SWATCH),
            );
            let cell = Rect::from_min_max(map(cell.min), map(cell.max));
            let h = ctx.animate_bool_with_time(Id::unique(("size", i)), hover(cell), 0.1);
            let selected = (self.size() - preset).abs() < 0.5;
            if selected || h > 0.0 {
                let bg = if selected {
                    ACCENT.gamma_multiply(0.22 * a)
                } else {
                    Color32::from_white_alpha((18.0 * h * a) as u8)
                };
                fg.rect_filled(cell.shrink2(vec2(3.0, 0.0)), 7, bg);
            }
            let fill = if selected { ACCENT } else { Color32::from_gray(215) }.gamma_multiply(a);
            if tool == Tool::Text {
                let g = fg.layout_no_wrap("A".into(), FontId::proportional(9.0 + i as f32 * 3.5), fill);
                fg.galley(cell.center() - g.size() / 2.0, g, fill);
            } else {
                fg.circle_filled(cell.center(), 1.5 + i as f32 * 1.8, fill);
            }
            if click(cell) {
                picked_size = Some(preset);
            }
        }

        if let Some(c) = picked_color {
            self.set_color(c);
            self.palette_open = false;
            *clicked = None;
        }
        if let Some(s) = picked_size {
            self.set_size(s);
            self.palette_open = false;
        }
        any_hover
    }

    #[allow(clippy::too_many_arguments)]
    /// `prefer` is "to the right" for vertical bars and "below" for horizontal ones.
    #[allow(clippy::too_many_arguments)]
    fn paint_tooltip(
        &self,
        fg: &Painter,
        item: Item,
        anchor: Rect,
        vertical: bool,
        bar: Rect,
        prefer: bool,
        screen: Rect,
        a: f32,
    ) {
        let (title, shortcut) = item.tooltip();
        let title = fg.layout_no_wrap(title, FontId::proportional(13.0), TEXT.gamma_multiply(a));
        let key = fg.layout_no_wrap(shortcut.into(), FontId::proportional(11.5), TEXT_DIM.gamma_multiply(a));
        let key_w = if shortcut.is_empty() { 0.0 } else { key.size().x + 12.0 + 8.0 };
        let size = vec2(title.size().x + key_w + 20.0, 28.0);

        let pos = if vertical {
            let gap = GAP + PAD;
            let right = pos2(bar.right() + gap, anchor.center().y - size.y / 2.0);
            let left = pos2(bar.left() - gap - size.x, anchor.center().y - size.y / 2.0);
            if (prefer && right.x + size.x < screen.right()) || left.x < screen.left() {
                right
            } else {
                left
            }
        } else {
            let below = pos2(anchor.center().x - size.x / 2.0, bar.bottom() + 8.0);
            let above = pos2(anchor.center().x - size.x / 2.0, bar.top() - 8.0 - size.y);
            if (prefer && below.y + size.y < screen.bottom()) || above.y < screen.top() {
                below
            } else {
                above
            }
        };
        let pos = pos2(pos.x.min(screen.right() - size.x - 4.0).max(screen.left() + 4.0), pos.y);
        let r = Rect::from_min_size(pos, size);
        paint::panel(fg, r, 8, a);
        fg.galley(pos2(r.left() + 10.0, r.center().y - title.size().y / 2.0), title.clone(), TEXT);
        if !shortcut.is_empty() {
            let kr = Rect::from_min_size(
                pos2(r.left() + 10.0 + title.size().x + 8.0, r.center().y - 9.0),
                vec2(key.size().x + 12.0, 18.0),
            );
            fg.rect_filled(kr, 5, Color32::from_white_alpha((16.0 * a) as u8));
            fg.galley(kr.center() - key.size() / 2.0, key, TEXT_DIM);
        }
    }

    /// Screencast mode: says next to the Record button how a recording ends.
    fn paint_record_caption(&self, fg: &Painter, chrome: &Chrome, rect: Rect, a: f32) {
        let key = self.stop_key();
        let before =
            fg.layout_no_wrap(t("record.stop_with").into(), FontId::proportional(13.0), TEXT_DIM.gamma_multiply(a));
        let cap = fg.layout_no_wrap(key, FontId::proportional(12.0), TEXT.gamma_multiply(a));
        let w = 14.0 + before.size().x + 8.0 + cap.size().x + 14.0 + 14.0;
        let h = chrome.hbar.height();
        let left = pos2(chrome.hbar.left() - GAP - w, chrome.hbar.top());
        let right = pos2(chrome.hbar.right() + GAP, chrome.hbar.top());
        let at = if left.x >= rect.left() + MARGIN {
            left
        } else if right.x + w <= rect.right() - MARGIN {
            right
        } else {
            return;
        };
        let r = Rect::from_min_size(at, vec2(w, h));
        paint::panel(fg, r, 12, a);
        let y = r.center().y;
        fg.galley(pos2(r.left() + 14.0, y - before.size().y / 2.0), before.clone(), TEXT_DIM);
        let kr = Rect::from_min_size(
            pos2(r.left() + 14.0 + before.size().x + 8.0, y - 11.0),
            vec2(cap.size().x + 14.0, 22.0),
        );
        fg.rect_filled(kr, 5, Color32::from_white_alpha((24.0 * a) as u8));
        fg.galley(kr.center() - cap.size() / 2.0, cap, TEXT);
    }

    fn paint_size_flash(&mut self, ctx: &egui::Context, fg: &Painter, idx: usize, now: f64) {
        let Some((t, p)) = self.size_flash else { return };
        if !self.screens[idx].global().contains(p) {
            return;
        }
        let age = (now - t) as f32;
        if age > 0.9 {
            self.size_flash = None;
            return;
        }
        ctx.request_repaint();
        let p = p + self.screens[idx].offset();
        let a = 1.0 - paint::ease_out((age - 0.5) / 0.4);
        let size = self.size();
        let r = match self.size_tool() {
            Tool::Text => size * 0.5,
            _ => size / 2.0,
        }
        .max(2.0);
        fg.circle_filled(p, r, self.color.gamma_multiply(0.35 * a));
        paint::ring(fg, p, r, Stroke::new(1.5, Color32::WHITE.gamma_multiply(a)));
        let label =
            fg.layout_no_wrap(format!("{} px", size.round()), FontId::proportional(12.0), TEXT.gamma_multiply(a));
        let br = Rect::from_min_size(p + vec2(r + 10.0, -11.0), label.size() + vec2(14.0, 8.0));
        paint::panel(fg, br, 6, a);
        fg.galley(br.min + vec2(7.0, 4.0), label, TEXT);
    }

    /// Camera-flash over the captured area as the windows go away.
    fn paint_flash(&self, fg: &Painter, idx: usize, now: f64) {
        let (Some(c), Some(sel)) = (&self.closing, self.selection) else { return };
        if !c.export {
            return;
        }
        let f = 1.0 - paint::ease_out(((now - c.started) / FADE_OUT) as f32);
        if f > 0.0 {
            fg.rect_filled(sel.translate(self.screens[idx].offset()), 0, Color32::from_white_alpha((80.0 * f) as u8));
        }
    }

    /// "Recording starts in 3, 2, 1", with how to stop it.
    fn paint_countdown(&self, ctx: &egui::Context, fg: &Painter, idx: usize, rect: Rect, now: f64) {
        let (Some(c), Some(sel)) = (&self.countdown, self.selection) else { return };
        if self.owner() != Some(idx) {
            return;
        }
        ctx.request_repaint();
        let elapsed = now - c.started;
        let steps = self.countdown_steps().max(1);
        let step = ((elapsed / COUNTDOWN_STEP) as u32).min(steps - 1);
        let phase = ((elapsed - step as f64 * COUNTDOWN_STEP) / COUNTDOWN_STEP) as f32;
        let a = paint::ease_out((elapsed / 0.2) as f32);

        let title = if self.kind == Kind::Gif { t("record.countdown_title_gif") } else { t("record.countdown_title") };
        let title = fg.layout_no_wrap(title.into(), FontId::proportional(15.0), TEXT.gamma_multiply(a));
        let stop1 =
            fg.layout_no_wrap(t("record.to_stop").into(), FontId::proportional(13.0), TEXT_DIM.gamma_multiply(a));
        let key = fg.layout_no_wrap(self.stop_key(), FontId::proportional(12.5), TEXT.gamma_multiply(a));
        let stop2 = fg.layout_no_wrap(t("record.again").into(), FontId::proportional(13.0), TEXT_DIM.gamma_multiply(a));
        let esc = fg.layout_no_wrap("Esc".into(), FontId::proportional(11.5), TEXT.gamma_multiply(a));
        let cancel =
            fg.layout_no_wrap(t("record.cancel").into(), FontId::proportional(12.0), TEXT_DIM.gamma_multiply(a));
        let stop_w = stop1.size().x + 8.0 + key.size().x + 14.0 + 8.0 + stop2.size().x;
        let size = vec2(title.size().x.max(stop_w) + 56.0, 196.0);

        let local = sel.translate(self.screens[idx].offset()).intersect(rect);
        let bounds = rect.shrink2(size / 2.0 + vec2(MARGIN, MARGIN));
        let center = pos2(
            local.center().x.min(bounds.right()).max(bounds.left()),
            local.center().y.min(bounds.bottom()).max(bounds.top()),
        );
        let card = Rect::from_center_size(center, size * (0.96 + 0.04 * a));
        paint::panel(fg, card, 18, a);

        // Number inside a ring that drains with each second.
        let ring_c = pos2(card.center().x, card.top() + 54.0);
        let ring_r = 30.0;
        paint::ring(fg, ring_c, ring_r, Stroke::new(3.0, Color32::from_white_alpha((28.0 * a) as u8)));
        let sweep = (1.0 - phase) * std::f32::consts::TAU;
        let arc: Vec<Pos2> = (0..=48)
            .map(|i| {
                let t = -std::f32::consts::FRAC_PI_2 + sweep * i as f32 / 48.0;
                ring_c + vec2(t.cos(), t.sin()) * ring_r
            })
            .collect();
        fg.add(egui::Shape::line(arc, Stroke::new(3.0, Color32::from_rgb(255, 69, 58).gamma_multiply(a))));
        let pop = 0.8 + 0.2 * paint::ease_out_back(phase * 4.0);
        let n = fg.layout_no_wrap((steps - step).to_string(), FontId::proportional(30.0 * pop), TEXT.gamma_multiply(a));
        fg.galley(ring_c - n.size() / 2.0, n, TEXT);

        let mut y = card.top() + 100.0;
        fg.galley(pos2(card.center().x - title.size().x / 2.0, y), title.clone(), TEXT);
        y += title.size().y + 14.0;

        let mut x = card.center().x - stop_w / 2.0;
        fg.galley(pos2(x, y + 11.0 - stop1.size().y / 2.0), stop1.clone(), TEXT_DIM);
        x += stop1.size().x + 8.0;
        let kr = Rect::from_min_size(pos2(x, y), vec2(key.size().x + 14.0, 22.0));
        fg.rect_filled(kr, 5, Color32::from_white_alpha((24.0 * a) as u8));
        fg.galley(kr.center() - key.size() / 2.0, key, TEXT);
        x = kr.right() + 8.0;
        fg.galley(pos2(x, y + 11.0 - stop2.size().y / 2.0), stop2, TEXT_DIM);
        y += 22.0 + 12.0;

        let w = esc.size().x + 12.0 + 6.0 + cancel.size().x;
        let er = Rect::from_min_size(pos2(card.center().x - w / 2.0, y), vec2(esc.size().x + 12.0, 20.0));
        fg.rect_filled(er, 5, Color32::from_white_alpha((18.0 * a) as u8));
        fg.galley(er.center() - esc.size() / 2.0, esc, TEXT);
        fg.galley(pos2(er.right() + 6.0, er.center().y - cancel.size().y / 2.0), cancel, TEXT_DIM);
    }
}

fn viewport_id(idx: usize) -> ViewportId {
    if idx == 0 {
        ViewportId::ROOT
    } else {
        ViewportId::from_hash_of(("queijo-screen", idx))
    }
}

/// The app behind a lock: every monitor's window is a *deferred* viewport whose
/// callback locks it. (eframe never answers a screenshot request made from an
/// immediate viewport, which silently dropped the drawings on second monitors.)
pub struct Shared(Arc<Mutex<QueijoApp>>);

impl Shared {
    pub fn new(app: QueijoApp) -> Self {
        Self(Arc::new(Mutex::new(app)))
    }
}

fn lock(app: &Mutex<QueijoApp>) -> std::sync::MutexGuard<'_, QueijoApp> {
    app.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl eframe::App for Shared {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        lock(&self.0).root_frame(ui, &self.0);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The windows only dim the live screen; nothing is opaque but our own UI.
        // Without a compositor they are opaque, and black is what is behind the picture.
        let opaque = lock(&self.0).frozen;
        [0.0, 0.0, 0.0, if opaque { 1.0 } else { 0.0 }]
    }
}

impl QueijoApp {
    fn root_frame(&mut self, ui: &mut egui::Ui, shared: &Arc<Mutex<QueijoApp>>) {
        let ctx = ui.ctx().clone();
        let mut arrived = false;
        // Nothing is on screen before the first frame, so the focused window is still the user's.
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        while !self.window_taken.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        if self.capture_job.as_ref().is_some_and(|j| j.is_finished()) {
            let result = self.capture_job.take().unwrap().join().unwrap_or_else(|_| Err("capture panicked".into()));
            match result {
                Ok(captured) => self.set_captured(captured),
                Err(e) => {
                    let message = crate::i18n::tf("error.capture", &[("error", &e)]);
                    eprintln!("queijo: {message}");
                    output::notify_error("Queijo", &message);
                    std::process::exit(1);
                }
            }
            arrived = true;
        }
        // While our windows are still fully transparent, a capture with the
        // pointer is the desktop plus the cursor.
        if let Some(job) = self.pointer_job.take() {
            let requested = self.pointer_requested.clone();
            self.pointer_thread = Some(std::thread::spawn(move || job(&requested)));
            self.hold_until = Some(f64::INFINITY);
        }
        let now = ctx.input(|i| i.time);
        if self.hold_until == Some(f64::INFINITY) && self.pointer_requested.load(Ordering::Acquire) {
            self.hold_until = Some(now + 0.08);
        }
        if self.pointer_thread.as_ref().is_some_and(|t| t.is_finished()) {
            if let Ok(images) = self.pointer_thread.take().unwrap().join() {
                for (screen, img) in self.screens.iter_mut().zip(images) {
                    screen.pointer_image = img;
                }
            }
            arrived = true;
        } else if self.pointer_thread.is_some() {
            ctx.request_repaint_after(Duration::from_millis(15));
        }
        if self.shots_job.as_ref().is_some_and(|j| j.is_finished()) {
            self.shots = self.shots_job.take().unwrap().join().unwrap_or_default();
            self.fill_thumbs();
            arrived = true;
        } else if self.shots_job.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        self.tick_countdown(&ctx, now);

        self.screen_ui(ui, 0);
        for idx in 1..self.screens.len() {
            let builder = self.screens[idx].viewport.clone();
            let shared = shared.clone();
            ctx.show_viewport_deferred(viewport_id(idx), builder, move |ui, _| lock(&shared).screen_ui(ui, idx));
        }
        if arrived || self.capture_job.is_some() || self.shots_job.is_some() || self.pointer_thread.is_some() {
            self.wake_others(&ctx, 0);
        }
    }

    /// A window the desktop gave no picture of gets its preview cut from the screen's picture.
    /// Whichever of the two arrives second does it.
    fn fill_thumbs(&mut self) {
        if self.desktop.width() == 0 {
            return;
        }
        let (origin, width) =
            ((self.desktop_rect.min.x as i32, self.desktop_rect.min.y as i32), self.desktop_rect.width());
        for shot in self.shots.iter_mut().filter(|s| s.thumb.is_none() && s.image.is_none()) {
            shot.thumb = windows::thumb_from_desktop(&self.desktop, origin, width, shot.frame);
        }
    }

    fn set_captured(&mut self, captured: Captured) {
        for (screen, image) in self.screens.iter_mut().zip(captured.images) {
            screen.image = image;
            screen.texture = None;
        }
        self.desktop = captured.desktop;
        self.fill_thumbs();
        self.window =
            captured.window.map(|[x, y, w, h]| Rect::from_min_size(pos2(x as f32, y as f32), vec2(w as f32, h as f32)));
        self.pointer_job = captured.pointer_job;
    }

    /// Windows don't repaint when state changes in another window; this does it.
    fn wake_others(&self, ctx: &egui::Context, current: usize) {
        for idx in (0..self.screens.len()).filter(|&i| i != current) {
            ctx.request_repaint_of(viewport_id(idx));
        }
    }
}

/// Fills `rect` around the `hole`.
fn paint_around(painter: &Painter, rect: Rect, hole: Rect, color: Color32) {
    painter.rect_filled(Rect::from_min_max(rect.min, pos2(rect.max.x, hole.min.y)), 0, color);
    painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, hole.max.y), rect.max), 0, color);
    painter.rect_filled(Rect::from_min_max(pos2(rect.min.x, hole.min.y), pos2(hole.min.x, hole.max.y)), 0, color);
    painter.rect_filled(Rect::from_min_max(pos2(hole.max.x, hole.min.y), pos2(rect.max.x, hole.max.y)), 0, color);
}

fn handle_points(r: Rect) -> [Pos2; 8] {
    [
        r.left_top(),
        r.center_top(),
        r.right_top(),
        r.right_center(),
        r.right_bottom(),
        r.center_bottom(),
        r.left_bottom(),
        r.left_center(),
    ]
}

fn hit_handle(r: Rect, p: Pos2) -> Option<Handle> {
    const CORNER: f32 = 11.0;
    const EDGE: f32 = 6.0;
    let handles = [
        (r.left_top(), Handle::NW),
        (r.right_top(), Handle::NE),
        (r.left_bottom(), Handle::SW),
        (r.right_bottom(), Handle::SE),
        (r.center_top(), Handle::N),
        (r.center_bottom(), Handle::S),
        (r.left_center(), Handle::W),
        (r.right_center(), Handle::E),
    ];
    if let Some((_, h)) = handles.iter().find(|(c, _)| c.distance(p) <= CORNER) {
        return Some(*h);
    }
    let in_x = p.x > r.left() && p.x < r.right();
    let in_y = p.y > r.top() && p.y < r.bottom();
    if in_x && (p.y - r.top()).abs() <= EDGE {
        Some(Handle::N)
    } else if in_x && (p.y - r.bottom()).abs() <= EDGE {
        Some(Handle::S)
    } else if in_y && (p.x - r.left()).abs() <= EDGE {
        Some(Handle::W)
    } else if in_y && (p.x - r.right()).abs() <= EDGE {
        Some(Handle::E)
    } else {
        None
    }
}

fn clamp_into(r: Rect, bounds: Rect) -> Rect {
    let x = r.min.x.clamp(bounds.min.x, (bounds.max.x - r.width()).max(bounds.min.x));
    let y = r.min.y.clamp(bounds.min.y, (bounds.max.y - r.height()).max(bounds.min.y));
    Rect::from_min_size(pos2(x, y), r.size())
}

fn crop_frame(frame: &egui::ColorImage, sel: Rect, screen: Rect) -> RgbaImage {
    let [fw, fh] = frame.size;
    let scale = fw as f32 / screen.width();
    let r = sel.translate(-screen.min.to_vec2());
    let x0 = ((r.min.x * scale).round().max(0.0) as usize).min(fw.saturating_sub(1));
    let y0 = ((r.min.y * scale).round().max(0.0) as usize).min(fh.saturating_sub(1));
    let x1 = ((r.max.x * scale).round().max(0.0) as usize).clamp(x0 + 1, fw);
    let y1 = ((r.max.y * scale).round().max(0.0) as usize).clamp(y0 + 1, fh);
    let mut out = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let [r, g, b, _] = frame.pixels[y * fw + x].to_array();
            out.put_pixel((x - x0) as u32, (y - y0) as u32, image::Rgba([r, g, b, 255]));
        }
    }
    out
}

fn shot_rect(shot: &WindowShot) -> Rect {
    let [x, y, w, h] = shot.frame;
    Rect::from_min_size(pos2(x as f32, y as f32), vec2(w as f32, h as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(width: f32, height: f32) -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(width, height))
    }

    fn selection(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn grabs_the_handles_at_corners_and_edges() {
        let r = selection(100.0, 100.0, 200.0, 100.0);
        let cases = [
            (pos2(100.0, 100.0), Some(Handle::NW)),
            (pos2(300.0, 100.0), Some(Handle::NE)),
            (pos2(100.0, 200.0), Some(Handle::SW)),
            (pos2(300.0, 200.0), Some(Handle::SE)),
            (pos2(200.0, 100.0), Some(Handle::N)),
            (pos2(200.0, 200.0), Some(Handle::S)),
            (pos2(100.0, 150.0), Some(Handle::W)),
            (pos2(300.0, 150.0), Some(Handle::E)),
            (pos2(130.0, 103.0), Some(Handle::N)), // anywhere along an edge
            (pos2(297.0, 170.0), Some(Handle::E)),
            (pos2(105.0, 105.0), Some(Handle::NW)), // a corner wins over the edges next to it
            (pos2(200.0, 150.0), None),             // the middle moves the selection instead
            (pos2(50.0, 50.0), None),
            (pos2(320.0, 150.0), None),
        ];
        for (point, expected) in cases {
            assert_eq!(hit_handle(r, point), expected, "{point:?}");
        }
    }

    #[test]
    fn the_eight_handles_sit_on_the_selection() {
        let r = selection(10.0, 20.0, 100.0, 60.0);
        let points = handle_points(r);
        assert_eq!(points.len(), 8);
        assert!(points.iter().all(|p| (r.left()..=r.right()).contains(&p.x) && (r.top()..=r.bottom()).contains(&p.y)));
        for p in points {
            assert!(hit_handle(r, p).is_some(), "{p:?} can be grabbed");
        }
    }

    #[test]
    fn moving_keeps_the_size_and_stays_inside() {
        let bounds = screen(1000.0, 500.0);
        let r = selection(0.0, 0.0, 200.0, 100.0);
        assert_eq!(clamp_into(r.translate(vec2(-50.0, -20.0)), bounds), r);
        assert_eq!(clamp_into(r.translate(vec2(5000.0, 5000.0)), bounds), selection(800.0, 400.0, 200.0, 100.0));
        let inside = selection(300.0, 200.0, 50.0, 50.0);
        assert_eq!(clamp_into(inside, bounds), inside);
        // Larger than the area: pinned to its corner rather than panicking.
        assert_eq!(clamp_into(selection(5.0, 5.0, 2000.0, 900.0), bounds).min, bounds.min);
    }

    #[test]
    fn cuts_the_selection_out_of_a_rendered_frame() {
        // A 200x100 frame of a 100x50-point window (twice the pixels).
        let pixels = (0..200 * 100).map(|i| Color32::from_rgb((i % 200) as u8, (i / 200) as u8, 7)).collect();
        let frame = egui::ColorImage::new([200, 100], pixels);
        let cut = crop_frame(&frame, selection(10.0, 5.0, 20.0, 20.0), screen(100.0, 50.0));
        assert_eq!(cut.dimensions(), (40, 40));
        assert_eq!(*cut.get_pixel(0, 0), image::Rgba([20, 10, 7, 255]));
        assert_eq!(*cut.get_pixel(39, 39), image::Rgba([59, 49, 7, 255]));

        // Whatever alpha the readback has, the picture is saved opaque.
        let see_through = egui::ColorImage::new([2, 2], vec![Color32::from_rgba_premultiplied(5, 6, 7, 128); 4]);
        let cut = crop_frame(&see_through, screen(2.0, 2.0), screen(2.0, 2.0));
        assert_eq!(*cut.get_pixel(1, 1), image::Rgba([5, 6, 7, 255]));

        // A window that doesn't start at the origin (the second monitor).
        let offset = Rect::from_min_size(pos2(1000.0, 0.0), vec2(100.0, 50.0));
        let cut = crop_frame(&frame, selection(1010.0, 5.0, 20.0, 20.0), offset);
        assert_eq!(*cut.get_pixel(0, 0), image::Rgba([20, 10, 7, 255]));

        // Sticking out of the window is clipped, never out of range.
        let cut = crop_frame(&frame, selection(90.0, 40.0, 50.0, 50.0), screen(100.0, 50.0));
        assert_eq!(cut.dimensions(), (20, 20));
    }

    #[test]
    fn bars_stay_on_screen_and_clear_of_each_other() {
        let display = screen(1920.0, 1080.0);
        for video in [false, true] {
            for (x, y, w, h) in [
                (600.0, 300.0, 400.0, 300.0),  // middle
                (0.0, 0.0, 300.0, 200.0),      // top-left corner
                (1620.0, 880.0, 300.0, 200.0), // bottom-right corner
                (0.0, 0.0, 1920.0, 1080.0),    // the whole screen
                (100.0, 900.0, 800.0, 180.0),  // along the bottom
                (1700.0, 100.0, 220.0, 700.0), // along the right
                (10.0, 10.0, 30.0, 20.0),      // tiny
            ] {
                let sel = selection(x, y, w, h);
                let chrome = Chrome::new(sel, display, video);
                let ok = display.expand(0.5);
                assert!(ok.contains_rect(chrome.hbar), "hbar {:?} for {sel:?}", chrome.hbar);
                if !video {
                    assert!(ok.contains_rect(chrome.vbar), "vbar {:?} for {sel:?}", chrome.vbar);
                    assert!(!chrome.vbar.intersects(chrome.hbar), "bars overlap for {sel:?}");
                }
            }
        }
    }

    #[test]
    fn bars_go_beside_a_selection_when_there_is_room() {
        let display = screen(1920.0, 1080.0);
        let sel = selection(600.0, 300.0, 400.0, 300.0);
        let chrome = Chrome::new(sel, display, false);
        assert!(chrome.vbar.left() > sel.right(), "tools to the right");
        assert!(chrome.hbar.top() > sel.bottom(), "actions below");
        assert!(chrome.opens_right && chrome.hbar_below);

        // Against the right edge the tools flip to the left; at the bottom the actions flip up.
        let corner = Chrome::new(selection(1500.0, 800.0, 420.0, 280.0), display, false);
        assert!(corner.vbar.right() < 1500.0 + 420.0 && !corner.opens_right);
        assert!(corner.hbar.bottom() < 800.0 && !corner.hbar_below);
    }

    #[test]
    fn the_palette_opens_beside_the_tool_bar_on_screen() {
        let display = screen(1920.0, 1080.0);
        let chrome = Chrome::new(selection(600.0, 300.0, 400.0, 300.0), display, false);
        assert!(display.contains_rect(chrome.palette));
        assert!(!chrome.palette.intersects(chrome.vbar));
        assert!(chrome.hit(chrome.vbar.center(), false) && chrome.hit(chrome.hbar.center(), false));
        assert!(!chrome.hit(chrome.palette.center(), false), "a closed palette isn't clickable");
        assert!(chrome.hit(chrome.palette.center(), true));
        let video = Chrome::new(selection(600.0, 300.0, 400.0, 300.0), display, true);
        assert!(!video.hit(video.vbar.center(), false), "no tool bar while recording");
    }

    #[test]
    fn bar_items_are_laid_out_in_order_without_gaps() {
        for (list, vertical) in [(&VBAR[..], true), (&HBAR[..], false), (&HBAR_VIDEO[..], false), (&MODEBAR[..], false)]
        {
            let bar = Rect::from_min_size(
                pos2(100.0, 100.0),
                if vertical { vec2(46.0, bar_len(list)) } else { vec2(bar_len(list), 46.0) },
            );
            let laid: Vec<_> = items(bar, list, vertical).collect();
            assert_eq!(laid.len(), list.len());
            let mut cursor = PAD;
            for (item, r) in laid {
                let (start, len) =
                    if vertical { (r.top() - bar.top(), r.height()) } else { (r.left() - bar.left(), r.width()) };
                assert!((start - cursor).abs() < 1e-3 && (len - item.len()).abs() < 1e-3);
                assert!(bar.expand(0.1).contains_rect(r));
                cursor += len;
            }
            assert!((cursor + PAD - bar_len(list)).abs() < 1e-3);
        }
    }

    #[test]
    fn the_mode_bar_is_centered_at_the_top() {
        let bar = modebar_rect(screen(1920.0, 1080.0));
        assert!((bar.center().x - 960.0).abs() < 0.5);
        assert!(bar.top() > 0.0 && bar.bottom() < 100.0);
        let second = modebar_rect(Rect::from_min_size(pos2(1920.0, 0.0), vec2(1000.0, 800.0)));
        assert!((second.center().x - 2420.0).abs() < 0.5, "centered on its own screen");
    }

    #[test]
    fn every_tool_and_mode_has_a_button() {
        for tool in Tool::ALL {
            assert_eq!(VBAR.iter().filter(|i| **i == Item::Tool(tool)).count(), 1, "{tool:?}");
        }
        for area in [Area::Selection, Area::Screen, Area::Window] {
            assert!(MODEBAR.contains(&Item::Mode(area)), "{area:?}");
        }
        for kind in [Kind::Photo, Kind::Video, Kind::Gif] {
            assert!(MODEBAR.contains(&Item::Kind(kind)), "{kind:?}");
        }
        assert!(MODEBAR.contains(&Item::Settings) && MODEBAR.contains(&Item::Pointer));
        assert!(HBAR.contains(&Item::Save) && HBAR.contains(&Item::Copy) && HBAR.contains(&Item::Close));
        assert!(HBAR_VIDEO.contains(&Item::Record) && !HBAR_VIDEO.contains(&Item::Save));
    }

    #[test]
    fn every_button_has_a_tooltip() {
        let all = VBAR.iter().chain(&HBAR).chain(&HBAR_VIDEO).chain(&MODEBAR);
        for item in all.filter(|i| **i != Item::Sep) {
            assert!(!item.tooltip().0.is_empty(), "{item:?}");
        }
    }

    #[test]
    fn a_screen_maps_between_desktop_and_window_coordinates() {
        let monitor = Monitor { x: 2560, y: 0, width: 1920, height: 1080, ..Default::default() };
        let mut s = Screen::new(egui::ViewportBuilder::default(), monitor, RgbaImage::new(3840, 2160));
        assert_eq!(s.global(), selection(2560.0, 0.0, 1920.0, 1080.0));
        assert_eq!(s.px_scale(), 2.0, "twice the pixels per logical pixel");
        // The window is drawn at (0, 0) of its own surface.
        s.rect = screen(1920.0, 1080.0);
        assert_eq!(s.offset(), vec2(-2560.0, 0.0));
        let on_desktop = pos2(2600.0, 40.0);
        assert_eq!(on_desktop + s.offset(), pos2(40.0, 40.0));
    }

    #[test]
    fn viewports_have_stable_distinct_ids() {
        assert_eq!(viewport_id(0), ViewportId::ROOT);
        assert_ne!(viewport_id(1), ViewportId::ROOT);
        assert_ne!(viewport_id(1), viewport_id(2));
        assert_eq!(viewport_id(1), viewport_id(1));
    }

    // Checked when compiling: closing has to feel instant, and the picker dims more than the editor.
    const _: () = assert!(COUNTDOWN_STEP > 0.0 && FADE_OUT > 0.0 && FADE_OUT < 1.0);
    const _: () = assert!(DIM < PICKER_DIM && PICKER_DIM < 1.0);
}
