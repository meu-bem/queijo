//! Recording an X11 screen with ffmpeg's `x11grab`.

use std::process::Command;
use std::sync::OnceLock;

use super::process::{self, Stop};
use super::{Job, Session};

pub fn start(job: &Job) -> Result<Box<dyn Session>, String> {
    let display = std::env::var("DISPLAY").map_err(|_| "no DISPLAY")?;
    start_on(job, &display)
}

pub(super) fn start_on(job: &Job, display: &str) -> Result<Box<dyn Session>, String> {
    let encoders = ffmpeg_output(&["-encoders"])?;
    if !ffmpeg_output(&["-devices"])?.contains("x11grab") {
        return Err("this ffmpeg was built without x11grab".into());
    }
    let mut command = Command::new("ffmpeg");
    command.args(args(job, display, &encoders)?);
    let recorder = process::spawn("ffmpeg", command, Stop::Quit, job.path.clone(), job.ended.clone())
        .map_err(|e| process::missing_tool(e, "ffmpeg", "ffmpeg"))?;
    Ok(Box::new(recorder))
}

/// What `ffmpeg -hide_banner <args>` lists, asked once per kind of question.
fn ffmpeg_output(args: &[&str]) -> Result<String, String> {
    static CACHE: OnceLock<std::sync::Mutex<Vec<(String, String)>>> = OnceLock::new();
    let key = args.join(" ");
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, text)) = cache.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return Ok(text.clone());
    }
    let out = Command::new("ffmpeg")
        .arg("-hide_banner")
        .args(args)
        .output()
        .map_err(|e| process::missing_tool(format!("ffmpeg: {e}"), "ffmpeg", "ffmpeg"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    cache.lock().unwrap().push((key, text.clone()));
    Ok(text)
}

/// The arguments for ffmpeg. A video is VP9 (or VP8 if that is all ffmpeg has),
/// tuned to keep up in real time; a GIF's clip is lossless, as with the GNOME Shell.
fn args(job: &Job, display: &str, encoders: &str) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = ["-loglevel", "warning", "-y", "-f", "x11grab"].map(String::from).into();
    args.extend([
        "-framerate".into(),
        job.fps.to_string(),
        "-draw_mouse".into(),
        (job.req.pointer as u8).to_string(),
        "-video_size".into(),
        format!("{}x{}", job.width, job.height),
        "-i".into(),
        format!("{display}+{},{}", job.req.x, job.req.y),
    ]);
    if job.req.gif {
        args.extend(["-c:v", "ffv1"].map(String::from));
    } else if encoders.contains("libvpx-vp9") {
        args.extend(
            [
                "-c:v",
                "libvpx-vp9",
                "-deadline",
                "realtime",
                "-cpu-used",
                "8",
                "-row-mt",
                "1",
                "-b:v",
                "0",
                "-crf",
                "30",
            ]
            .map(String::from),
        );
        args.extend(["-pix_fmt", "yuv420p"].map(String::from));
    } else if encoders.contains("libvpx") {
        args.extend(
            ["-c:v", "libvpx", "-deadline", "realtime", "-cpu-used", "8", "-b:v", "6M", "-crf", "10"].map(String::from),
        );
        args.extend(["-pix_fmt", "yuv420p"].map(String::from));
    } else {
        return Err("this ffmpeg has no VP8 or VP9 encoder".into());
    }
    args.push(job.path.to_string_lossy().into_owned());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Request;
    use crate::testutil::VirtualDisplay;
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn job(gif: bool, pointer: bool, path: PathBuf, tx: mpsc::Sender<()>) -> Job<'static> {
        Job {
            req: Request { x: 10, y: 20, width: 301, height: 200, pointer, gif },
            width: 300,
            height: 200,
            fps: 25,
            path,
            conn: None,
            ended: tx,
            cfg: Box::leak(Box::default()),
        }
    }

    #[test]
    fn asks_ffmpeg_for_the_area_in_real_time() {
        let (tx, _rx) = mpsc::channel();
        let video = args(&job(false, true, "/v/out.webm".into(), tx.clone()), ":0", " libvpx-vp9 libvpx").unwrap();
        let text = video.join(" ");
        assert!(text.contains("-f x11grab -framerate 25 -draw_mouse 1 -video_size 300x200 -i :0+10,20"), "{text}");
        assert!(text.contains("-c:v libvpx-vp9 -deadline realtime"), "{text}");
        assert!(text.ends_with("/v/out.webm"), "{text}");

        let no_pointer = args(&job(false, false, "/v/out.webm".into(), tx.clone()), ":1.0", " libvpx").unwrap();
        let text = no_pointer.join(" ");
        assert!(text.contains("-draw_mouse 0") && text.contains("-i :1.0+10,20"), "{text}");
        assert!(text.contains("-c:v libvpx -deadline"), "falls back to VP8: {text}");

        let gif = args(&job(true, false, "/c/clip.mkv".into(), tx), ":0", " libvpx-vp9").unwrap().join(" ");
        assert!(gif.contains("-c:v ffv1") && !gif.contains("vpx"), "{gif}");
    }

    #[test]
    fn without_a_vpx_encoder_it_says_so() {
        let (tx, _rx) = mpsc::channel();
        assert!(args(&job(false, false, "/o.webm".into(), tx), ":0", " h264").unwrap_err().contains("VP8"));
    }

    /// Needs ffmpeg with x11grab and libvpx, and Xvfb. QUEIJO_REQUIRE_FFMPEG / QUEIJO_REQUIRE_XVFB make a miss a failure.
    #[test]
    fn records_an_area_of_a_virtual_screen() {
        let have = |program: &str| Command::new(program).arg("-version").output().is_ok();
        if !have("ffmpeg") || !have("ffprobe") {
            assert!(std::env::var_os("QUEIJO_REQUIRE_FFMPEG").is_none(), "ffmpeg is required here");
            eprintln!("skipping: ffmpeg is not installed");
            return;
        }
        let Some(display) = VirtualDisplay::start() else { return };
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let mut job = job(false, false, dir.path().join("out.webm"), tx);
        job.req = Request { x: 0, y: 0, width: 320, height: 240, pointer: false, gif: false };
        job.width = 320;
        job.height = 240;

        let session = start_on(&job, &display.name).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let path = session.stop().unwrap();
        assert!(rx.try_recv().is_err());

        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,width,height", "-of", "csv=p=0"])
            .arg(&path)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&probe.stdout);
        assert!(text.trim().starts_with("vp9,320,240") || text.trim().starts_with("vp8,320,240"), "{text}");
    }

    #[test]
    fn an_area_off_the_screen_fails_at_once() {
        let have = |program: &str| Command::new(program).arg("-version").output().is_ok();
        if !have("ffmpeg") {
            return;
        }
        let Some(display) = VirtualDisplay::start() else { return };
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut job = job(false, false, dir.path().join("out.webm"), tx);
        job.req.x = 5000;
        let err = start_on(&job, &display.name).err().expect("ffmpeg refuses it");
        assert!(err.contains("ffmpeg"), "{err}");
    }
}
