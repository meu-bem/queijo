//! Talking to the X server directly.

use image::RgbaImage;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xfixes::ConnectionExt as _;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, ImageOrder};
use x11rb::rust_connection::RustConnection;

use crate::monitors::Monitor;

/// Whether a compositing manager is running on the screen (it owns the
/// `_NET_WM_CM_S<n>` selection). Without one, see-through windows are drawn
/// black. `display` is as in `$DISPLAY`; `None` uses that. `None` is returned if
/// the server can't be asked.
pub fn has_compositor(display: Option<&str>) -> Option<bool> {
    let (conn, screen) = RustConnection::connect(display).ok()?;
    let atom = conn.intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes()).ok()?.reply().ok()?.atom;
    let owner = conn.get_selection_owner(atom).ok()?.reply().ok()?.owner;
    Some(owner != x11rb::NONE)
}

/// The mouse pointer at the moment of a screenshot.
pub struct Cursor {
    /// Premultiplied RGBA, as the server gives it.
    pub image: RgbaImage,
    /// Where its top-left corner falls on the screen.
    pub x: i32,
    pub y: i32,
}

/// How the server lays out the pixels of the root window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelFormat {
    red: u32,
    green: u32,
    blue: u32,
    little_endian: bool,
}

/// Pieces of about this many bytes are asked for one after another, so no
/// single request or reply is huge (a 4K screen is 33 MB).
const BAND_BYTES: usize = 4 << 20;

/// A picture of the whole screen (the root window), and the pointer if the
/// server can say where it is. Only for a real X11 session: XWayland's root is black.
pub fn screenshot(display: Option<&str>) -> Result<(RgbaImage, Option<Cursor>), String> {
    let (conn, screen_num) = RustConnection::connect(display).map_err(|e| e.to_string())?;
    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let (width, height) = (screen.width_in_pixels as usize, screen.height_in_pixels as usize);

    let depth = screen.root_depth;
    let bits = setup.pixmap_formats.iter().find(|f| f.depth == depth).map_or(0, |f| f.bits_per_pixel);
    if bits != 32 {
        return Err(format!("the screen has {bits} bits per pixel, only 32 is supported"));
    }
    let visual = screen
        .allowed_depths
        .iter()
        .flat_map(|d| &d.visuals)
        .find(|v| v.visual_id == screen.root_visual)
        .ok_or("the root visual is unknown")?;
    let format = PixelFormat {
        red: visual.red_mask,
        green: visual.green_mask,
        blue: visual.blue_mask,
        little_endian: setup.image_byte_order == ImageOrder::LSB_FIRST,
    };

    // Ask for every band first and read the replies after, so the server never waits for us.
    let rows = (BAND_BYTES / (width * 4).max(1)).max(1);
    let mut cookies = Vec::new();
    for top in (0..height).step_by(rows) {
        let band = rows.min(height - top);
        cookies.push((
            top,
            band,
            conn.get_image(ImageFormat::Z_PIXMAP, screen.root, 0, top as i16, width as u16, band as u16, !0)
                .map_err(|e| e.to_string())?,
        ));
    }
    let mut pixels = Vec::with_capacity(width * height * 4);
    for (_, band, cookie) in cookies {
        let reply = cookie.reply().map_err(|e| e.to_string())?;
        pixels.extend(to_rgba(&reply.data, width * band, format));
    }
    let image =
        RgbaImage::from_raw(width as u32, height as u32, pixels).ok_or("the screen picture has the wrong size")?;
    Ok((image, cursor(&conn)))
}

/// The pointer's picture and place, or `None` without the XFixes extension.
fn cursor(conn: &RustConnection) -> Option<Cursor> {
    conn.xfixes_query_version(4, 0).ok()?.reply().ok()?;
    let c = conn.xfixes_get_cursor_image().ok()?.reply().ok()?;
    let rgba =
        c.cursor_image.iter().flat_map(|argb| [(argb >> 16) as u8, (argb >> 8) as u8, *argb as u8, (argb >> 24) as u8]);
    let image = RgbaImage::from_raw(c.width.into(), c.height.into(), rgba.collect())?;
    Some(Cursor { image, x: i32::from(c.x) - i32::from(c.xhot), y: i32::from(c.y) - i32::from(c.yhot) })
}

/// Draws `cursor` (premultiplied) onto a copy of the picture.
pub fn with_cursor(desktop: &RgbaImage, cursor: &Cursor) -> RgbaImage {
    let mut out = desktop.clone();
    for (cx, cy, px) in cursor.image.enumerate_pixels() {
        let (x, y) = (cursor.x + cx as i32, cursor.y + cy as i32);
        if x < 0 || y < 0 || x >= out.width() as i32 || y >= out.height() as i32 {
            continue;
        }
        let under = out.get_pixel_mut(x as u32, y as u32);
        let a = u32::from(px[3]);
        for c in 0..3 {
            // Premultiplied: the cursor's colour already carries its own alpha.
            under[c] = (u32::from(px[c]) + u32::from(under[c]) * (255 - a) / 255).min(255) as u8;
        }
    }
    out
}

/// Turns the server's 32-bit pixels into RGBA bytes with full opacity.
fn to_rgba(data: &[u8], pixels: usize, format: PixelFormat) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels * 4);
    let fast = format.little_endian && (format.red, format.green, format.blue) == (0xff0000, 0xff00, 0xff);
    for chunk in data.chunks_exact(4).take(pixels) {
        if fast {
            out.extend([chunk[2], chunk[1], chunk[0], 255]);
            continue;
        }
        let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
        let value = if format.little_endian { u32::from_le_bytes(bytes) } else { u32::from_be_bytes(bytes) };
        out.extend([channel(value, format.red), channel(value, format.green), channel(value, format.blue), 255]);
    }
    out
}

/// One colour channel of `value`, as 8 bits, whatever the mask's width.
fn channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    (((value & mask) >> shift) * 255 / max) as u8
}

/// The monitors, in the order `xrandr --listmonitors` and the windowing backend
/// list them (RandR 1.5). `None` if the server can't be asked or is too old.
pub fn monitors(display: Option<&str>) -> Option<Vec<Monitor>> {
    let (conn, screen) = RustConnection::connect(display).ok()?;
    let root = conn.setup().roots[screen].root;
    let reply = conn.randr_get_monitors(root, true).ok()?.reply().ok()?;
    reply
        .monitors
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let name = conn.get_atom_name(m.name).ok()?.reply().ok()?.name;
            Some(Monitor {
                index,
                name: String::from_utf8_lossy(&name).into_owned(),
                description: String::new(),
                x: m.x.into(),
                y: m.y.into(),
                width: m.width.into(),
                height: m.height.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::VirtualDisplay;
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};
    use x11rb::COPY_DEPTH_FROM_PARENT;

    #[test]
    fn sees_whether_a_compositor_owns_the_selection() {
        let Some(display) = VirtualDisplay::start() else { return };
        assert_eq!(has_compositor(Some(&display.name)), Some(false));

        // Become the compositor, as picom or a window manager's own would.
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
        let atom = conn.intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes()).unwrap().reply().unwrap().atom;
        conn.set_selection_owner(window, atom, x11rb::CURRENT_TIME).unwrap();
        conn.flush().unwrap();
        assert_eq!(has_compositor(Some(&display.name)), Some(true));
    }

    /// A window of one colour at `x`, `y`, as a bare X server shows it.
    fn paint(conn: &RustConnection, root: u32, (x, y, w, h): (i16, i16, u16, u16), color: u32) {
        use x11rb::protocol::xproto::ConnectionExt as _;
        let window = conn.generate_id().unwrap();
        let aux = CreateWindowAux::new().background_pixel(color).override_redirect(1);
        conn.create_window(COPY_DEPTH_FROM_PARENT, window, root, x, y, w, h, 0, WindowClass::INPUT_OUTPUT, 0, &aux)
            .unwrap();
        conn.map_window(window).unwrap();
        conn.flush().unwrap();
    }

    #[test]
    fn takes_a_picture_of_the_screen() {
        let Some(display) = VirtualDisplay::start() else { return };
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        paint(&conn, root, (0, 0, 320, 480), 0xff0000);
        paint(&conn, root, (320, 0, 320, 240), 0x0000ff);
        conn.get_input_focus().unwrap().reply().unwrap(); // the server has drawn them

        let (image, cursor) = screenshot(Some(&display.name)).unwrap();
        assert_eq!(image.dimensions(), (640, 480));
        assert_eq!(image.get_pixel(10, 10).0, [255, 0, 0, 255], "left half is red");
        assert_eq!(image.get_pixel(630, 10).0, [0, 0, 255, 255], "top right is blue");
        assert_eq!(image.get_pixel(10, 470).0, [255, 0, 0, 255]);
        let cursor = cursor.expect("Xvfb has XFixes");
        assert!(cursor.image.width() > 0 && cursor.image.height() > 0);
    }

    #[test]
    fn a_picture_taken_in_bands_has_no_seams() {
        // At 1100 pixels wide a band is 953 rows, so this screen takes three requests.
        let Some(display) = VirtualDisplay::start_sized("1100x2600x24") else { return };
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        paint(&conn, root, (0, 0, 1100, 1300), 0x00ff00);
        paint(&conn, root, (0, 1300, 1100, 1300), 0xffff00);
        conn.get_input_focus().unwrap().reply().unwrap();

        let (image, _) = screenshot(Some(&display.name)).unwrap();
        assert_eq!(image.dimensions(), (1100, 2600));
        for y in [0, 952, 953, 954, 1299] {
            assert_eq!(image.get_pixel(5, y).0, [0, 255, 0, 255], "row {y}");
        }
        for y in [1300, 1906, 1907, 1908, 2599] {
            assert_eq!(image.get_pixel(5, y).0, [255, 255, 0, 255], "row {y}");
        }
    }

    #[test]
    fn converts_the_pixel_layouts_a_server_may_use() {
        let bgrx = PixelFormat { red: 0xff0000, green: 0xff00, blue: 0xff, little_endian: true };
        assert_eq!(to_rgba(&[3, 2, 1, 0, 30, 20, 10, 0], 2, bgrx), [1, 2, 3, 255, 10, 20, 30, 255]);
        // The same pixel from a server that sends the most significant byte first.
        let big = PixelFormat { little_endian: false, ..bgrx };
        assert_eq!(to_rgba(&[0, 1, 2, 3], 1, big), [1, 2, 3, 255]);
        // 10 bits a channel: full intensity is still 255, and half is about half (rounded down).
        let ten = PixelFormat { red: 0x3ff0_0000, green: 0x000f_fc00, blue: 0x0000_03ff, little_endian: true };
        let value: u32 = (0x3ff << 20) | (0x200 << 10);
        assert_eq!(to_rgba(&value.to_le_bytes(), 1, ten), [255, 127, 0, 255]);
        // A short buffer gives only the whole pixels in it.
        assert_eq!(to_rgba(&[1, 2, 3, 4, 5], 2, bgrx).len(), 4);
    }

    #[test]
    fn draws_a_premultiplied_pointer_onto_the_picture() {
        let desktop = RgbaImage::from_pixel(4, 4, image::Rgba([100, 100, 100, 255]));
        let mut pointer = RgbaImage::new(2, 2);
        pointer.put_pixel(0, 0, image::Rgba([255, 255, 255, 255])); // opaque white
        pointer.put_pixel(1, 0, image::Rgba([50, 0, 0, 128])); // half-transparent red, premultiplied
        let drawn = with_cursor(&desktop, &Cursor { image: pointer, x: 1, y: 1 });
        assert_eq!(drawn.get_pixel(1, 1).0, [255, 255, 255, 255]);
        assert_eq!(drawn.get_pixel(2, 1).0, [99, 49, 49, 255], "50 + 100 * (1 - 128/255)");
        assert_eq!(drawn.get_pixel(0, 0).0, [100, 100, 100, 255], "away from the pointer nothing changes");
        assert_eq!(desktop.get_pixel(1, 1).0, [100, 100, 100, 255], "the original is untouched");
        // A pointer hanging off the edge is cut, not a panic.
        let _ = with_cursor(&desktop, &Cursor { image: RgbaImage::new(3, 3), x: -2, y: 3 });
    }

    #[test]
    fn cannot_tell_without_a_server() {
        assert_eq!(has_compositor(Some(":999")), None);
        assert!(monitors(Some(":999")).is_none());
        assert!(screenshot(Some(":999")).is_err());
    }

    #[test]
    fn lists_the_monitors_with_their_geometry() {
        use x11rb::protocol::randr::MonitorInfo;

        let Some(display) = VirtualDisplay::start() else { return };
        let all = monitors(Some(&display.name)).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].index, all[0].x, all[0].y, all[0].width, all[0].height), (0, 0, 0, 640, 480));
        assert!(!all[0].name.is_empty());

        // Cut the screen into two monitors, like a second output would.
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        for (name, x) in [("left", 0), ("right", 320)] {
            let atom = conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
            let info = MonitorInfo {
                name: atom,
                primary: false,
                automatic: false,
                x,
                y: 0,
                width: 320,
                height: 480,
                width_in_millimeters: 100,
                height_in_millimeters: 150,
                outputs: Vec::new(),
            };
            conn.randr_set_monitor(root, info).unwrap().check().unwrap();
        }
        let all = monitors(Some(&display.name)).unwrap();
        let find = |name: &str| all.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{name} in {all:?}"));
        assert_eq!((find("left").x, find("left").width, find("left").height), (0, 320, 480));
        assert_eq!((find("right").x, find("right").width, find("right").height), (320, 320, 480));
        // The index is the position in the list, which is how the windowing backend counts.
        assert!(all.iter().enumerate().all(|(i, m)| m.index == i));
    }
}
