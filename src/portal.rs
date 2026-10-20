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

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
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
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use zbus::blocking::connection::Builder;
    use zbus::zvariant::ObjectPath;

    /// A session bus of its own, so a test can play the portal. It starts no
    /// services: the default configuration would launch the real portal.
    struct PrivateBus {
        child: Child,
        address: String,
        _dir: tempfile::TempDir,
    }

    impl PrivateBus {
        /// `None` without `dbus-daemon`; set QUEIJO_REQUIRE_DBUS=1 to make that a failure (CI does).
        fn start() -> Option<Self> {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("bus.conf");
            std::fs::write(
                &config,
                format!(
                    "<busconfig><type>session</type><listen>unix:path={}</listen>\
                     <policy context=\"default\"><allow send_destination=\"*\"/><allow own=\"*\"/>\
                     <allow receive_sender=\"*\"/><allow receive_type=\"method_return\"/>\
                     <allow receive_type=\"error\"/><allow receive_type=\"signal\"/></policy>\
                     </busconfig>",
                    dir.path().join("bus.sock").display()
                ),
            )
            .unwrap();
            let child = Command::new("dbus-daemon")
                .arg("--nofork")
                .arg("--print-address=1")
                .arg(format!("--config-file={}", config.display()))
                .stdout(Stdio::piped())
                .spawn();
            let Ok(mut child) = child else {
                assert!(std::env::var_os("QUEIJO_REQUIRE_DBUS").is_none(), "dbus-daemon is required here");
                eprintln!("skipping: dbus-daemon is not installed");
                return None;
            };
            let mut address = String::new();
            BufReader::new(child.stdout.take().unwrap()).read_line(&mut address).unwrap();
            assert!(address.starts_with("unix:"), "dbus-daemon did not start: {address:?}");
            Some(PrivateBus { child, address: address.trim().to_string(), _dir: dir })
        }

        fn connect(&self) -> Connection {
            Builder::address(self.address.as_str()).unwrap().build().unwrap()
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

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

    #[test]
    fn a_missing_portal_is_an_error() {
        let Some(bus) = PrivateBus::start() else { return };
        assert!(choose_folder_on(&bus.connect(), "Choose", Path::new("/")).is_err());
    }
}
