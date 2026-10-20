//! Recording with GNOME Shell's own screencast service.
//! GNOME shows its orange "screen is being recorded" indicator meanwhile.

use std::collections::HashMap;
use std::path::PathBuf;

use zbus::blocking::Connection;
use zbus::zvariant::Value;

use super::{Job, Session};

/// GIFs are recorded losslessly (FFV1, full RGB) and only then quantized, so
/// text stays crisp; GNOME's default VP8 would blur it before we even start.
const LOSSLESS_PIPELINE: &str = "videoconvert chroma-mode=none dither=none n-threads=%T ! video/x-raw,format=BGRx \
    ! queue ! avenc_ffv1 ! queue ! matroskamux";

const DESTINATION: &str = "org.gnome.Shell.Screencast";
const PATH: &str = "/org/gnome/Shell/Screencast";

struct Shell {
    conn: Connection,
    file: PathBuf,
}

pub fn start(job: &Job) -> Result<Box<dyn Session>, String> {
    let conn = job.conn.ok_or("no session bus")?.clone();
    let file = job.path.to_str().ok_or("non UTF-8 path")?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("draw-cursor", job.req.pointer.into());
    options.insert("framerate", (job.fps as i32).into());
    if job.req.gif {
        options.insert("pipeline", LOSSLESS_PIPELINE.into());
    }
    let reply = conn
        .call_method(
            Some(DESTINATION),
            PATH,
            Some(DESTINATION),
            "ScreencastArea",
            &(job.req.x, job.req.y, job.width, job.height, file, options),
        )
        .map_err(|e| e.to_string())?;
    let (ok, used): (bool, String) = reply.body().deserialize().map_err(|e| e.to_string())?;
    if !ok {
        return Err(crate::i18n::t("error.shell_refused").into());
    }
    Ok(Box::new(Shell { conn, file: PathBuf::from(used) }))
}

impl Session for Shell {
    fn stop(self: Box<Self>) -> Result<PathBuf, String> {
        self.conn
            .call_method(Some(DESTINATION), PATH, Some(DESTINATION), "StopScreencast", &())
            .map_err(|e| e.to_string())?;
        Ok(self.file)
    }
}
