//! Client for the "Queijo helper" GNOME Shell extension (see `gnome-extension/`),
//! which can list and capture windows — something GNOME offers no public API for.

use std::path::PathBuf;

use crate::windows::{crop_to_frame, shot, WindowInfo, WindowShot, WindowSource};

const DEST: &str = "org.gnome.Shell";
pub(crate) const PATH: &str = "/app/queijo/Queijo";
pub(crate) const IFACE: &str = "app.queijo.Queijo";

type Row = (u64, String, String, String, i32, i32, i32, i32, i32, i32, i32, i32, bool);

/// The extension, reached over the session bus.
pub struct ShellExtension {
    conn: zbus::blocking::Connection,
}

impl ShellExtension {
    /// `None` when the extension isn't running (not installed, or not loaded yet).
    pub fn connect() -> Option<Self> {
        let source = ShellExtension { conn: zbus::blocking::Connection::session().ok()? };
        source.list().map(|_| source)
    }
}

impl WindowSource for ShellExtension {
    fn name(&self) -> &'static str {
        "the Queijo GNOME Shell extension"
    }

    fn list(&self) -> Option<Vec<WindowInfo>> {
        let reply = self.conn.call_method(Some(DEST), PATH, Some(IFACE), "ListWindows", &()).ok()?;
        let (rows,): (Vec<Row>,) = reply.body().deserialize().ok()?;
        Some(
            rows.into_iter()
                .map(|(id, title, app, _class, fx, fy, fw, fh, bx, by, bw, bh, focused)| WindowInfo {
                    id,
                    title,
                    app,
                    frame: [fx, fy, fw, fh],
                    buffer: [bx, by, bw, bh],
                    focused,
                })
                .collect(),
        )
    }

    /// Captures every window (in parallel) and cuts each down to its frame.
    fn shots(&self, windows: Vec<WindowInfo>) -> Vec<WindowShot> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let jobs: Vec<_> = windows
            .into_iter()
            .map(|w| {
                let path = dir.join(format!("queijo-{}-win-{}.png", std::process::id(), w.id));
                let ok = path.to_str().is_some_and(|file| {
                    self.conn
                        .call_method(Some(DEST), PATH, Some(IFACE), "CaptureWindow", &(w.id, file))
                        .ok()
                        .and_then(|r| r.body().deserialize::<(bool,)>().ok())
                        .is_some_and(|(ok,)| ok)
                });
                std::thread::spawn(move || {
                    let image = ok.then(|| image::open(&path).ok()).flatten().map(|i| i.to_rgba8());
                    let _ = std::fs::remove_file(&path);
                    let image = image.and_then(|img| crop_to_frame(&img, w.frame, w.buffer));
                    shot(w, image)
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The program and the Shell extension agree on every D-Bus name and path; a
    /// mismatch would silently switch off the window picker and the indicator.
    #[test]
    fn the_extension_uses_the_names_the_program_does() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gnome-extension");
        let extension = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .expect("the extension folder");
        let js = std::fs::read_to_string(extension.join("extension.js")).unwrap();
        // The one field we need, without pulling in a JSON library for a test.
        let metadata = std::fs::read_to_string(extension.join("metadata.json")).unwrap();
        let uuid = metadata.split("\"uuid\"").nth(1).and_then(|rest| rest.split('"').nth(1)).expect("a uuid");

        assert!(js.contains(&format!("const OBJECT_PATH = '{PATH}';")), "service path");
        assert!(js.contains(&format!("<interface name=\"{IFACE}\">")), "service interface");
        let recorder = crate::record::RECORDER_NAME;
        assert!(js.contains(&format!("const RECORDER_NAME = '{recorder}';")), "recorder name");
        assert!(js.contains(&format!("const RECORDER_PATH = '{}';", crate::record::RECORDER_PATH)), "recorder path");
        assert!(
            include_str!("record/mod.rs").contains(&format!("#[zbus::interface(name = \"{recorder}\")]")),
            "the recorder object's interface is named like its bus name"
        );
        assert_eq!(uuid, extension.file_name().unwrap().to_str().unwrap(), "folder name is the uuid");
    }
}
