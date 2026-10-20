mod gnome;

use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;

use chrono::Local;

use crate::config::Config;
use crate::desktop::Env;

/// Area to record, in logical desktop coordinates.
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub pointer: bool,
    /// Produce an animated GIF instead of a video.
    pub gif: bool,
}

impl Request {
    /// `x,y,width,height,pointer,gif` — how the editor hands the area to the recorder process.
    pub fn to_arg(self) -> String {
        format!("{},{},{},{},{},{}", self.x, self.y, self.width, self.height, self.pointer as u8, self.gif as u8)
    }

    pub fn parse(arg: &str) -> Option<Self> {
        let v: Vec<i32> = arg.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
        let (&[x, y, width, height, pointer], gif) = match v[..] {
            [.., gif] if v.len() == 6 => (v[..5].try_into().ok()?, gif != 0),
            _ => (v[..].try_into().ok()?, false),
        };
        (width > 0 && height > 0).then_some(Request { x, y, width, height, pointer: pointer != 0, gif })
    }
}

/// Starts `queijo --record` in the background. Recording in a fresh process
/// that owns no windows means nothing of the editor can linger on screen (or
/// in the video), and the editor simply exits.
pub fn spawn(req: Request) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Command::new(exe)
        .arg("--record")
        .arg(req.to_arg())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// What another `queijo` invocation sends to the running one.
pub const STOP: &[u8] = b"stop";

/// Owned while recording; the Queijo GNOME Shell extension shows a top-bar
/// indicator for as long as this name exists, and calls `Stop` when clicked.
pub(crate) const RECORDER_NAME: &str = "app.queijo.Queijo.Recorder";
pub(crate) const RECORDER_PATH: &str = "/app/queijo/Queijo/Recorder";

struct Control(mpsc::Sender<()>);

#[zbus::interface(name = "app.queijo.Queijo.Recorder")]
impl Control {
    fn stop(&self) {
        let _ = self.0.send(());
    }
}

/// What a backend is asked to record.
pub struct Job<'a> {
    pub req: Request,
    /// The size to record: the area's, made even as the encoders want.
    pub width: i32,
    pub height: i32,
    pub fps: u32,
    /// Where to write: the video, or for a GIF the lossless clip it is made from.
    pub path: PathBuf,
    /// The session bus, if there is one. The GNOME Shell stops recording when
    /// the connection that started it goes away, so this one lives to the end.
    pub conn: Option<&'a zbus::blocking::Connection>,
}

/// A recording in progress.
pub trait Session {
    /// Ends it and returns the file it wrote.
    fn stop(self: Box<Self>) -> Result<PathBuf, String>;
}

/// A way of recording the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    GnomeShell,
}

impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Backend::GnomeShell => "GNOME Shell",
        }
    }

    fn start(self, job: &Job) -> Result<Box<dyn Session>, String> {
        match self {
            Backend::GnomeShell => gnome::start(job),
        }
    }
}

/// The backends for this desktop, in the order they are tried.
pub fn backends(_env: &Env) -> Vec<Backend> {
    vec![Backend::GnomeShell]
}

/// Starts the first backend that can.
fn start_first(chain: &[Backend], job: &Job) -> Result<Box<dyn Session>, String> {
    let mut last = String::from("nothing can record the screen here");
    for (i, backend) in chain.iter().enumerate() {
        match backend.start(job) {
            Ok(session) => return Ok(session),
            Err(e) => {
                match chain.get(i + 1) {
                    Some(next) => eprintln!("queijo: {} can't record ({e}), trying {}", backend.name(), next.name()),
                    None => eprintln!("queijo: {} can't record ({e})", backend.name()),
                }
                last = e;
            }
        }
    }
    Err(last)
}

/// Records the area until another `queijo` (the same hotkey pressed again),
/// or the top-bar indicator says to stop.
pub fn run(req: Request, listener: &UnixListener, cfg: &Config) -> Result<PathBuf, String> {
    let target = if req.gif { &cfg.gifs } else { &cfg.videos };
    std::fs::create_dir_all(target.dir()).map_err(|e| e.to_string())?;
    let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = if req.gif {
        // Big while it lasts, so on disk rather than in the RAM-backed runtime dir.
        let cache = cache_dir();
        std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        cache.join(format!("recording-{stamp}.mkv"))
    } else {
        target.unique_path("webm", req.width as u32, req.height as u32)
    };

    // Stop requests come from the top-bar indicator (D-Bus) or from launching
    // queijo again (the instance socket).
    let (stop_tx, stop_rx) = mpsc::channel();
    let socket = listener.try_clone().map_err(|e| e.to_string())?;
    let tx = stop_tx.clone();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = socket.accept() {
            let mut msg = Vec::new();
            let _ = stream.read_to_end(&mut msg);
            if msg == STOP {
                let _ = tx.send(());
            }
        }
    });

    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at(RECORDER_PATH, Control(stop_tx.clone())))
        .and_then(|b| b.build())
        .map_err(|e| eprintln!("queijo: no session bus ({e})"))
        .ok();
    let fps = if req.gif { cfg.gif_fps.clamp(1, 50) } else { cfg.video_fps.clamp(1, 60) };
    let job = Job {
        req,
        // The encoder wants even dimensions.
        width: req.width & !1,
        height: req.height & !1,
        fps,
        path,
        conn: conn.as_ref(),
    };
    let session = start_first(&backends(Env::current()), &job)?;

    // Only now, so the indicator appears exactly when recording runs.
    if let Some(conn) = &conn {
        if let Err(e) = conn.request_name(RECORDER_NAME) {
            eprintln!("queijo: no top-bar indicator ({e})");
        }
    }
    let _ = stop_rx.recv();
    if let Some(conn) = &conn {
        let _ = conn.release_name(RECORDER_NAME);
    }

    let used = session.stop()?;
    drop(conn);
    if !req.gif {
        return Ok(used);
    }
    crate::output::notify(crate::i18n::t("notify.making_gif"), crate::i18n::t("notify.making_gif_body"), None);
    let gif = target.unique_path("gif", req.width as u32, req.height as u32);
    let result = crate::gif::convert(&used, &gif, cfg.gif_max_width);
    let _ = std::fs::remove_file(&used);
    result.map(|()| gif)
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache"))
        .join("queijo")
}
