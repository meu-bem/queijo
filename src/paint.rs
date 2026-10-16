//! Annotation shapes, vector icons and the colour theme.

use std::f32::consts::{FRAC_PI_4, PI, TAU};

use crate::i18n::t;
use eframe::egui::{
    self, epaint::PathStroke, pos2, vec2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
    TextureHandle, Vec2,
};

pub const ACCENT: Color32 = Color32::from_rgb(255, 196, 46);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(22, 22, 26, 238);
pub const PANEL_BORDER: Color32 = Color32::from_rgba_premultiplied(20, 20, 20, 20);
pub const TEXT: Color32 = Color32::from_rgb(242, 242, 245);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 150, 160);

pub const PALETTE: [Color32; 10] = [
    Color32::from_rgb(255, 59, 48),
    Color32::from_rgb(255, 149, 0),
    Color32::from_rgb(255, 204, 0),
    Color32::from_rgb(52, 199, 89),
    Color32::from_rgb(50, 173, 230),
    Color32::from_rgb(0, 122, 255),
    Color32::from_rgb(175, 82, 222),
    Color32::from_rgb(255, 45, 85),
    Color32::from_rgb(255, 255, 255),
    Color32::from_rgb(28, 28, 30),
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tool {
    Pen,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Marker,
    Text,
    Pixelate,
}

impl Tool {
    pub const ALL: [Tool; 8] =
        [Tool::Pen, Tool::Line, Tool::Arrow, Tool::Rect, Tool::Ellipse, Tool::Marker, Tool::Text, Tool::Pixelate];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap()
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Pen => t("tool.pen"),
            Tool::Line => t("tool.line"),
            Tool::Arrow => t("tool.arrow"),
            Tool::Rect => t("tool.rect"),
            Tool::Ellipse => t("tool.ellipse"),
            Tool::Marker => t("tool.marker"),
            Tool::Text => t("tool.text"),
            Tool::Pixelate => t("tool.pixelate"),
        }
    }

    pub fn key(self) -> egui::Key {
        use egui::Key;
        match self {
            Tool::Pen => Key::P,
            Tool::Line => Key::L,
            Tool::Arrow => Key::A,
            Tool::Rect => Key::R,
            Tool::Ellipse => Key::E,
            Tool::Marker => Key::M,
            Tool::Text => Key::T,
            Tool::Pixelate => Key::B,
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Tool::Pen => Icon::Pen,
            Tool::Line => Icon::Line,
            Tool::Arrow => Icon::Arrow,
            Tool::Rect => Icon::Rect,
            Tool::Ellipse => Icon::Ellipse,
            Tool::Marker => Icon::Marker,
            Tool::Text => Icon::Text,
            Tool::Pixelate => Icon::Pixelate,
        }
    }

    pub fn default_size(self) -> f32 {
        match self {
            Tool::Pen | Tool::Line | Tool::Arrow => 4.0,
            Tool::Rect | Tool::Ellipse => 3.0,
            Tool::Marker => 18.0,
            Tool::Text => 24.0,
            Tool::Pixelate => 12.0,
        }
    }

    pub fn size_range(self) -> (f32, f32) {
        match self {
            Tool::Marker => (6.0, 60.0),
            Tool::Text => (10.0, 120.0),
            Tool::Pixelate => (4.0, 40.0),
            _ => (1.0, 30.0),
        }
    }

    /// Quick-pick sizes offered in the colour popover.
    pub fn presets(self) -> [f32; 4] {
        match self {
            Tool::Marker => [10.0, 18.0, 28.0, 40.0],
            Tool::Text => [16.0, 24.0, 36.0, 56.0],
            Tool::Pixelate => [6.0, 12.0, 20.0, 32.0],
            _ => [2.0, 4.0, 7.0, 12.0],
        }
    }
}

#[derive(Clone)]
pub enum Annotation {
    Pen { points: Vec<Pos2>, color: Color32, width: f32 },
    Marker { points: Vec<Pos2>, color: Color32, width: f32, straight: bool },
    Line { from: Pos2, to: Pos2, color: Color32, width: f32, arrow: bool },
    Rect { from: Pos2, to: Pos2, color: Color32, width: f32 },
    Ellipse { from: Pos2, to: Pos2, color: Color32, width: f32 },
    Text { pos: Pos2, text: String, color: Color32, size: f32 },
    Pixelate { rect: Rect, block: f32, texture: Option<TextureHandle> },
}

impl Annotation {
    pub fn start(tool: Tool, p: Pos2, color: Color32, size: f32) -> Option<Self> {
        Some(match tool {
            Tool::Pen => Annotation::Pen { points: vec![p], color, width: size },
            Tool::Marker => Annotation::Marker { points: vec![p], color, width: size, straight: false },
            Tool::Line => Annotation::Line { from: p, to: p, color, width: size, arrow: false },
            Tool::Arrow => Annotation::Line { from: p, to: p, color, width: size, arrow: true },
            Tool::Rect => Annotation::Rect { from: p, to: p, color, width: size },
            Tool::Ellipse => Annotation::Ellipse { from: p, to: p, color, width: size },
            Tool::Pixelate => Annotation::Pixelate { rect: Rect::from_two_pos(p, p), block: size, texture: None },
            Tool::Text => return None,
        })
    }

    /// Extends the shape being drawn to `p`. `shift` constrains it (45° lines, squares, circles).
    pub fn update(&mut self, p: Pos2, shift: bool) {
        match self {
            Annotation::Pen { points, .. } => {
                if points.last().is_none_or(|last| last.distance(p) >= 1.5) {
                    points.push(p);
                }
            }
            Annotation::Marker { points, straight, .. } => {
                *straight = shift;
                if shift {
                    points.truncate(1);
                    points.push(pos2(p.x, points[0].y));
                } else if points.last().is_none_or(|last| last.distance(p) >= 1.5) {
                    points.push(p);
                }
            }
            Annotation::Line { from, to, .. } => *to = if shift { snap_angle(*from, p) } else { p },
            Annotation::Rect { from, to, .. } | Annotation::Ellipse { from, to, .. } => {
                *to = if shift { square(*from, p) } else { p }
            }
            Annotation::Pixelate { rect, .. } => *rect = Rect::from_two_pos(rect.min, p),
            Annotation::Text { .. } => {}
        }
    }

    /// The same shape moved by `d` (annotations live in desktop coordinates;
    /// each monitor paints them shifted into its own window).
    pub fn translated(&self, d: Vec2) -> Self {
        let mut a = self.clone();
        match &mut a {
            Annotation::Pen { points, .. } | Annotation::Marker { points, .. } => {
                points.iter_mut().for_each(|p| *p += d);
            }
            Annotation::Line { from, to, .. }
            | Annotation::Rect { from, to, .. }
            | Annotation::Ellipse { from, to, .. } => {
                *from += d;
                *to += d;
            }
            Annotation::Text { pos, .. } => *pos += d,
            Annotation::Pixelate { rect, .. } => *rect = rect.translate(d),
        }
        a
    }

    /// Drops accidental clicks that would leave an invisible shape behind.
    pub fn is_meaningful(&self) -> bool {
        match self {
            Annotation::Pen { .. } | Annotation::Marker { .. } => true,
            Annotation::Line { from, to, .. } => from.distance(*to) > 3.0,
            Annotation::Rect { from, to, .. } | Annotation::Ellipse { from, to, .. } => {
                (from.x - to.x).abs() > 2.0 && (from.y - to.y).abs() > 2.0
            }
            Annotation::Pixelate { rect, .. } => rect.width() > 3.0 && rect.height() > 3.0,
            Annotation::Text { text, .. } => !text.trim().is_empty(),
        }
    }

    /// Smooths freehand strokes once they are finished.
    pub fn finish(&mut self) {
        if let Annotation::Pen { points, .. } | Annotation::Marker { points, straight: false, .. } = self {
            *points = chaikin(points);
        }
    }

    pub fn paint(&self, painter: &Painter) {
        match self {
            Annotation::Pen { points, color, width } => {
                if points.len() == 1 {
                    painter.circle_filled(points[0], width / 2.0, *color);
                    return;
                }
                painter.add(Shape::line(points.clone(), Stroke::new(*width, *color)));
                // Round caps; egui paths end square.
                painter.circle_filled(points[0], width / 2.0, *color);
                painter.circle_filled(*points.last().unwrap(), width / 2.0, *color);
            }
            Annotation::Marker { points, color, width, .. } => {
                let c = color.gamma_multiply(0.42);
                if points.len() == 1 {
                    painter.rect_filled(Rect::from_center_size(points[0], vec2(width / 3.0, *width)), 2.0, c);
                } else {
                    painter.add(Shape::line(points.clone(), PathStroke::new(*width, c)));
                }
            }
            Annotation::Line { from, to, color, width, arrow } => {
                if *arrow {
                    paint_arrow(painter, *from, *to, *color, *width);
                } else {
                    painter.line_segment([*from, *to], Stroke::new(*width, *color));
                    painter.circle_filled(*from, width / 2.0, *color);
                    painter.circle_filled(*to, width / 2.0, *color);
                }
            }
            Annotation::Rect { from, to, color, width } => {
                painter.rect_stroke(Rect::from_two_pos(*from, *to), 2, Stroke::new(*width, *color), StrokeKind::Middle);
            }
            Annotation::Ellipse { from, to, color, width } => {
                let r = Rect::from_two_pos(*from, *to);
                painter.add(Shape::ellipse_stroke(r.center(), r.size() / 2.0, Stroke::new(*width, *color)));
            }
            Annotation::Text { pos, text, color, size } => {
                painter.galley(*pos, text_galley(painter, text, *color, *size), *color);
            }
            Annotation::Pixelate { rect, texture, .. } => match texture {
                Some(tex) => {
                    painter.image(tex.id(), *rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                }
                None => {
                    painter.rect_filled(*rect, 0, Color32::from_white_alpha(40));
                    dashed_rect(painter, *rect, Stroke::new(1.0, Color32::from_white_alpha(200)));
                }
            },
        }
    }

    pub fn text_rect(&self, painter: &Painter) -> Option<Rect> {
        match self {
            Annotation::Text { pos, text, color, size } => {
                Some(Rect::from_min_size(*pos, text_galley(painter, text, *color, *size).size()))
            }
            _ => None,
        }
    }
}

pub fn text_galley(painter: &Painter, text: &str, color: Color32, size: f32) -> std::sync::Arc<egui::Galley> {
    painter.layout_no_wrap(text.to_string(), FontId::proportional(size), color)
}

/// Builds the blocky texture for a pixelate annotation from the frozen screen.
/// `scale` converts points to image pixels.
pub fn pixelate(
    ctx: &egui::Context,
    image: &image::RgbaImage,
    rect: Rect,
    screen: Rect,
    scale: f32,
    block: f32,
) -> Option<TextureHandle> {
    let x0 = (((rect.min.x - screen.min.x) * scale).floor().max(0.0) as u32).min(image.width());
    let y0 = (((rect.min.y - screen.min.y) * scale).floor().max(0.0) as u32).min(image.height());
    let x1 = (((rect.max.x - screen.min.x) * scale).ceil().max(0.0) as u32).min(image.width());
    let y1 = (((rect.max.y - screen.min.y) * scale).ceil().max(0.0) as u32).min(image.height());
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let block = ((block * scale).round() as u32).max(2);
    let cols = (x1 - x0).div_ceil(block);
    let rows = (y1 - y0).div_ceil(block);
    let mut pixels = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            let (bx, by) = (x0 + c * block, y0 + r * block);
            let (ex, ey) = ((bx + block).min(x1), (by + block).min(y1));
            let mut sum = [0u64; 3];
            for y in by..ey {
                for x in bx..ex {
                    let p = image.get_pixel(x, y).0;
                    sum[0] += p[0] as u64;
                    sum[1] += p[1] as u64;
                    sum[2] += p[2] as u64;
                }
            }
            let n = ((ex - bx) * (ey - by)).max(1) as u64;
            pixels.push(Color32::from_rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8));
        }
    }
    let img =
        egui::ColorImage { size: [cols as usize, rows as usize], source_size: vec2(cols as f32, rows as f32), pixels };
    Some(ctx.load_texture("pixelate", img, egui::TextureOptions::NEAREST))
}

fn paint_arrow(painter: &Painter, from: Pos2, to: Pos2, color: Color32, width: f32) {
    let delta = to - from;
    let len = delta.length();
    if len < 1.0 {
        painter.circle_filled(from, width / 2.0, color);
        return;
    }
    let dir = delta / len;
    let normal = vec2(-dir.y, dir.x);
    let head_len = (width * 3.2 + 9.0).min(len * 0.8);
    let half_w = head_len * 0.42;
    let base = to - dir * head_len;
    painter.line_segment([from, base + dir * (head_len * 0.3)], Stroke::new(width, color));
    painter.circle_filled(from, width / 2.0, color);
    // Not convex (notched base), but it is a fan around the tip, which is all the tessellator needs.
    painter.add(Shape::convex_polygon(
        vec![to, base + normal * half_w, base + dir * (head_len * 0.18), base - normal * half_w],
        color,
        Stroke::NONE,
    ));
}

fn snap_angle(from: Pos2, p: Pos2) -> Pos2 {
    let d = p - from;
    let angle = (d.y.atan2(d.x) / FRAC_PI_4).round() * FRAC_PI_4;
    from + vec2(angle.cos(), angle.sin()) * d.length()
}

fn square(from: Pos2, p: Pos2) -> Pos2 {
    let d = p - from;
    let s = d.x.abs().max(d.y.abs());
    from + vec2(s.copysign(d.x), s.copysign(d.y))
}

fn chaikin(points: &[Pos2]) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);
    for w in points.windows(2) {
        out.push(w[0].lerp(w[1], 0.25));
        out.push(w[0].lerp(w[1], 0.75));
    }
    out.push(*points.last().unwrap());
    out
}

pub fn dashed_rect(painter: &Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    painter.extend(Shape::dashed_line(&pts, stroke, 4.0, 3.0));
}

// ---------------------------------------------------------------------------
// Icons, drawn as vectors so they stay crisp at any scale.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Icon {
    Pen,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Marker,
    Text,
    Pixelate,
    Undo,
    Redo,
    Save,
    Copy,
    Close,
    Selection,
    Screen,
    Window,
    Photo,
    Video,
    Gif,
    Pointer,
    Gear,
    Keyboard,
    Folder,
}

pub fn icon(painter: &Painter, icon: Icon, c: Pos2, color: Color32) {
    let s = Stroke::new(1.7, color);
    let p = |x: f32, y: f32| c + vec2(x, y);
    match icon {
        Icon::Pen => {
            let body = [p(-6.5, 6.5), p(-6.0, 3.0), p(3.5, -6.5), p(6.5, -3.5), p(-3.0, 6.0)];
            painter.add(Shape::closed_line(body.to_vec(), s));
            painter.line_segment([p(1.5, -4.5), p(4.5, -1.5)], s);
        }
        Icon::Line => {
            painter.line_segment([p(-6.0, 6.0), p(6.0, -6.0)], s);
            painter.circle_filled(p(-6.0, 6.0), 1.6, color);
            painter.circle_filled(p(6.0, -6.0), 1.6, color);
        }
        Icon::Arrow => {
            painter.line_segment([p(-6.0, 6.0), p(5.5, -5.5)], s);
            painter.add(Shape::line(vec![p(-0.5, -6.5), p(6.5, -6.5), p(6.5, 0.5)], s));
        }
        Icon::Rect => {
            painter.rect_stroke(Rect::from_center_size(c, vec2(15.0, 11.0)), 2, s, StrokeKind::Middle);
        }
        Icon::Ellipse => {
            painter.add(Shape::ellipse_stroke(c, vec2(7.5, 6.0), s));
        }
        Icon::Marker => {
            let body = [p(-4.5, 2.5), p(2.5, -6.5), p(6.5, -2.5), p(-2.5, 4.5)];
            painter.add(Shape::closed_line(body.to_vec(), s));
            painter.add(Shape::convex_polygon(vec![p(-4.5, 2.5), p(-2.5, 4.5), p(-5.5, 5.5)], color, Stroke::NONE));
            painter.rect_filled(Rect::from_min_max(p(-7.5, 6.5), p(7.5, 8.5)), 1, color.gamma_multiply(0.45));
        }
        Icon::Text => {
            painter.add(Shape::line(vec![p(-6.0, -4.5), p(-6.0, -6.5), p(6.0, -6.5), p(6.0, -4.5)], s));
            painter.line_segment([p(0.0, -6.5), p(0.0, 6.5)], s);
            painter.line_segment([p(-2.5, 6.5), p(2.5, 6.5)], s);
        }
        Icon::Pixelate => {
            for row in 0..3 {
                for col in 0..3 {
                    let r = Rect::from_min_size(p(-7.0 + col as f32 * 5.0, -7.0 + row as f32 * 5.0), vec2(4.0, 4.0));
                    let a = if (row + col) % 2 == 0 { 1.0 } else { 0.35 };
                    painter.rect_filled(r, 1, color.gamma_multiply(a));
                }
            }
        }
        Icon::Undo | Icon::Redo => {
            let f = if icon == Icon::Redo { -1.0 } else { 1.0 };
            let q = |x: f32, y: f32| c + vec2(x * f, y);
            let mut pts = vec![q(-6.0, -2.5), q(1.5, -2.5)];
            pts.extend((1..=12).map(|i| {
                let a = -PI / 2.0 + i as f32 / 12.0 * PI;
                q(1.5 + 4.25 * a.cos(), 1.75 + 4.25 * a.sin())
            }));
            pts.push(q(-2.5, 6.0));
            painter.add(Shape::line(pts, s));
            painter.add(Shape::line(vec![q(-3.0, -5.5), q(-6.0, -2.5), q(-3.0, 0.5)], s));
        }
        Icon::Save => {
            painter.line_segment([p(0.0, -7.0), p(0.0, 3.0)], s);
            painter.add(Shape::line(vec![p(-4.0, -1.0), p(0.0, 3.0), p(4.0, -1.0)], s));
            painter.add(Shape::line(vec![p(-7.0, 3.0), p(-7.0, 7.0), p(7.0, 7.0), p(7.0, 3.0)], s));
        }
        Icon::Copy => {
            painter.rect_stroke(Rect::from_min_size(p(-3.5, -3.5), vec2(10.0, 10.5)), 2, s, StrokeKind::Middle);
            painter.add(Shape::line(vec![p(-6.5, 3.5), p(-6.5, -7.0), p(3.5, -7.0)], s));
        }
        Icon::Close => {
            painter.line_segment([p(-5.5, -5.5), p(5.5, 5.5)], s);
            painter.line_segment([p(-5.5, 5.5), p(5.5, -5.5)], s);
        }
        Icon::Selection => {
            let r = Rect::from_center_size(c, vec2(15.0, 12.0));
            for (corner, dx, dy) in [
                (r.left_top(), 1.0, 1.0),
                (r.right_top(), -1.0, 1.0),
                (r.left_bottom(), 1.0, -1.0),
                (r.right_bottom(), -1.0, -1.0),
            ] {
                painter.add(Shape::line(vec![corner + vec2(0.0, 4.0 * dy), corner, corner + vec2(4.0 * dx, 0.0)], s));
            }
            painter.rect_filled(Rect::from_center_size(c, vec2(5.0, 3.0)), 1, color.gamma_multiply(0.5));
        }
        Icon::Screen => {
            painter.rect_stroke(Rect::from_min_max(p(-8.0, -6.5), p(8.0, 4.0)), 2, s, StrokeKind::Middle);
            painter.line_segment([p(0.0, 4.0), p(0.0, 7.0)], s);
            painter.line_segment([p(-4.0, 7.5), p(4.0, 7.5)], s);
        }
        Icon::Window => {
            let r = Rect::from_min_max(p(-8.0, -6.5), p(8.0, 6.5));
            painter.rect_stroke(r, 2, s, StrokeKind::Middle);
            painter.line_segment([p(-8.0, -2.5), p(8.0, -2.5)], s);
            painter.circle_filled(p(5.0, -4.5), 0.9, color);
            painter.circle_filled(p(2.5, -4.5), 0.9, color);
        }
        Icon::Photo => {
            painter.rect_stroke(Rect::from_min_max(p(-8.0, -4.5), p(8.0, 6.5)), 3, s, StrokeKind::Middle);
            painter.add(Shape::line(vec![p(-3.5, -4.5), p(-2.0, -7.0), p(2.0, -7.0), p(3.5, -4.5)], s));
            painter.circle_stroke(p(0.0, 1.0), 3.0, s);
        }
        Icon::Video => {
            painter.rect_stroke(Rect::from_min_max(p(-8.0, -5.0), p(3.5, 5.0)), 2, s, StrokeKind::Middle);
            painter.add(Shape::closed_line(vec![p(3.5, -1.5), p(8.0, -4.5), p(8.0, 4.5), p(3.5, 1.5)], s));
        }
        Icon::Gif => {
            painter.rect_stroke(Rect::from_center_size(c, vec2(19.0, 14.0)), 3, s, StrokeKind::Middle);
            let g = painter.layout_no_wrap("GIF".into(), FontId::proportional(8.5), color);
            painter.galley(c - g.size() / 2.0 + vec2(0.3, 0.0), g, color);
        }
        Icon::Gear => {
            let n = 8;
            let pts: Vec<Pos2> = (0..n * 4)
                .map(|i| {
                    let a = i as f32 / (n * 4) as f32 * TAU;
                    let r = if (i / 2) % 2 == 0 { 7.5 } else { 5.6 };
                    c + Vec2::angled(a + TAU / (n as f32 * 8.0)) * r
                })
                .collect();
            painter.add(Shape::closed_line(pts, s));
            painter.circle_stroke(c, 2.4, s);
        }
        Icon::Keyboard => {
            painter.rect_stroke(Rect::from_center_size(c, vec2(17.0, 11.0)), 2, s, StrokeKind::Middle);
            for (x, y) in [(-4.5, -1.5), (-1.5, -1.5), (1.5, -1.5), (4.5, -1.5)] {
                painter.circle_filled(p(x, y), 0.9, color);
            }
            painter.line_segment([p(-3.5, 2.0), p(3.5, 2.0)], s);
        }
        Icon::Folder => {
            painter.add(Shape::closed_line(
                vec![p(-8.0, -5.5), p(-3.0, -5.5), p(-1.0, -3.5), p(8.0, -3.5), p(8.0, 6.0), p(-8.0, 6.0)],
                s,
            ));
        }
        Icon::Pointer => {
            let pts =
                vec![p(-4.5, -7.5), p(-4.5, 5.0), p(-1.5, 2.0), p(1.0, 7.5), p(3.0, 6.5), p(0.5, 1.0), p(4.5, 1.0)];
            painter.add(Shape::closed_line(pts, s));
        }
    }
}

/// Draws a soft drop shadow plus a rounded panel.
pub fn panel(painter: &Painter, rect: Rect, radius: u8, alpha: f32) {
    let shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(110) };
    let mut shape = shadow.as_shape(rect, radius);
    shape.fill = shadow.color.gamma_multiply(alpha);
    painter.add(shape);
    painter.rect(
        rect,
        radius,
        PANEL.gamma_multiply(alpha),
        Stroke::new(1.0, PANEL_BORDER.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Overshoots slightly before settling; used for things that "pop" in.
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

pub fn ring(painter: &Painter, c: Pos2, r: f32, stroke: Stroke) {
    let n = ((r * TAU / 4.0) as usize).clamp(16, 96);
    let pts = (0..n).map(|i| c + Vec2::angled(i as f32 / n as f32 * TAU) * r).collect();
    painter.add(Shape::closed_line(pts, stroke));
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}
