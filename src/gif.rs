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

/// `max_width` > 0 scales wider recordings down (Lanczos), keeping the aspect.
pub fn convert(input: &Path, output: &Path, max_width: u32) -> Result<(), String> {
    let filter = if max_width > 0 {
        FILTER.replacen("[0:v]split", &format!("[0:v]scale='min(iw,{max_width})':-2:flags=lanczos,split"), 1)
    } else {
        FILTER.to_string()
    };
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        // Keep the recording's own frame timing: PipeWire only sends frames when
        // something changed, so still moments cost nothing.
        .args(["-vsync", "vfr", "-filter_complex", &filter, "-loop", "0"])
        .arg(output)
        .output()
        .map_err(|e| format!("ffmpeg: {e} (install ffmpeg)"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    // Lossless re-optimization of the frames; optional.
    let _ = Command::new("gifsicle").args(["--batch", "-O3"]).arg(output).status();
    Ok(())
}
