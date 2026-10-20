//! Recording a wlroots compositor (Sway, Hyprland, ...) with `wf-recorder`.

use std::process::Command;
use std::sync::OnceLock;

use super::process::{self, missing_tool, Stop};
use super::{Job, Session};

pub fn start(job: &Job) -> Result<Box<dyn Session>, String> {
    let help = help()?;
    let framerate = help.contains("--framerate");
    if !job.req.pointer {
        // It has no option for this: the compositor's pointer is always in the picture.
        eprintln!("queijo: wf-recorder always draws the pointer");
    }
    let mut last = String::new();
    // VP9 if the compositor's libav has it, then VP8.
    for (i, codec) in ["libvpx-vp9", "libvpx"].into_iter().enumerate() {
        let mut command = Command::new("wf-recorder");
        command.args(args(job, codec, framerate));
        match process::spawn("wf-recorder", command, Stop::Interrupt, job.path.clone(), job.ended.clone()) {
            Ok(recorder) => return Ok(Box::new(recorder)),
            // Only a codec it doesn't know is worth another try with the next one.
            Err(e) if i == 0 && e.to_lowercase().contains("codec") => last = e,
            Err(e) => return Err(missing_tool(e, "wf-recorder", "wf-recorder")),
        }
    }
    Err(last)
}

/// `wf-recorder --help`, which says which options this version has. Fails if it isn't installed.
fn help() -> Result<&'static str, String> {
    static HELP: OnceLock<Result<String, String>> = OnceLock::new();
    HELP.get_or_init(|| {
        let out = Command::new("wf-recorder")
            .arg("--help")
            .output()
            .map_err(|e| missing_tool(format!("wf-recorder: {e}"), "wf-recorder", "wf-recorder"))?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
    })
    .as_ref()
    .map(String::as_str)
    .map_err(Clone::clone)
}

/// The arguments for wf-recorder. A GIF's clip is lossless; a video is tuned to
/// keep up in real time. `framerate` is whether this version takes `-r`.
fn args(job: &Job, codec: &str, framerate: bool) -> Vec<String> {
    let mut args: Vec<String> =
        vec!["-g".into(), format!("{},{} {}x{}", job.req.x, job.req.y, job.width, job.height), "-f".into()];
    args.push(job.path.to_string_lossy().into_owned());
    if framerate {
        args.extend(["-r".into(), job.fps.to_string()]);
    }
    if job.req.gif {
        args.extend(["-c", "ffv1", "-m", "matroska"].map(String::from));
    } else {
        args.extend(["-c".into(), codec.into()]);
        let params: &[&str] = if codec == "libvpx-vp9" {
            &["deadline=realtime", "cpu-used=8", "row-mt=1", "b=0", "crf=30"]
        } else {
            &["deadline=realtime", "cpu-used=8", "b=6M", "crf=10"]
        };
        for param in params {
            args.extend(["-p".into(), (*param).into()]);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::Request;
    use std::sync::mpsc;

    fn job(gif: bool) -> Job<'static> {
        let (tx, _rx) = mpsc::channel();
        Job {
            req: Request { x: -1280, y: 40, width: 641, height: 360, pointer: false, gif },
            width: 640,
            height: 360,
            fps: 25,
            path: "/v/out.webm".into(),
            conn: None,
            ended: tx,
            cfg: Box::leak(Box::default()),
        }
    }

    #[test]
    fn asks_wf_recorder_for_the_area() {
        let text = args(&job(false), "libvpx-vp9", false).join(" ");
        assert!(text.starts_with("-g -1280,40 640x360 -f /v/out.webm"), "{text}");
        assert!(text.contains("-c libvpx-vp9 -p deadline=realtime"), "{text}");
        assert!(!text.contains("-r "), "an old wf-recorder has no -r: {text}");
        assert!(args(&job(false), "libvpx", true).join(" ").contains("-r 25"));
        assert!(args(&job(false), "libvpx", false)
            .join(" ")
            .contains("-c libvpx -p deadline=realtime -p cpu-used=8 -p b=6M"));
    }

    #[test]
    fn a_gif_clip_is_lossless() {
        let text = args(&job(true), "libvpx-vp9", false).join(" ");
        assert!(text.contains("-c ffv1 -m matroska") && !text.contains("vpx"), "{text}");
    }
}
