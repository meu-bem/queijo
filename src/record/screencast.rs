//! Recording through `xdg-desktop-portal`'s ScreenCast and PipeWire, which any
//! desktop can offer. The user picks a screen in the desktop's own dialog; the
//! video is a PipeWire stream, which `gst-launch-1.0` crops to the area and encodes.

use std::collections::HashMap;
use std::os::fd::{AsRawFd, OwnedFd};
use std::process::Command;

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use super::process::{self, missing_tool, Recorder, Stop};
use super::{Failure, Job, Session};
use crate::portal::{request, Options, DESTINATION, PATH};

const INTERFACE: &str = "org.freedesktop.portal.ScreenCast";

/// A rectangle in the desktop's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Area {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Area {
    fn intersect(self, other: Area) -> Option<Area> {
        let (x, y) = (self.x.max(other.x), self.y.max(other.y));
        let (right, bottom) = ((self.x + self.w).min(other.x + other.w), (self.y + self.h).min(other.y + other.h));
        (right > x && bottom > y).then_some(Area { x, y, w: right - x, h: bottom - y })
    }

    fn contains(self, other: Area) -> bool {
        self.intersect(other) == Some(other)
    }
}

/// What to cut off each side of a stream to leave the area, in the stream's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Crop {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

/// The part of `stream` that is the `area`, made even-sized as the encoder wants;
/// `None` if the two don't meet.
fn crop(area: Area, stream: Area) -> Option<Crop> {
    let part = area.intersect(stream)?;
    let (w, h) = (part.w & !1, part.h & !1);
    if w < 2 || h < 2 {
        return None;
    }
    let (left, top) = (part.x - stream.x, part.y - stream.y);
    Some(Crop { left, top, right: stream.w - left - w, bottom: stream.h - top - h })
}

/// What the portal handed over for a recording.
struct Granted {
    session: ObjectPath<'static>,
    /// The PipeWire connection, which the encoder reads the stream from.
    fd: OwnedFd,
    node: u32,
    /// Where the picked screen is on the desktop, if the portal says.
    stream: Option<Area>,
    token: Option<String>,
}

pub fn start(job: &Job) -> Result<Box<dyn Session>, Failure> {
    let conn = match job.conn {
        Some(conn) => conn.clone(),
        None => Connection::session().map_err(|e| e.to_string())?,
    };
    let area = Area { x: job.req.x, y: job.req.y, w: job.width, h: job.height };
    let token = job.cfg.screencast_token.as_str();
    let mut granted = negotiate(&conn, job.req.pointer, token)?;
    // A remembered screen is not asked about again, but it may not be where the area is.
    if !token.is_empty() && granted.stream.is_some_and(|s| !s.contains(area)) {
        close(&conn, &granted.session);
        granted = negotiate(&conn, job.req.pointer, "")?;
    }
    remember(granted.token.as_deref(), token);
    Ok(Box::new(launch(job, conn, granted, area)?))
}

/// Keeps the portal's token, so the same screen can be picked again without the dialog.
fn remember(new: Option<&str>, old: &str) {
    if let Some(new) = new.filter(|t| !t.is_empty() && *t != old) {
        let mut cfg = crate::config::Config::load();
        cfg.screencast_token = new.to_string();
        let _ = cfg.save();
    }
}

fn handle(value: &OwnedValue) -> Result<ObjectPath<'static>, String> {
    // The portal says the session handle is a string; the object path it names is what we need.
    if let Ok(text) = value.try_clone().and_then(String::try_from) {
        return ObjectPath::try_from(text).map_err(|e| e.to_string());
    }
    let path = value.try_clone().and_then(OwnedObjectPath::try_from).map_err(|e| e.to_string())?;
    Ok(path.into_inner())
}

/// The portal's four steps: create a session, say what to record, let the user
/// pick it (`Start`), and open the PipeWire connection.
fn negotiate(conn: &Connection, pointer: bool, token: &str) -> Result<Granted, Failure> {
    let created = request(conn, INTERFACE, "CreateSession", |mut options| {
        options.insert("session_handle_token", Value::from(format!("queijo_session_{}", std::process::id())));
        (options,)
    })?;
    ended(created.code)?;
    let session = handle(created.results.get("session_handle").ok_or("the portal made no session")?)?;

    let selected = request(conn, INTERFACE, "SelectSources", |mut options: Options| {
        options.insert("types", 1u32.into()); // monitors
        options.insert("multiple", false.into());
        options.insert("cursor_mode", (if pointer { 2u32 } else { 1u32 }).into()); // drawn in, or hidden
        options.insert("persist_mode", 2u32.into()); // until revoked
        if !token.is_empty() {
            options.insert("restore_token", Value::from(token.to_string()));
        }
        (&session, options)
    })?;
    ended(selected.code)?;

    let started = request(conn, INTERFACE, "Start", |options| (&session, "", options))?;
    ended(started.code)?;
    let streams = started.results.get("streams").ok_or("the portal gave no stream")?;
    let streams = Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(streams.try_clone().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let (node, props) = streams.into_iter().next().ok_or("the portal gave no stream")?;
    let pair = |key: &str| props.get(key).and_then(|v| v.try_clone().ok()).and_then(|v| <(i32, i32)>::try_from(v).ok());
    let stream = pair("position").zip(pair("size")).map(|((x, y), (w, h))| Area { x, y, w, h });
    let token =
        started.results.get("restore_token").and_then(|v| v.try_clone().ok()).and_then(|v| String::try_from(v).ok());

    let options: HashMap<&str, Value> = HashMap::new();
    let reply = conn
        .call_method(Some(DESTINATION), PATH, Some(INTERFACE), "OpenPipeWireRemote", &(&session, options))
        .map_err(|e| e.to_string())?;
    let (fd,): (zbus::zvariant::OwnedFd,) = reply.body().deserialize().map_err(|e| e.to_string())?;
    Ok(Granted { session, fd: fd.into(), node, stream, token })
}

/// What a response code means: 0 is the go-ahead, 1 the user closing the dialog.
fn ended(code: u32) -> Result<(), Failure> {
    match code {
        0 => Ok(()),
        1 => Err(Failure::Cancelled),
        other => Err(Failure::Unavailable(format!("the portal ended the request with code {other}"))),
    }
}

fn close(conn: &Connection, session: &ObjectPath) {
    let _ = conn.call_method(Some(DESTINATION), session.as_str(), Some("org.freedesktop.portal.Session"), "Close", &());
}

/// The arguments of `gst-launch-1.0`: `source` (a PipeWire stream), scaled to the
/// screen's logical size so the crop is in the same units, cropped, given a steady
/// frame rate and encoded. A GIF's clip is lossless.
fn pipeline(job: &Job, source: &[String], stream: Option<Area>, crop: Option<Crop>) -> Vec<String> {
    let mut args: Vec<String> = vec!["-e".into()];
    args.extend(source.iter().cloned());
    args.extend(["!", "queue", "!", "videoconvert", "!", "videoscale", "!"].map(String::from));
    match stream {
        Some(s) => args.push(format!("video/x-raw,width={},height={},pixel-aspect-ratio=1/1", s.w, s.h)),
        None => args.push("video/x-raw".into()),
    }
    if let Some(c) = crop {
        args.extend(["!".into(), "videocrop".into()]);
        args.extend([format!("left={}", c.left), format!("top={}", c.top)]);
        args.extend([format!("right={}", c.right), format!("bottom={}", c.bottom)]);
    }
    args.extend(["!".into(), "videorate".into(), "!".into(), format!("video/x-raw,framerate={}/1", job.fps)]);
    args.extend(["!", "videoconvert", "!"].map(String::from));
    if job.req.gif {
        args.extend(["avenc_ffv1", "!", "matroskamux"].map(String::from));
    } else {
        args.extend(
            ["vp8enc", "deadline=1", "cpu-used=8", "threads=4", "end-usage=vbr", "target-bitrate=8000000"]
                .map(String::from),
        );
        args.extend(["!", "webmmux"].map(String::from));
    }
    let path = job.path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    args.extend(["!".into(), "filesink".into(), format!("location=\"{path}\"")]);
    args
}

/// The encoder is another program: the connection must not close when it starts.
fn keep_open_on_exec(fd: &OwnedFd) -> Result<(), String> {
    rustix::io::fcntl_setfd(fd, rustix::io::FdFlags::empty()).map_err(|e| e.to_string())
}

/// Starts the encoder on the PipeWire stream `node`, read through the connection `fd`.
fn spawn_encoder(
    job: &Job,
    fd: &OwnedFd,
    node: u32,
    stream: Option<Area>,
    crop: Option<Crop>,
) -> Result<Recorder, String> {
    keep_open_on_exec(fd)?;
    let source = vec![
        "pipewiresrc".to_string(),
        format!("fd={}", fd.as_raw_fd()),
        format!("path={node}"),
        "do-timestamp=true".into(),
    ];
    let mut command = Command::new("gst-launch-1.0");
    command.args(pipeline(job, &source, stream, crop));
    process::spawn("gst-launch-1.0", command, Stop::Interrupt, job.path.clone(), job.ended.clone())
        .map_err(|e| missing_tool(e, "gst-launch-1.0", "gstreamer1.0-tools, gstreamer1.0-pipewire"))
}

struct Cast {
    recorder: Recorder,
    conn: Connection,
    session: ObjectPath<'static>,
    /// Kept open until the encoder has finished with it.
    _fd: OwnedFd,
}

fn launch(job: &Job, conn: Connection, granted: Granted, area: Area) -> Result<Cast, Failure> {
    let crop_to = granted.stream.and_then(|stream| crop(area, stream));
    if granted.stream.is_none() || crop_to.is_none() {
        // The picked screen can't be cut down to the area (the portal didn't say where it
        // is, or the area is on another screen): the whole screen is recorded.
        crate::output::notify(
            crate::i18n::t("notify.record_whole_screen"),
            crate::i18n::t("notify.record_whole_screen_body"),
            None,
        );
    }
    let stream = granted.stream.filter(|_| crop_to.is_some());
    let recorder = spawn_encoder(job, &granted.fd, granted.node, stream, crop_to)?;
    let key = crate::shortcut::find().unwrap_or_else(|| crate::i18n::t("record.shortcut_fallback").to_string());
    crate::output::notify(
        crate::i18n::t("notify.recording_started"),
        &crate::i18n::tf("notify.recording_started_body", &[("shortcut", &key)]),
        None,
    );
    Ok(Cast { recorder, conn, session: granted.session, _fd: granted.fd })
}

impl Session for Cast {
    fn stop(self: Box<Self>) -> Result<std::path::PathBuf, String> {
        let Cast { recorder, conn, session, _fd } = *self;
        let result = Box::new(recorder).stop();
        close(&conn, &session);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Request;
    use crate::testutil::PrivateBus;
    use std::sync::mpsc;
    use zbus::blocking::connection::Builder;
    use zbus::blocking::MessageIterator;

    fn area(x: i32, y: i32, w: i32, h: i32) -> Area {
        Area { x, y, w, h }
    }

    #[test]
    fn crops_the_area_out_of_the_screen_that_was_picked() {
        // A 1920x1080 screen at the right of another one; the area is inside it.
        let stream = area(1280, 0, 1920, 1080);
        let c = crop(area(1380, 100, 600, 400), stream).unwrap();
        assert_eq!(c, Crop { left: 100, top: 100, right: 1920 - 100 - 600, bottom: 1080 - 100 - 400 });
        // Odd sizes lose a pixel, as the encoder wants even ones.
        let c = crop(area(1280, 0, 301, 201), stream).unwrap();
        assert_eq!((c.right, c.bottom), (1920 - 300, 1080 - 200));
        // The part of an area hanging over the edge is what is recorded.
        let c = crop(area(3000, 900, 400, 400), stream).unwrap();
        assert_eq!((c.left, c.top, c.right, c.bottom), (1720, 900, 0, 0));
        // Another screen, or nothing to record at all.
        assert_eq!(crop(area(0, 0, 500, 500), stream), None);
        assert_eq!(crop(area(1280, 0, 1, 500), stream), None);
    }

    #[test]
    fn knows_when_a_screen_holds_the_area() {
        let stream = area(0, 0, 1920, 1080);
        assert!(stream.contains(area(10, 10, 100, 100)));
        assert!(stream.contains(stream));
        assert!(!stream.contains(area(1900, 10, 100, 100)));
        assert!(!stream.contains(area(2000, 0, 10, 10)));
    }

    fn job(gif: bool, path: &str) -> Job<'static> {
        let (tx, _rx) = mpsc::channel();
        Job {
            req: Request { x: 0, y: 0, width: 640, height: 360, pointer: false, gif },
            width: 640,
            height: 360,
            fps: 30,
            path: path.into(),
            conn: None,
            ended: tx,
            cfg: Box::leak(Box::default()),
        }
    }

    #[test]
    fn builds_the_pipeline() {
        let source = ["pipewiresrc", "fd=7", "path=42"].map(String::from);
        let crop = Crop { left: 100, top: 50, right: 1180, bottom: 670 };
        let text =
            pipeline(&job(false, "/v/my \"clip\".webm"), &source, Some(area(0, 0, 1920, 1080)), Some(crop)).join(" ");
        assert!(
            text.starts_with(
                "-e pipewiresrc fd=7 path=42 ! queue ! videoconvert ! videoscale ! video/x-raw,width=1920,height=1080"
            ),
            "{text}"
        );
        assert!(
            text.contains("videocrop left=100 top=50 right=1180 bottom=670 ! videorate ! video/x-raw,framerate=30/1"),
            "{text}"
        );
        assert!(text.contains("vp8enc deadline=1") && text.contains("! webmmux ! filesink"), "{text}");
        assert!(text.ends_with(r#"location="/v/my \"clip\".webm""#), "the path is quoted: {text}");

        let whole = pipeline(&job(true, "/c/clip.mkv"), &source, None, None).join(" ");
        assert!(!whole.contains("videocrop") && whole.contains("avenc_ffv1 ! matroskamux"), "{whole}");
    }

    /// Plays the portal's ScreenCast, answering the four calls of a recording. What
    /// each one was asked is handed back at the end.
    fn fake_portal(
        bus: &PrivateBus,
        start_code: u32,
        restored: Option<&'static str>,
    ) -> std::thread::JoinHandle<Vec<(String, HashMap<String, OwnedValue>)>> {
        let service = Builder::address(bus.address.as_str()).unwrap().name(DESTINATION).unwrap().build().unwrap();
        let calls = MessageIterator::from(service.clone());
        std::thread::spawn(move || {
            let mut seen = Vec::new();
            for message in calls {
                let message = message.unwrap();
                let member = message.header().member().map(|m| m.to_string()).unwrap_or_default();
                let body = message.body();
                let options: HashMap<String, OwnedValue> = match member.as_str() {
                    "CreateSession" => body.deserialize::<(HashMap<String, OwnedValue>,)>().unwrap().0,
                    "SelectSources" => body.deserialize::<(ObjectPath, HashMap<String, OwnedValue>)>().unwrap().1,
                    "Start" => body.deserialize::<(ObjectPath, String, HashMap<String, OwnedValue>)>().unwrap().2,
                    "OpenPipeWireRemote" => {
                        let none = std::fs::File::open("/dev/null").unwrap();
                        service.reply(&message.header(), &(zbus::zvariant::Fd::from(&none),)).unwrap();
                        seen.push((member, HashMap::new()));
                        return seen;
                    }
                    _ => continue,
                };
                let token = String::try_from(options["handle_token"].try_clone().unwrap()).unwrap();
                let sender = message.header().sender().unwrap().as_str().trim_start_matches(':').replace('.', "_");
                let handle = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
                let mut results: HashMap<&str, Value> = HashMap::new();
                let mut code = 0;
                match member.as_str() {
                    "CreateSession" => {
                        // A string, as the real portals send it.
                        results.insert("session_handle", "/org/freedesktop/portal/desktop/session/1_2/queijo".into());
                    }
                    "Start" => {
                        code = start_code;
                        let props: HashMap<&str, Value> =
                            [("position", Value::from((1280i32, 0i32))), ("size", Value::from((1920i32, 1080i32)))]
                                .into();
                        results.insert("streams", Value::from(vec![(42u32, props)]));
                        results.insert("restore_token", restored.unwrap_or("new-token").into());
                    }
                    _ => {}
                }
                service.emit_signal(None::<&str>, handle.as_str(), REQUEST, "Response", &(code, results)).unwrap();
                service.reply(&message.header(), &(ObjectPath::try_from(handle).unwrap(),)).unwrap();
                seen.push((member, options));
            }
            seen
        })
    }

    const REQUEST: &str = "org.freedesktop.portal.Request";

    #[test]
    fn walks_through_the_portals_steps() {
        let Some(bus) = PrivateBus::start() else { return };
        let portal = fake_portal(&bus, 0, None);
        let granted = negotiate(&bus.connect(), true, "old-token").unwrap_or_else(|_| panic!("negotiation failed"));
        assert_eq!(granted.node, 42);
        assert_eq!(granted.stream, Some(area(1280, 0, 1920, 1080)));
        assert_eq!(granted.token.as_deref(), Some("new-token"));
        assert_eq!(granted.session.as_str(), "/org/freedesktop/portal/desktop/session/1_2/queijo");

        let seen = portal.join().unwrap();
        let names: Vec<_> = seen.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(names, ["CreateSession", "SelectSources", "Start", "OpenPipeWireRemote"]);
        let select = &seen[1].1;
        let number = |key: &str| u32::try_from(select[key].try_clone().unwrap()).unwrap();
        assert_eq!(
            (number("types"), number("cursor_mode"), number("persist_mode")),
            (1, 2, 2),
            "monitors, pointer drawn in, remembered"
        );
        assert_eq!(String::try_from(select["restore_token"].try_clone().unwrap()).unwrap(), "old-token");
    }

    #[test]
    fn asks_for_a_hidden_pointer_and_sends_no_token_the_first_time() {
        let Some(bus) = PrivateBus::start() else { return };
        let portal = fake_portal(&bus, 0, None);
        negotiate(&bus.connect(), false, "").unwrap_or_else(|_| panic!("negotiation failed"));
        let seen = portal.join().unwrap();
        let select = &seen[1].1;
        assert_eq!(u32::try_from(select["cursor_mode"].try_clone().unwrap()).unwrap(), 1);
        assert!(!select.contains_key("restore_token"));
    }

    #[test]
    fn closing_the_dialog_is_not_a_failure_to_record() {
        let Some(bus) = PrivateBus::start() else { return };
        let _portal = fake_portal(&bus, 1, None);
        assert!(matches!(negotiate(&bus.connect(), false, ""), Err(Failure::Cancelled)));
    }

    #[test]
    fn without_a_portal_it_is_unavailable() {
        let Some(bus) = PrivateBus::start() else { return };
        assert!(matches!(negotiate(&bus.connect(), false, ""), Err(Failure::Unavailable(_))));
    }

    /// Runs the real encoder pipeline on GStreamer's test picture instead of a
    /// PipeWire stream. QUEIJO_REQUIRE_GSTREAMER=1 makes a missing GStreamer a failure (CI does).
    /// The encoder reads PipeWire through a descriptor we hand it, so the descriptor
    /// must still be open in the program we start (descriptors close on exec by default).
    #[test]
    fn the_pipewire_connection_survives_into_the_encoder() {
        let sees = |fd: &OwnedFd| {
            Command::new("sh")
                .arg("-c")
                .arg(format!("test -e /proc/self/fd/{}", fd.as_raw_fd()))
                .status()
                .unwrap()
                .success()
        };
        let fd = OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
        assert!(!sees(&fd), "closed on exec to begin with");
        keep_open_on_exec(&fd).unwrap();
        assert!(sees(&fd));
    }

    #[test]
    fn the_pipeline_makes_a_cropped_webm() {
        let have = |program: &str, arg: &str| Command::new(program).arg(arg).output().is_ok_and(|o| o.status.success());
        if !have("gst-launch-1.0", "--version") || !have("ffprobe", "-version") || !have("gst-inspect-1.0", "vp8enc") {
            assert!(std::env::var_os("QUEIJO_REQUIRE_GSTREAMER").is_none(), "GStreamer and ffprobe are required here");
            eprintln!("skipping: GStreamer or ffprobe is missing");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my clip.webm");
        let mut job = job(false, &path.to_string_lossy());
        job.fps = 15;
        let source = ["videotestsrc", "is-live=true", "pattern=smpte"].map(String::from);
        // A 640x480 test picture, shown as 640x480 logical pixels; keep the middle 320x200.
        let crop = crop(area(160, 100, 320, 200), area(0, 0, 640, 480));
        let mut command = Command::new("gst-launch-1.0");
        command.args(pipeline(&job, &source, Some(area(0, 0, 640, 480)), crop));
        let (tx, rx) = mpsc::channel();
        let recorder = process::spawn("gst-launch-1.0", command, Stop::Interrupt, path.clone(), tx).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let file = Box::new(recorder).stop().unwrap();
        assert!(rx.try_recv().is_err());

        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,width,height", "-of", "csv=p=0"])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&probe.stdout).trim().starts_with("vp8,320,200"),
            "{}",
            String::from_utf8_lossy(&probe.stdout)
        );
    }
}
