//! Client for the "Queijo helper" GNOME Shell extension (see `gnome-extension/`),
//! which can list and capture windows — something GNOME offers no public API for.

use std::path::PathBuf;

use crate::windows::{crop_to_frame, shot, WindowInfo, WindowShot, WindowSource};

const DEST: &str = "org.gnome.Shell";

/// The extension for GNOME Shell 42 to 44, and the one for 45 and later (which changed
/// how extensions are written). Both are installed; only the one that fits the Shell loads.
pub const LEGACY_UUID: &str = "queijo@queijo.app";
pub const MODULE_UUID: &str = "queijo-gnome45@queijo.app";

/// The extension that fits GNOME Shell `major`.
pub fn uuid_for(major: u32) -> &'static str {
    if major >= 45 {
        MODULE_UUID
    } else {
        LEGACY_UUID
    }
}
pub(crate) const PATH: &str = "/app/queijo/Queijo";
pub(crate) const IFACE: &str = "app.queijo.Queijo";

type Row = (u64, String, String, String, i32, i32, i32, i32, i32, i32, i32, i32, bool);

/// What enabling the extension did.
#[derive(Debug, PartialEq, Eq)]
pub enum Enabled {
    /// It was added to the Shell's list of enabled extensions; log out and in to load it.
    Now(&'static str),
    AlreadyOn(&'static str),
}

/// Turns on the extension that fits this GNOME, by adding it to `enabled-extensions` (the
/// Shell only loads a newly installed extension at login, and `gnome-extensions enable`
/// refuses one it doesn't know yet).
pub fn enable() -> Result<Enabled, String> {
    let major = crate::desktop::Desktop::Gnome.major_version().ok_or("GNOME Shell is not installed")?;
    let uuid = uuid_for(major);
    let run = |args: &[&str]| -> Result<String, String> {
        let out = std::process::Command::new("gsettings").args(args).output().map_err(|e| format!("gsettings: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    };
    let current = run(&["get", "org.gnome.shell", "enabled-extensions"])?;
    match with_enabled(&current, uuid) {
        None => Ok(Enabled::AlreadyOn(uuid)),
        Some(list) => {
            run(&["set", "org.gnome.shell", "enabled-extensions", &list])?;
            Ok(Enabled::Now(uuid))
        }
    }
}

/// `current` (as `gsettings` prints the list) with `uuid` in it; `None` if it already is.
fn with_enabled(current: &str, uuid: &str) -> Option<String> {
    let mut list = crate::shortcut::parse_list(current);
    if list.iter().any(|u| u == uuid) {
        return None;
    }
    list.push(uuid.to_string());
    Some(crate::shortcut::gvariant_list(&list))
}

/// The whole screen as a PNG written to `path`, by the extension from inside the Shell
/// (which, unlike Queijo, the Shell has always answered). `Err` if the extension is not
/// running or its Shell has no such method.
pub fn screenshot(
    conn: &zbus::blocking::Connection,
    path: &std::path::Path,
    include_cursor: bool,
) -> Result<(), String> {
    let file = path.to_str().ok_or("non UTF-8 runtime dir")?;
    let reply = conn
        .call_method(Some(DEST), PATH, Some(IFACE), "Screenshot", &(file, include_cursor))
        .map_err(|e| format!("Screenshot: {e}"))?;
    let (ok,): (bool,) = reply.body().deserialize().map_err(|e| e.to_string())?;
    if ok {
        Ok(())
    } else {
        let _ = std::fs::remove_file(path);
        Err("Screenshot: the extension could not take it".into())
    }
}

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
    /// Every extension folder: `gnome-extension/<uuid>` and `gnome-extension/esm/<uuid>`.
    fn extensions() -> Vec<std::path::PathBuf> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gnome-extension");
        let folders = |parent: &std::path::Path| -> Vec<std::path::PathBuf> {
            std::fs::read_dir(parent)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.join("metadata.json").is_file())
                .collect()
        };
        let mut found = folders(&dir);
        found.extend(folders(&dir.join("esm")));
        found
    }

    #[test]
    fn adds_the_extension_to_the_enabled_ones_once() {
        assert_eq!(with_enabled("@as []", MODULE_UUID).unwrap(), "['queijo-gnome45@queijo.app']");
        assert_eq!(
            with_enabled("['appindicatorsupport@rgcjonas.gmail.com']", LEGACY_UUID).unwrap(),
            "['appindicatorsupport@rgcjonas.gmail.com', 'queijo@queijo.app']"
        );
        assert_eq!(with_enabled("['a@b', 'queijo@queijo.app']", LEGACY_UUID), None);
        // The other Queijo extension is another extension; this one is still added.
        assert!(with_enabled("['queijo@queijo.app']", MODULE_UUID).unwrap().contains("queijo-gnome45@queijo.app"));
    }

    #[test]
    fn picks_the_extension_that_fits_the_shell() {
        assert_eq!(uuid_for(42), LEGACY_UUID);
        assert_eq!(uuid_for(44), LEGACY_UUID);
        assert_eq!(uuid_for(45), MODULE_UUID);
        assert_eq!(uuid_for(49), MODULE_UUID);
        let names: Vec<_> = extensions().iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert!(names.contains(&LEGACY_UUID.to_string()) && names.contains(&MODULE_UUID.to_string()), "{names:?}");
    }

    /// The Shell versions each extension says it works with are the ones it was written for.
    #[test]
    fn each_extension_covers_its_shell_versions() {
        for extension in extensions() {
            let metadata = std::fs::read_to_string(extension.join("metadata.json")).unwrap();
            let versions: Vec<u32> = metadata
                .split("\"shell-version\"")
                .nth(1)
                .and_then(|rest| rest.split(['[', ']']).nth(1))
                .unwrap()
                .split(',')
                .map(|v| v.trim().trim_matches('"').parse().unwrap())
                .collect();
            let uuid = extension.file_name().unwrap().to_str().unwrap();
            assert!(!versions.is_empty() && versions.iter().all(|v| uuid_for(*v) == uuid), "{uuid}: {versions:?}");
        }
    }

    #[test]
    fn the_extension_uses_the_names_the_program_does() {
        for extension in extensions() {
            names_match(&extension);
        }
    }

    fn names_match(extension: &std::path::Path) {
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
