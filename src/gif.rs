//! Lossless recording → animated GIF, tuned for screen content.

use std::path::Path;
use std::process::Command;

/// One ffmpeg pass builds a single palette from the whole clip and maps every
/// frame onto it:
/// - `stats_mode=full`: colours of static areas count as much as moving ones,
///   so the parts on screen the whole time don't band.
/// - Light ordered (Bayer) dithering: exact UI/text colours stay untouched,
///   gradients don't band, and unlike error diffusion it doesn't add frame to
///   frame noise, which compresses badly.
/// - `diff_mode=rectangle` + the GIF muxer's transparency diffing: each frame
///   only stores the region that changed.
const FILTER: &str = "[0:v]split[a][b];[a]palettegen=stats_mode=full[p];\
                      [b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle";

/// The ffmpeg filter graph; a `max_width` above 0 scales wider recordings down first.
fn filter(max_width: u32) -> String {
    if max_width > 0 {
        FILTER.replacen("[0:v]split", &format!("[0:v]scale='min(iw,{max_width})':-2:flags=lanczos,split"), 1)
    } else {
        FILTER.to_string()
    }
}

/// `max_width` > 0 scales wider recordings down (Lanczos), keeping the aspect.
pub fn convert(input: &Path, output: &Path, max_width: u32) -> Result<(), String> {
    let filter = filter(max_width);
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        // Keep the recording's own frame timing: PipeWire only sends frames when
        // something changed, so still moments cost nothing.
        .args(["-vsync", "vfr", "-filter_complex", &filter, "-loop", "0"])
        .arg(output)
        .output()
        .map_err(|e| crate::i18n::tf("error.no_ffmpeg", &[("error", &e.to_string())]))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    // Lossless re-optimization of the frames; optional.
    let _ = Command::new("gifsicle").args(["--batch", "-O3"]).arg(output).status();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn have(program: &str) -> bool {
        Command::new(program).arg("-version").output().is_ok()
    }

    /// `(width, height)` from a GIF's header.
    fn size(gif: &[u8]) -> (u16, u16) {
        assert_eq!(&gif[..6], b"GIF89a", "not a GIF");
        (u16::from_le_bytes([gif[6], gif[7]]), u16::from_le_bytes([gif[8], gif[9]]))
    }

    #[test]
    fn the_filter_scales_only_when_asked() {
        assert!(!filter(0).contains("lanczos"), "no resizing without a maximum width");
        let scaled = filter(960);
        assert!(scaled.starts_with("[0:v]scale='min(iw,960)':-2:flags=lanczos,split"), "{scaled}");
        for graph in [filter(0), scaled] {
            assert!(graph.contains("palettegen=stats_mode=full"));
            assert!(graph.contains("paletteuse=dither=bayer"));
            assert!(graph.contains("diff_mode=rectangle"));
        }
    }

    #[test]
    fn a_missing_recording_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let result = convert(&dir.path().join("nope.mkv"), &dir.path().join("out.gif"), 0);
        assert!(result.is_err());
    }

    /// Needs ffmpeg; set QUEIJO_REQUIRE_FFMPEG=1 to make its absence a failure (CI does).
    #[test]
    fn converts_a_lossless_recording_to_a_gif() {
        if !have("ffmpeg") {
            assert!(std::env::var_os("QUEIJO_REQUIRE_FFMPEG").is_none(), "ffmpeg is required here");
            eprintln!("skipping: ffmpeg is not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.mkv");
        let made = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=10", "-t", "1"])
            .args(["-c:v", "ffv1"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());

        let full = dir.path().join("full.gif");
        convert(&input, &full, 0).unwrap();
        assert_eq!(size(&std::fs::read(&full).unwrap()), (320, 180));

        let small = dir.path().join("small.gif");
        convert(&input, &small, 160).unwrap();
        assert_eq!(size(&std::fs::read(&small).unwrap()), (160, 90), "scaled down, same aspect");

        let wide = dir.path().join("wide.gif");
        convert(&input, &wide, 1000).unwrap();
        assert_eq!(size(&std::fs::read(&wide).unwrap()), (320, 180), "never scaled up");
    }

    #[test]
    fn gifsicle_is_optional() {
        // convert() must not depend on it; this just documents the probe used above.
        let _ = have("gifsicle");
    }
}
