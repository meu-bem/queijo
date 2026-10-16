use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "queijo")]
#[command(author, version, about = "Screenshot, screencast and GIF tool for Linux", long_about = None)]
pub struct Cli {
    /// Directory where screenshots are saved (overrides the settings)
    #[arg(short, long)]
    pub output_dir: Option<PathBuf>,

    /// Capture and save immediately, without opening the editor
    #[arg(short, long)]
    pub quick: bool,

    /// Seconds to wait before capturing
    #[arg(short, long, default_value_t = 0)]
    pub delay: u64,

    /// Only use this monitor (index or connector name, e.g. eDP-1); see --list-monitors
    #[arg(short, long)]
    pub monitor: Option<String>,

    /// Start in this mode: photo, video or gif
    #[arg(short, long, value_parser = ["photo", "video", "gif"])]
    pub kind: Option<String>,

    /// Start with this area mode: selection, screen or window
    #[arg(long, value_parser = ["selection", "screen", "window"])]
    pub area: Option<String>,

    /// Open the settings window
    #[arg(long)]
    pub settings: bool,

    /// Stop a recording in progress (same as launching queijo again)
    #[arg(long)]
    pub stop: bool,

    /// Internal: record this area (x,y,width,height,pointer) until stopped
    #[arg(long, hide = true)]
    pub record: Option<String>,

    /// Print the detected monitors and exit
    #[arg(long)]
    pub list_monitors: bool,
}
