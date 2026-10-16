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

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color32 = Color32::RED;

    fn start(tool: Tool, p: Pos2) -> Annotation {
        Annotation::start(tool, p, RED, 4.0).expect("drawable tool")
    }

    #[test]
    fn tool_tables_are_consistent() {
        for (i, tool) in Tool::ALL.into_iter().enumerate() {
            assert_eq!(tool.index(), i);
            let (lo, hi) = tool.size_range();
            assert!(lo < hi);
            assert!((lo..=hi).contains(&tool.default_size()), "{tool:?} default size");
            let presets = tool.presets();
            assert!(presets.windows(2).all(|w| w[0] < w[1]), "{tool:?} presets grow");
            assert!(presets.iter().all(|p| (lo..=hi).contains(p)), "{tool:?} presets are within the range");
            assert!(!tool.name().is_empty());
        }
        let keys: std::collections::HashSet<_> = Tool::ALL.iter().map(|t| t.key()).collect();
        assert_eq!(keys.len(), Tool::ALL.len(), "every tool has its own key");
    }

    #[test]
    fn every_tool_but_text_starts_a_drawing() {
        for tool in Tool::ALL {
            let drawing = Annotation::start(tool, pos2(5.0, 6.0), RED, 3.0);
            assert_eq!(drawing.is_some(), tool != Tool::Text, "{tool:?}");
        }
    }

    #[test]
    fn the_pen_skips_points_that_barely_moved() {
        let mut pen = start(Tool::Pen, pos2(0.0, 0.0));
        pen.update(pos2(0.5, 0.5), false);
        pen.update(pos2(10.0, 0.0), false);
        let Annotation::Pen { points, .. } = &pen else { unreachable!() };
        assert_eq!(points, &[pos2(0.0, 0.0), pos2(10.0, 0.0)]);
    }

    #[test]
    fn the_marker_goes_straight_with_shift() {
        let mut marker = start(Tool::Marker, pos2(10.0, 20.0));
        for p in [pos2(30.0, 25.0), pos2(60.0, 33.0)] {
            marker.update(p, false);
        }
        marker.update(pos2(80.0, 90.0), true);
        let Annotation::Marker { points, straight, .. } = &marker else { unreachable!() };
        assert!(*straight);
        assert_eq!(points, &[pos2(10.0, 20.0), pos2(80.0, 20.0)], "horizontal from the start");
        marker.update(pos2(85.0, 40.0), false);
        let Annotation::Marker { points, straight, .. } = &marker else { unreachable!() };
        assert!(!*straight);
        assert_eq!(
            points,
            &[pos2(10.0, 20.0), pos2(80.0, 20.0), pos2(85.0, 40.0)],
            "freehand again from where the line ended"
        );
    }

    #[test]
    fn lines_snap_to_45_degrees_with_shift() {
        let mut line = start(Tool::Line, pos2(0.0, 0.0));
        line.update(pos2(100.0, 12.0), true);
        let Annotation::Line { to, .. } = &line else { unreachable!() };
        assert!(to.y.abs() < 0.01 && (to.x - 100.7).abs() < 1.0, "snapped to horizontal: {to:?}");
        line.update(pos2(100.0, 90.0), true);
        let Annotation::Line { to, .. } = &line else { unreachable!() };
        assert!((to.x - to.y).abs() < 0.01, "snapped to the diagonal: {to:?}");
        line.update(pos2(100.0, 12.0), false);
        let Annotation::Line { to, .. } = &line else { unreachable!() };
        assert_eq!(*to, pos2(100.0, 12.0), "free without shift");
    }

    #[test]
    fn arrows_are_lines_with_a_head() {
        let Annotation::Line { arrow, .. } = start(Tool::Arrow, pos2(0.0, 0.0)) else { unreachable!() };
        let Annotation::Line { arrow: plain, .. } = start(Tool::Line, pos2(0.0, 0.0)) else { unreachable!() };
        assert!(arrow && !plain);
    }

    #[test]
    fn rectangles_and_ellipses_become_square_with_shift() {
        for tool in [Tool::Rect, Tool::Ellipse] {
            let mut shape = start(tool, pos2(10.0, 10.0));
            shape.update(pos2(110.0, 40.0), true);
            let (Annotation::Rect { to, .. } | Annotation::Ellipse { to, .. }) = &shape else { unreachable!() };
            assert_eq!(*to, pos2(110.0, 110.0), "{tool:?}: the longer side wins");
            shape.update(pos2(-50.0, 30.0), true);
            let (Annotation::Rect { to, .. } | Annotation::Ellipse { to, .. }) = &shape else { unreachable!() };
            assert_eq!(*to, pos2(-50.0, 70.0), "{tool:?}: each axis follows the direction of the drag");
        }
    }

    #[test]
    fn stray_clicks_leave_nothing_behind() {
        let text = |s: &str| Annotation::Text { pos: Pos2::ZERO, text: s.into(), color: RED, size: 20.0 };
        assert!(!text("").is_meaningful());
        assert!(!text("  \n ").is_meaningful());
        assert!(text("hi").is_meaningful());

        assert!(!start(Tool::Line, pos2(0.0, 0.0)).is_meaningful(), "a click is not a line");
        let mut line = start(Tool::Line, pos2(0.0, 0.0));
        line.update(pos2(20.0, 0.0), false);
        assert!(line.is_meaningful());

        let mut thin = start(Tool::Rect, pos2(0.0, 0.0));
        thin.update(pos2(100.0, 1.0), false);
        assert!(!thin.is_meaningful(), "no height: invisible");
        let mut rect = start(Tool::Rect, pos2(0.0, 0.0));
        rect.update(pos2(10.0, 10.0), false);
        assert!(rect.is_meaningful());

        assert!(start(Tool::Pen, pos2(0.0, 0.0)).is_meaningful(), "a dot is a dot");
        assert!(!start(Tool::Pixelate, pos2(0.0, 0.0)).is_meaningful());
    }

    #[test]
    fn finishing_smooths_freehand_strokes_and_keeps_the_ends() {
        let mut pen = start(Tool::Pen, pos2(0.0, 0.0));
        for p in [pos2(10.0, 10.0), pos2(20.0, 0.0), pos2(30.0, 10.0), pos2(40.0, 0.0)] {
            pen.update(p, false);
        }
        pen.finish();
        let Annotation::Pen { points, .. } = &pen else { unreachable!() };
        assert_eq!(points.len(), 2 * 4 + 2);
        assert_eq!((points[0], *points.last().unwrap()), (pos2(0.0, 0.0), pos2(40.0, 0.0)));

        // A ruler-straight marker line must stay exactly straight.
        let mut marker = start(Tool::Marker, pos2(0.0, 0.0));
        marker.update(pos2(50.0, 9.0), true);
        marker.finish();
        let Annotation::Marker { points, .. } = &marker else { unreachable!() };
        assert_eq!(points.len(), 2);

        let mut rect = start(Tool::Rect, pos2(0.0, 0.0));
        rect.update(pos2(9.0, 9.0), false);
        rect.finish(); // nothing to smooth
    }

    #[test]
    fn translating_moves_every_kind_of_drawing() {
        let d = vec2(5.0, -3.0);
        let moved = |a: Annotation| a.translated(d);

        let Annotation::Pen { points, .. } = moved(start(Tool::Pen, pos2(1.0, 1.0))) else { unreachable!() };
        assert_eq!(points, [pos2(6.0, -2.0)]);
        let Annotation::Marker { points, .. } = moved(start(Tool::Marker, pos2(1.0, 1.0))) else { unreachable!() };
        assert_eq!(points, [pos2(6.0, -2.0)]);
        let mut line = start(Tool::Arrow, pos2(1.0, 1.0));
        line.update(pos2(11.0, 11.0), false);
        let Annotation::Line { from, to, .. } = moved(line) else { unreachable!() };
        assert_eq!((from, to), (pos2(6.0, -2.0), pos2(16.0, 8.0)));
        let mut rect = start(Tool::Rect, pos2(0.0, 0.0));
        rect.update(pos2(10.0, 10.0), false);
        let Annotation::Rect { from, to, .. } = moved(rect) else { unreachable!() };
        assert_eq!((from, to), (pos2(5.0, -3.0), pos2(15.0, 7.0)));
        let Annotation::Text { pos, .. } =
            moved(Annotation::Text { pos: pos2(2.0, 2.0), text: "x".into(), color: RED, size: 9.0 })
        else {
            unreachable!()
        };
        assert_eq!(pos, pos2(7.0, -1.0));
        let Annotation::Pixelate { rect, .. } = moved(start(Tool::Pixelate, pos2(0.0, 0.0))) else { unreachable!() };
        assert_eq!(rect.min, pos2(5.0, -3.0));
        // The original is untouched.
        let original = start(Tool::Pen, pos2(1.0, 1.0));
        let _ = original.translated(d);
        let Annotation::Pen { points, .. } = original else { unreachable!() };
        assert_eq!(points, [pos2(1.0, 1.0)]);
    }

    #[test]
    fn geometry_helpers() {
        let straight = snap_angle(pos2(0.0, 0.0), pos2(0.0, 50.0));
        assert!(straight.x.abs() < 1e-3 && (straight.y - 50.0).abs() < 1e-3, "{straight:?}");
        let diagonal = snap_angle(pos2(10.0, 10.0), pos2(60.0, 55.0));
        assert!(((diagonal.x - 10.0) - (diagonal.y - 10.0)).abs() < 0.01);
        assert_eq!(square(pos2(0.0, 0.0), pos2(30.0, -10.0)), pos2(30.0, -30.0));

        assert_eq!(chaikin(&[pos2(0.0, 0.0), pos2(1.0, 1.0)]).len(), 2, "two points can't be smoothed");
        let smooth = chaikin(&[pos2(0.0, 0.0), pos2(4.0, 0.0), pos2(4.0, 4.0)]);
        assert_eq!(
            smooth,
            [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(3.0, 0.0), pos2(4.0, 1.0), pos2(4.0, 3.0), pos2(4.0, 4.0)]
        );
    }

    #[test]
    fn easing_curves_start_and_end_where_they_should() {
        for f in [ease_out, ease_out_back] {
            assert!(f(0.0).abs() < 1e-6);
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert!((f(-5.0) - f(0.0)).abs() < 1e-6, "clamped below");
            assert!((f(5.0) - f(1.0)).abs() < 1e-6, "clamped above");
        }
        assert!(ease_out(0.3) > 0.3, "fast at first");
        assert!((0..100).any(|i| ease_out_back(i as f32 / 100.0) > 1.0), "overshoots before settling");
        assert_eq!(
            lerp(Color32::from_rgb(0, 0, 0), Color32::from_rgb(200, 100, 50), 0.5),
            Color32::from_rgb(100, 50, 25)
        );
    }

    /// What a pass drew and uploaded.
    struct Pass {
        shapes: usize,
        uploads: Vec<egui::epaint::ImageDelta>,
    }

    /// Runs `f` inside an egui pass, which is what painting needs.
    fn in_pass<R>(ctx: &egui::Context, f: impl FnOnce(&egui::Context, &Painter) -> R) -> (R, Pass) {
        let mut f = Some(f);
        let mut result = None;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            result = Some((f.take().unwrap())(ui.ctx(), &painter));
        });
        // egui insists that texture uploads are handled before the output is dropped.
        let uploads = std::mem::take(&mut output.textures_delta.set).into_values().flatten().collect();
        output.textures_delta.clear();
        (result.unwrap(), Pass { shapes: output.shapes.len(), uploads })
    }

    #[test]
    fn every_icon_and_annotation_paints() {
        let ctx = egui::Context::default();
        let (_, pass) = in_pass(&ctx, |_, painter| {
            let icons = [
                Icon::Pen,
                Icon::Line,
                Icon::Arrow,
                Icon::Rect,
                Icon::Ellipse,
                Icon::Marker,
                Icon::Text,
                Icon::Pixelate,
                Icon::Undo,
                Icon::Redo,
                Icon::Save,
                Icon::Copy,
                Icon::Close,
                Icon::Selection,
                Icon::Screen,
                Icon::Window,
                Icon::Photo,
                Icon::Video,
                Icon::Gif,
                Icon::Pointer,
                Icon::Gear,
                Icon::Keyboard,
                Icon::Folder,
            ];
            for icon_kind in icons {
                icon(painter, icon_kind, pos2(20.0, 20.0), Color32::WHITE);
            }
            for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Text) {
                let mut drawing = start(tool, pos2(10.0, 10.0));
                drawing.update(pos2(80.0, 60.0), false);
                drawing.update(pos2(90.0, 20.0), false);
                drawing.paint(painter);
            }
            Annotation::Text { pos: pos2(5.0, 5.0), text: "Olá".into(), color: RED, size: 18.0 }.paint(painter);
            panel(painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 40.0)), 10, 1.0);
            ring(painter, pos2(30.0, 30.0), 12.0, Stroke::new(1.0, Color32::WHITE));
            dashed_rect(
                painter,
                Rect::from_min_size(pos2(0.0, 0.0), vec2(30.0, 30.0)),
                Stroke::new(1.0, Color32::WHITE),
            );
        });
        assert!(pass.shapes > 0);
    }

    #[test]
    fn pixelating_averages_blocks_of_the_screen() {
        let ctx = egui::Context::default();
        // Left half red, right half blue; one 8-pixel block covers the whole 8x4 area.
        let image = image::RgbaImage::from_fn(8, 4, |x, _| {
            if x < 4 {
                image::Rgba([200, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 200, 255])
            }
        });
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(8.0, 4.0));
        let (texture, pass) = in_pass(&ctx, |ctx, _| pixelate(ctx, &image, screen, screen, 1.0, 8.0));
        assert!(texture.is_some());
        let uploaded: Vec<_> = pass.uploads.iter().filter(|d| d.image.width() == 1).collect();
        assert_eq!(uploaded.len(), 1, "one block, so a 1x1 texture");
        let egui::ImageData::Color(pixels) = &uploaded[0].image;
        assert_eq!(pixels.pixels[0], Color32::from_rgb(100, 0, 100));

        // Smaller blocks keep more detail: 2 columns of 4 pixels each.
        let (texture, pass) = in_pass(&ctx, |ctx, _| pixelate(ctx, &image, screen, screen, 1.0, 4.0));
        assert!(texture.is_some());
        let pair: Vec<_> = pass.uploads.iter().filter(|d| d.image.width() == 2).collect();
        let egui::ImageData::Color(pixels) = &pair[0].image;
        assert_eq!(pixels.pixels[0], Color32::from_rgb(200, 0, 0));
        assert_eq!(pixels.pixels[1], Color32::from_rgb(0, 0, 200));
    }

    #[test]
    fn pixelating_outside_the_screen_gives_nothing() {
        let ctx = egui::Context::default();
        let image = image::RgbaImage::new(10, 10);
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(10.0, 10.0));
        let (outside, _) = in_pass(&ctx, |ctx, _| {
            pixelate(ctx, &image, Rect::from_min_size(pos2(50.0, 50.0), vec2(5.0, 5.0)), screen, 1.0, 4.0)
        });
        assert!(outside.is_none());
        let (empty, _) = in_pass(&ctx, |ctx, _| {
            pixelate(ctx, &image, Rect::from_min_size(pos2(2.0, 2.0), Vec2::ZERO), screen, 1.0, 4.0)
        });
        assert!(empty.is_none());
    }
}
