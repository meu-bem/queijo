//! KDE Plasma's own screenshot service, `org.kde.KWin.ScreenShot2`.
//!
//! KWin only answers programs whose launcher (`queijo.desktop`) lists the
//! interface under `X-KDE-DBUS-Restricted-Interfaces`, matched by the program's
//! path. It does not write a file: it writes the raw pixels into a pipe we hand it.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::OwnedFd;

use image::RgbaImage;
use zbus::blocking::Connection;
use zbus::zvariant::{Fd, OwnedValue, Value};

const DESTINATION: &str = "org.kde.KWin";
const PATH: &str = "/org/kde/KWin/ScreenShot2";
const INTERFACE: &str = "org.kde.KWin.ScreenShot2";

/// A picture of the whole workspace (every monitor), at the monitors' own resolution.
pub fn screenshot() -> Result<RgbaImage, String> {
    let conn = Connection::session().map_err(|e| e.to_string())?;
    capture(&conn, DESTINATION, "CaptureWorkspace", &[("native-resolution", true)])
}

/// The window that has the focus, with its title bar and without its shadow.
pub fn active_window() -> Result<RgbaImage, String> {
    let conn = Connection::session().map_err(|e| e.to_string())?;
    let options = [("native-resolution", true), ("include-decoration", true), ("include-shadow", false)];
    capture(&conn, DESTINATION, "CaptureActiveWindow", &options)
}

/// Asks `method` for a picture, with `options`, and reads it from the pipe.
fn capture(conn: &Connection, destination: &str, method: &str, options: &[(&str, bool)]) -> Result<RgbaImage, String> {
    let (mut reader, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let writer = OwnedFd::from(writer);
    // KWin finishes writing before it answers, and a pipe holds little: read meanwhile.
    let reading = std::thread::spawn(move || {
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map(|_| data)
    });

    let options: HashMap<&str, Value> = options.iter().map(|&(key, on)| (key, on.into())).collect();
    let reply = conn.call_method(Some(destination), PATH, Some(INTERFACE), method, &(options, Fd::from(&writer)));
    // Our end must be closed, or the reader never sees the end of the data.
    drop(writer);
    let reply = reply.map_err(|e| e.to_string())?;
    let results: HashMap<String, OwnedValue> = reply.body().deserialize().map_err(|e| e.to_string())?;
    let data = reading.join().map_err(|_| "the reader panicked".to_string())?.map_err(|e| e.to_string())?;

    let number = |key: &str| -> Result<u32, String> {
        let value = results.get(key).ok_or(format!("KWin's answer has no {key}"))?;
        u32::try_from(value.try_clone().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    };
    decode(&data, number("width")?, number("height")?, number("stride")?, number("format")?)
}

/// KWin's pixels are a `QImage`: `stride` bytes a row, in `format` (Qt's enum).
fn decode(data: &[u8], width: u32, height: u32, stride: u32, format: u32) -> Result<RgbaImage, String> {
    // The byte order of each pixel, as the positions of red, green and blue in it.
    let (r, g, b) = match format {
        // RGB32, ARGB32 and ARGB32_Premultiplied: a 32-bit word 0xAARRGGBB, little-endian here.
        4..=6 => (2, 1, 0),
        // RGBX8888, RGBA8888 and RGBA8888_Premultiplied.
        16..=18 => (0, 1, 2),
        other => return Err(format!("KWin sent pixels in format {other}, which is not supported")),
    };
    let (width, height, stride) = (width as usize, height as usize, stride as usize);
    if width == 0 || height == 0 || stride < width * 4 || data.len() < stride * (height - 1) + width * 4 {
        return Err(format!("KWin sent {} bytes for {width}x{height} pixels", data.len()));
    }
    // A screen is opaque, whatever the alpha channel says.
    let mut out = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for px in data[row * stride..][..width * 4].chunks_exact(4) {
            out.extend([px[r], px[g], px[b], 255]);
        }
    }
    RgbaImage::from_raw(width as u32, height as u32, out).ok_or_else(|| "the picture has the wrong size".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::PrivateBus;
    use std::io::Write;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::MessageIterator;
    use zbus::zvariant::OwnedFd as BusFd;

    /// Pixels as Qt lays them out: 2x2, 12 bytes a row (4 of padding).
    fn argb32(pixels: [[u8; 3]; 4]) -> Vec<u8> {
        let mut data = Vec::new();
        for row in pixels.chunks(2) {
            for [r, g, b] in row {
                data.extend([*b, *g, *r, 0xff]);
            }
            data.extend([0xee; 4]);
        }
        data
    }

    #[test]
    fn decodes_qt_images() {
        let data = argb32([[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]);
        let image = decode(&data, 2, 2, 12, 5).unwrap();
        assert_eq!(image.dimensions(), (2, 2));
        assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [4, 5, 6, 255]);
        assert_eq!(image.get_pixel(0, 1).0, [7, 8, 9, 255], "the padding at the end of a row is skipped");
        assert_eq!(image.get_pixel(1, 1).0, [10, 11, 12, 255]);

        // RGBA8888 has red first; and alpha is ignored.
        let rgba = [1, 2, 3, 0, 4, 5, 6, 0];
        assert_eq!(decode(&rgba, 2, 1, 8, 17).unwrap().get_pixel(1, 0).0, [4, 5, 6, 255]);
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(decode(&[0; 16], 2, 2, 8, 30).unwrap_err().contains("format 30"));
        assert!(decode(&[0; 15], 2, 2, 8, 5).is_err(), "too little data");
        assert!(decode(&[0; 16], 2, 2, 4, 5).is_err(), "a row can't be shorter than its pixels");
        assert!(decode(&[], 0, 0, 0, 5).is_err());
    }

    /// Plays KWin: takes the pipe from the call, writes `data` into it, and answers.
    fn fake_kwin(bus: &PrivateBus, data: Vec<u8>, format: u32) -> std::thread::JoinHandle<HashMap<String, OwnedValue>> {
        let service = Builder::address(bus.address.as_str()).unwrap().name(DESTINATION).unwrap().build().unwrap();
        let calls = MessageIterator::from(service.clone());
        std::thread::spawn(move || {
            for message in calls {
                let message = message.unwrap();
                let member = message.header().member().map(|m| m.to_string());
                if !matches!(member.as_deref(), Some("CaptureWorkspace" | "CaptureActiveWindow")) {
                    continue;
                }
                let (options, pipe): (HashMap<String, OwnedValue>, BusFd) = message.body().deserialize().unwrap();
                assert!(bool::try_from(options["native-resolution"].try_clone().unwrap()).unwrap());
                let mut pipe = std::fs::File::from(OwnedFd::from(pipe));
                // More than a pipe holds, so the call only returns if it is read meanwhile.
                let mut big = data.clone();
                big.extend(std::iter::repeat_n(0xee, 1 << 20));
                pipe.write_all(&big).unwrap();
                drop(pipe);

                let mut results: HashMap<&str, Value> = HashMap::new();
                results.insert("type", "raw".into());
                results.insert("width", 2u32.into());
                results.insert("height", 2u32.into());
                results.insert("stride", 12u32.into());
                results.insert("format", format.into());
                service.reply(&message.header(), &(results,)).unwrap();
                return options;
            }
            unreachable!("the client never called")
        })
    }

    #[test]
    fn takes_a_screenshot_through_a_pipe() {
        let Some(bus) = PrivateBus::start() else { return };
        let kwin = fake_kwin(&bus, argb32([[255, 0, 0], [0, 255, 0], [0, 0, 255], [9, 9, 9]]), 6);
        let image = capture(&bus.connect(), DESTINATION, "CaptureWorkspace", &[("native-resolution", true)]).unwrap();
        kwin.join().unwrap();
        assert_eq!(image.dimensions(), (2, 2));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(1, 0).0, [0, 255, 0, 255]);
        assert_eq!(image.get_pixel(0, 1).0, [0, 0, 255, 255]);
    }

    #[test]
    fn asks_for_the_active_window_with_its_title_bar_and_no_shadow() {
        let Some(bus) = PrivateBus::start() else { return };
        let kwin = fake_kwin(&bus, argb32([[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]), 5);
        let options = [("native-resolution", true), ("include-decoration", true), ("include-shadow", false)];
        let image = capture(&bus.connect(), DESTINATION, "CaptureActiveWindow", &options).unwrap();
        assert_eq!(image.get_pixel(1, 1).0, [10, 11, 12, 255]);
        let sent = kwin.join().unwrap();
        assert!(bool::try_from(sent["include-decoration"].try_clone().unwrap()).unwrap());
        assert!(!bool::try_from(sent["include-shadow"].try_clone().unwrap()).unwrap());
    }

    #[test]
    fn an_unsupported_format_is_an_error_not_garbage() {
        let Some(bus) = PrivateBus::start() else { return };
        let _kwin = fake_kwin(&bus, argb32([[0; 3]; 4]), 99);
        let err = capture(&bus.connect(), DESTINATION, "CaptureWorkspace", &[("native-resolution", true)]).unwrap_err();
        assert!(err.contains("format 99"), "{err}");
    }

    #[test]
    fn without_kwin_it_is_an_error() {
        let Some(bus) = PrivateBus::start() else { return };
        assert!(capture(&bus.connect(), DESTINATION, "CaptureWorkspace", &[]).is_err());
    }
}
