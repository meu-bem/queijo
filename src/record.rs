use std::collections::HashMap;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;

use chrono::Local;

use crate::config::Config;
use zbus::zvariant::Value;

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

/// GIFs are recorded losslessly (FFV1, full RGB) and only then quantized, so
/// text stays crisp; GNOME's default VP8 would blur it before we even start.
const LOSSLESS_PIPELINE: &str = "videoconvert chroma-mode=none dither=none n-threads=%T ! video/x-raw,format=BGRx \
    ! queue ! avenc_ffv1 ! queue ! matroskamux";


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
const RECORDER_NAME: &str = "io.github.meu_bem.Queijo.Recorder";
const RECORDER_PATH: &str = "/io/github/meu_bem/Queijo/Recorder";

struct Control(mpsc::Sender<()>);

#[zbus::interface(name = "io.github.meu_bem.Queijo.Recorder")]
impl Control {
    fn stop(&self) {
        let _ = self.0.send(());
    }
}

/// Records the area with GNOME Shell's screencast service until another
/// `queijo` (the same hotkey pressed again) tells us to stop.
/// GNOME shows its orange "screen is being recorded" indicator meanwhile.
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
    let file = path.to_str().ok_or("non UTF-8 path")?;

    // The encoder wants even dimensions.
    let (width, height) = (req.width & !1, req.height & !1);
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("draw-cursor", req.pointer.into());
    if req.gif {
        options.insert("framerate", (cfg.gif_fps.clamp(1, 50) as i32).into());
        options.insert("pipeline", LOSSLESS_PIPELINE.into());
    } else {
        options.insert("framerate", (cfg.video_fps.clamp(1, 60) as i32).into());
    }

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

    // The Shell stops recording if this connection goes away, so it lives until the end.
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.serve_at(RECORDER_PATH, Control(stop_tx)))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    let reply = conn
        .call_method(
            Some("org.gnome.Shell.Screencast"),
            "/org/gnome/Shell/Screencast",
            Some("org.gnome.Shell.Screencast"),
            "ScreencastArea",
            &(req.x, req.y, width, height, file, options),
        )
        .map_err(|e| e.to_string())?;
    let (ok, used): (bool, String) = reply.body().deserialize().map_err(|e| e.to_string())?;
    if !ok {
        return Err("GNOME Shell refused to start recording".into());
    }

    // Only now, so the indicator appears exactly when recording runs.
    if let Err(e) = conn.request_name(RECORDER_NAME) {
        eprintln!("queijo: no top-bar indicator ({e})");
    }
    let _ = stop_rx.recv();
    let _ = conn.release_name(RECORDER_NAME);

    conn.call_method(
        Some("org.gnome.Shell.Screencast"),
        "/org/gnome/Shell/Screencast",
        Some("org.gnome.Shell.Screencast"),
        "StopScreencast",
        &(),
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    let used = PathBuf::from(used);
    if !req.gif {
        return Ok(used);
    }
    crate::output::notify("Making GIF…", "Optimizing colors and frames", None);
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
