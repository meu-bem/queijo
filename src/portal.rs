//! Asking the desktop through `xdg-desktop-portal`, which every modern desktop
//! provides: a file chooser, screenshots and screen casts that work the same on
//! GNOME, KDE and the rest.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{DynamicType, OwnedValue, Value};
use zbus::MatchRule;

pub(crate) const DESTINATION: &str = "org.freedesktop.portal.Desktop";
pub(crate) const PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// The `a{sv}` argument every portal request ends with.
pub type Options = HashMap<&'static str, Value<'static>>;

pub struct Response {
    /// 0: done, 1: the user cancelled, 2: it ended some other way.
    pub code: u32,
    pub results: HashMap<String, OwnedValue>,
}

/// Calls `interface.method` of the portal and waits for the answer, which
/// arrives as a `Response` signal on a request object the call returns.
/// `body` builds the arguments from the options, which already carry the
/// `handle_token` that names the request. There is no timeout: the portal waits
/// as long as its dialog is open.
pub fn request<B>(
    conn: &Connection,
    interface: &str,
    method: &str,
    body: impl FnOnce(Options) -> B,
) -> Result<Response, String>
where
    B: serde::Serialize + DynamicType,
{
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let token = format!("queijo_{}_{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed));
    let mut options = Options::new();
    options.insert("handle_token", Value::from(token.clone()));

    // The answer can come before the call returns, so listen first. Every
    // request's signal is matched, and the right one picked out by its path.
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(DESTINATION)
        .and_then(|b| b.interface(REQUEST))
        .and_then(|b| b.member("Response"))
        .map(|b| b.build())
        .map_err(|e| e.to_string())?;
    let responses = MessageIterator::for_match_rule(rule, conn, None).map_err(|e| e.to_string())?;

    let reply = conn
        .call_method(Some(DESTINATION), PATH, Some(interface), method, &body(options))
        .map_err(|e| e.to_string())?;
    let (handle,): (zbus::zvariant::OwnedObjectPath,) = reply.body().deserialize().map_err(|e| e.to_string())?;

    for message in responses {
        let message = message.map_err(|e| e.to_string())?;
        if message.header().path().map(|p| p.as_str()) != Some(handle.as_str()) {
            continue;
        }
        let (code, results): (u32, HashMap<String, OwnedValue>) =
            message.body().deserialize().map_err(|e| e.to_string())?;
        return Ok(Response { code, results });
    }
    Err("the connection closed before the portal answered".into())
}

/// Asks the file chooser for a folder, opening at `start`. `Ok(None)` is the
/// user cancelling; an error means the portal could not be used at all.
pub fn choose_folder(title: &str, start: &Path) -> Result<Option<String>, String> {
    choose_folder_on(&Connection::session().map_err(|e| e.to_string())?, title, start)
}

fn choose_folder_on(conn: &Connection, title: &str, start: &Path) -> Result<Option<String>, String> {
    let response = request(conn, "org.freedesktop.portal.FileChooser", "OpenFile", |mut options| {
        options.insert("directory", true.into());
        options.insert("modal", true.into());
        // The folder to open is a NUL-terminated byte string.
        let mut folder = start.as_os_str().as_encoded_bytes().to_vec();
        folder.push(0);
        options.insert("current_folder", folder.into());
        ("", title.to_owned(), options)
    })?;
    match response.code {
        0 => {
            let uris = response.results.get("uris").ok_or("the portal returned no folder")?;
            let uris: Vec<String> = uris.try_clone().and_then(Vec::<String>::try_from).map_err(|e| e.to_string())?;
            Ok(uris.first().and_then(|uri| uri_to_path(uri)))
        }
        1 => Ok(None),
        code => Err(format!("the portal ended the request with code {code}")),
    }
}

/// A picture of the whole desktop from the Screenshot portal, which every
/// desktop answers (GNOME may ask the user once). The portal saves it to a
/// file; it is read, and deleted if the portal put it in a temporary folder.
pub fn screenshot() -> Result<image::RgbaImage, String> {
    screenshot_on(&Connection::session().map_err(|e| e.to_string())?)
}

fn screenshot_on(conn: &Connection) -> Result<image::RgbaImage, String> {
    let response = request(conn, "org.freedesktop.portal.Screenshot", "Screenshot", |mut options| {
        options.insert("interactive", false.into());
        ("", options)
    })?;
    match response.code {
        0 => {}
        1 => return Err("the portal did not allow the screenshot".into()),
        code => return Err(format!("the portal ended the request with code {code}")),
    }
    let uri = response.results.get("uri").ok_or("the portal returned no picture")?;
    let uri = uri.try_clone().and_then(String::try_from).map_err(|e| e.to_string())?;
    let path = uri_to_path(&uri).ok_or_else(|| format!("the portal's picture is not a local file: {uri}"))?;
    let image = image::open(&path).map_err(|e| format!("{path}: {e}"));
    if is_temporary(Path::new(&path)) {
        let _ = std::fs::remove_file(&path);
    }
    Ok(image?.to_rgba8())
}

/// Whether the file lives somewhere that is cleaned up by itself, and so is ours to delete.
fn is_temporary(path: &Path) -> bool {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
    [std::env::temp_dir(), "/tmp".into(), "/var/tmp".into()].into_iter().chain(runtime).any(|dir| path.starts_with(dir))
}

/// The path in a `file://` address, with its `%20`-style escapes undone.
fn uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // "file://host/path" is a path on another host; "file:///path" is ours.
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::PrivateBus;
    use zbus::blocking::connection::Builder;
    use zbus::zvariant::ObjectPath;

    /// Answers one `OpenFile` like the portal does, sending the `Response`
    /// signal *before* the reply that names the request, which the real one may do.
    fn fake_portal(
        bus: &PrivateBus,
        code: u32,
        uri: &'static str,
    ) -> std::thread::JoinHandle<HashMap<String, OwnedValue>> {
        let service = Builder::address(bus.address.as_str()).unwrap().name(DESTINATION).unwrap().build().unwrap();
        // Listen before the client can call, or its call could be missed.
        let calls = MessageIterator::from(service.clone());
        std::thread::spawn(move || {
            for message in calls {
                let message = message.unwrap();
                if message.header().member().map(|m| m.as_str()) != Some("OpenFile") {
                    continue;
                }
                let (_, _, options): (String, String, HashMap<String, OwnedValue>) =
                    message.body().deserialize().unwrap();
                let token = String::try_from(options["handle_token"].try_clone().unwrap()).unwrap();
                let sender = message.header().sender().unwrap().as_str().trim_start_matches(':').replace('.', "_");
                let handle = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");

                let mut results = HashMap::new();
                results.insert("uris", Value::from(vec![uri.to_string()]));
                service.emit_signal(None::<&str>, handle.as_str(), REQUEST, "Response", &(code, results)).unwrap();
                service.reply(&message.header(), &(ObjectPath::try_from(handle).unwrap(),)).unwrap();
                return options;
            }
            unreachable!("the client never called")
        })
    }

    #[test]
    fn turns_addresses_into_paths() {
        assert_eq!(uri_to_path("file:///home/ann/Pictures").as_deref(), Some("/home/ann/Pictures"));
        assert_eq!(uri_to_path("file:///home/ann/My%20Shots").as_deref(), Some("/home/ann/My Shots"));
        assert_eq!(uri_to_path("file:///tmp/%C3%A1gua").as_deref(), Some("/tmp/água"));
        assert_eq!(uri_to_path("file://localhost/tmp").as_deref(), Some("/tmp"));
        assert_eq!(uri_to_path("file:///100%").as_deref(), Some("/100%"));
        assert_eq!(uri_to_path("https://example.com/x"), None);
        assert_eq!(uri_to_path("file://nopath"), None);
    }

    #[test]
    fn gets_the_answer_even_when_it_comes_before_the_reply() {
        let Some(bus) = PrivateBus::start() else { return };
        let portal = fake_portal(&bus, 0, "file:///home/ann/My%20Shots");
        let chosen = choose_folder_on(&bus.connect(), "Choose", Path::new("/home/ann")).unwrap();
        assert_eq!(chosen.as_deref(), Some("/home/ann/My Shots"));

        let options = portal.join().unwrap();
        assert!(bool::try_from(options["directory"].try_clone().unwrap()).unwrap());
        let folder = Vec::<u8>::try_from(options["current_folder"].try_clone().unwrap()).unwrap();
        assert_eq!(folder, b"/home/ann\0");
    }

    #[test]
    fn cancelling_is_not_an_error() {
        let Some(bus) = PrivateBus::start() else { return };
        let _portal = fake_portal(&bus, 1, "file:///unused");
        assert_eq!(choose_folder_on(&bus.connect(), "Choose", Path::new("/")).unwrap(), None);
    }

    /// Opens the real folder chooser and waits for someone to answer it.
    #[test]
    #[ignore = "needs a desktop and a person"]
    fn asks_the_real_portal() {
        let chosen = choose_folder("Queijo test", Path::new("/tmp"));
        eprintln!("the portal said: {chosen:?}");
    }

    /// Answers one `Screenshot` call like the portal: a picture in a file, and its address in the response.
    fn fake_screenshot_portal(bus: &PrivateBus, code: u32, uri: String) -> std::thread::JoinHandle<()> {
        let service = Builder::address(bus.address.as_str()).unwrap().name(DESTINATION).unwrap().build().unwrap();
        let calls = MessageIterator::from(service.clone());
        std::thread::spawn(move || {
            for message in calls {
                let message = message.unwrap();
                if message.header().member().map(|m| m.as_str()) != Some("Screenshot") {
                    continue;
                }
                let (_, options): (String, HashMap<String, OwnedValue>) = message.body().deserialize().unwrap();
                let token = String::try_from(options["handle_token"].try_clone().unwrap()).unwrap();
                assert!(!bool::try_from(options["interactive"].try_clone().unwrap()).unwrap());
                let sender = message.header().sender().unwrap().as_str().trim_start_matches(':').replace('.', "_");
                let handle = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
                let mut results = HashMap::new();
                results.insert("uri", Value::from(uri));
                service.emit_signal(None::<&str>, handle.as_str(), REQUEST, "Response", &(code, results)).unwrap();
                service.reply(&message.header(), &(ObjectPath::try_from(handle).unwrap(),)).unwrap();
                return;
            }
        })
    }

    #[test]
    fn reads_the_screenshot_the_portal_saved_and_removes_it() {
        let Some(bus) = PrivateBus::start() else { return };
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Shot one.png");
        image::RgbaImage::from_pixel(8, 4, image::Rgba([10, 20, 30, 255])).save(&file).unwrap();
        let uri = format!("file://{}", file.display().to_string().replace(' ', "%20"));
        let portal = fake_screenshot_portal(&bus, 0, uri);

        let image = screenshot_on(&bus.connect()).unwrap();
        portal.join().unwrap();
        assert_eq!(image.dimensions(), (8, 4));
        assert_eq!(image.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert!(!file.exists(), "a picture in a temporary folder is ours to delete");
    }

    /// Asks the real portal for a screenshot (GNOME may ask the user to allow it).
    #[test]
    #[ignore = "needs a desktop"]
    fn takes_a_screenshot_from_the_real_portal() {
        let shot = screenshot().unwrap();
        eprintln!("the portal's picture is {}x{}", shot.width(), shot.height());
    }

    #[test]
    fn a_refused_screenshot_is_an_error() {
        let Some(bus) = PrivateBus::start() else { return };
        let _portal = fake_screenshot_portal(&bus, 1, "file:///unused".into());
        let err = screenshot_on(&bus.connect()).unwrap_err();
        assert!(err.contains("did not allow"), "{err}");
    }

    #[test]
    fn only_deletes_what_is_in_a_temporary_folder() {
        assert!(is_temporary(Path::new("/tmp/x.png")));
        assert!(is_temporary(&std::env::temp_dir().join("x.png")));
        assert!(!is_temporary(Path::new("/home/ann/Pictures/Screenshot.png")));
        assert!(!is_temporary(Path::new("/tmpfoo/x.png")), "whole path components, not a prefix of the text");
    }

    #[test]
    fn a_missing_portal_is_an_error() {
        let Some(bus) = PrivateBus::start() else { return };
        assert!(choose_folder_on(&bus.connect(), "Choose", Path::new("/")).is_err());
    }
}
