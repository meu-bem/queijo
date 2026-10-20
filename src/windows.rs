//! Which windows are on screen, for window mode's picker. Every desktop has its own
//! way of telling; they all answer in the same terms.

use image::RgbaImage;

use crate::desktop::{Desktop, Env};

/// A window on screen, as a desktop reports it.
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app: String,
    /// The visible window, in desktop coordinates: [x, y, width, height].
    pub frame: [i32; 4],
    /// What `WindowSource::capture` returns (frame plus client-side shadows).
    pub buffer: [i32; 4],
    pub focused: bool,
}

/// A window ready for the picker.
pub struct WindowShot {
    pub title: String,
    pub app: String,
    pub frame: [i32; 4],
    pub focused: bool,
    /// The window's own pixels (even where other windows cover it), if the desktop can give them.
    pub image: Option<RgbaImage>,
    /// Small copy for the picker. Without it the editor cuts one from the screen's picture.
    pub thumb: Option<RgbaImage>,
}

/// A way of asking a desktop for its windows.
pub trait WindowSource: Send {
    fn name(&self) -> &'static str;

    /// The windows on screen, topmost first; `None` if this desktop can't be asked.
    fn list(&self) -> Option<Vec<WindowInfo>>;

    /// The window's own pixels, the whole buffer (`WindowInfo::buffer`), even where it is
    /// covered. Most desktops can't, and then the picture of the screen stands in for it.
    fn capture(&self, _window: &WindowInfo) -> Option<RgbaImage> {
        None
    }

    /// Everything the picker needs about each window.
    fn shots(&self, windows: Vec<WindowInfo>) -> Vec<WindowShot> {
        windows
            .into_iter()
            .map(|w| {
                let image = self.capture(&w).and_then(|img| crop_to_frame(&img, w.frame, w.buffer));
                shot(w, image)
            })
            .collect()
    }
}

/// The ways to ask this desktop, in the order they are tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The Queijo GNOME Shell extension.
    Extension,
}

pub fn kinds(env: &Env) -> Vec<Kind> {
    match &env.desktop {
        Desktop::Gnome | Desktop::Other(_) => vec![Kind::Extension],
        _ => Vec::new(),
    }
}

/// The first way that answers, and the windows it listed.
pub fn find(env: &Env) -> Option<(Box<dyn WindowSource>, Vec<WindowInfo>)> {
    kinds(env).into_iter().find_map(|kind| {
        let source: Box<dyn WindowSource> = match kind {
            Kind::Extension => Box::new(crate::shell_ext::ShellExtension::connect()?),
        };
        let list = source.list()?;
        Some((source, list))
    })
}

/// A picker entry for `info`, with its small preview if the window's pixels are known.
pub fn shot(info: WindowInfo, image: Option<RgbaImage>) -> WindowShot {
    let thumb = image.as_ref().map(thumbnail);
    WindowShot { title: info.title, app: info.app, frame: info.frame, focused: info.focused, image, thumb }
}

/// A copy small enough for the picker: at most 480x300, in the same shape.
pub fn thumbnail(img: &RgbaImage) -> RgbaImage {
    let s = (480.0 / img.width() as f32).min(300.0 / img.height() as f32).min(1.0);
    let (tw, th) = ((img.width() as f32 * s) as u32, (img.height() as f32 * s) as u32);
    image::imageops::thumbnail(img, tw.max(1), th.max(1))
}

/// The picker's preview of a window, cut from the picture of the whole desktop.
/// `origin` is where the picture starts on the desktop and `logical_width` how wide
/// the desktop is in logical pixels (the picture may have more pixels than that).
pub fn thumb_from_desktop(
    desktop: &RgbaImage,
    origin: (i32, i32),
    logical_width: f32,
    [fx, fy, fw, fh]: [i32; 4],
) -> Option<RgbaImage> {
    let scale = desktop.width() as f32 / logical_width.max(1.0);
    let px = |v: i32| (v as f32 * scale).round() as i64;
    let (x0, y0) = (px(fx - origin.0).max(0), px(fy - origin.1).max(0));
    let (x1, y1) =
        (px(fx - origin.0 + fw).min(desktop.width() as i64), px(fy - origin.1 + fh).min(desktop.height() as i64));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let part = image::imageops::crop_imm(desktop, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image();
    Some(thumbnail(&part))
}

pub fn crop_to_frame(img: &RgbaImage, [fx, fy, fw, fh]: [i32; 4], [bx, by, bw, _]: [i32; 4]) -> Option<RgbaImage> {
    let s = img.width() as f32 / bw.max(1) as f32;
    let px = |v: i32| (v as f32 * s).round().max(0.0) as u32;
    let (x, y) = (px(fx - bx), px(fy - by));
    if x >= img.width() || y >= img.height() {
        return None;
    }
    let (w, h) = (px(fw).min(img.width() - x), px(fh).min(img.height() - y));
    (w > 0 && h > 0).then(|| image::imageops::crop_imm(img, x, y, w, h).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn numbered(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255]))
    }

    #[test]
    fn cuts_the_frame_out_of_the_buffer() {
        // The buffer starts 10 px up and left of the frame (that is the shadow).
        let cut = crop_to_frame(&numbered(120, 100), [0, 0, 100, 80], [-10, -10, 120, 100]).unwrap();
        assert_eq!(cut.dimensions(), (100, 80));
        assert_eq!(*cut.get_pixel(0, 0), Rgba([10, 10, 0, 255]));
    }

    #[test]
    fn scales_with_the_buffer() {
        let cut = crop_to_frame(&numbered(240, 200), [0, 0, 100, 80], [-10, -10, 120, 100]).unwrap();
        assert_eq!(cut.dimensions(), (200, 160), "twice the pixels per logical pixel");
        assert_eq!(*cut.get_pixel(0, 0), Rgba([20, 20, 0, 255]));
    }

    #[test]
    fn stays_inside_the_picture() {
        let cut = crop_to_frame(&numbered(100, 100), [0, 0, 500, 500], [0, 0, 100, 100]).unwrap();
        assert_eq!(cut.dimensions(), (100, 100));
        assert!(crop_to_frame(&numbered(100, 100), [500, 0, 50, 50], [0, 0, 100, 100]).is_none());
        assert!(crop_to_frame(&numbered(100, 100), [10, 10, 0, 50], [0, 0, 100, 100]).is_none(), "empty frame");
    }

    #[test]
    fn the_extension_is_only_asked_on_gnome_and_unknown_desktops() {
        let env = |desktop| Env { session: crate::desktop::Session::Wayland, desktop, compositing: true };
        assert_eq!(kinds(&env(Desktop::Gnome)), [Kind::Extension]);
        assert_eq!(kinds(&env(Desktop::Other(String::new()))), [Kind::Extension]);
        assert!(kinds(&env(Desktop::Kde)).is_empty());
    }

    #[test]
    fn cuts_a_preview_out_of_the_desktop_at_any_scale() {
        // Two monitors side by side at 2x: 3840x1080 pixels for 1920x540 logical.
        let desktop =
            RgbaImage::from_fn(
                3840,
                1080,
                |x, _| if x < 1920 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) },
            );
        let left = thumb_from_desktop(&desktop, (0, 0), 1920.0, [100, 100, 400, 300]).unwrap();
        // 400x300 logical is 800x600 pixels; the preview shrinks it to fit 480x300.
        assert_eq!(left.dimensions(), (400, 300));
        assert_eq!(left.get_pixel(5, 5).0, [255, 0, 0, 255]);
        let right = thumb_from_desktop(&desktop, (0, 0), 1920.0, [1000, 100, 400, 300]).unwrap();
        assert_eq!(right.get_pixel(5, 5).0, [0, 0, 255, 255]);
        // A window at negative coordinates: the picture starts at the left-most monitor.
        let moved = thumb_from_desktop(&desktop, (-1000, 0), 1920.0, [-900, 100, 400, 300]).unwrap();
        assert_eq!(moved.get_pixel(5, 5).0, [255, 0, 0, 255]);
        // Off the picture altogether, or empty.
        assert!(thumb_from_desktop(&desktop, (0, 0), 1920.0, [5000, 0, 100, 100]).is_none());
        assert!(thumb_from_desktop(&desktop, (0, 0), 1920.0, [0, 0, 0, 100]).is_none());
    }
}
