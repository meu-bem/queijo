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

    /// Print the windows window mode would offer and exit
    #[arg(long)]
    pub list_windows: bool,

    /// Turn on the Queijo GNOME Shell extension that fits your GNOME, then exit
    #[arg(long)]
    pub enable_extension: bool,

    /// Print what Queijo found on this system and what it is missing, then exit
    #[arg(long)]
    pub doctor: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("queijo").chain(args.iter().copied()))
    }

    #[test]
    fn the_command_line_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_means_the_editor_with_the_settings() {
        let cli = parse(&[]).unwrap();
        assert!(
            !cli.quick
                && !cli.stop
                && !cli.settings
                && !cli.list_monitors
                && !cli.list_windows
                && !cli.doctor
                && !cli.enable_extension
        );
        assert!(cli.kind.is_none() && cli.area.is_none() && cli.monitor.is_none() && cli.record.is_none());
        assert_eq!(cli.delay, 0);
    }

    #[test]
    fn understands_every_option() {
        let cli =
            parse(&["--kind", "gif", "--area", "window", "-m", "eDP-1", "-d", "3", "-q", "-o", "/tmp/x"]).unwrap();
        assert_eq!(cli.kind.as_deref(), Some("gif"));
        assert_eq!(cli.area.as_deref(), Some("window"));
        assert_eq!(cli.monitor.as_deref(), Some("eDP-1"));
        assert_eq!(cli.delay, 3);
        assert!(cli.quick);
        assert_eq!(cli.output_dir.as_deref(), Some(std::path::Path::new("/tmp/x")));
        assert!(parse(&["--settings"]).unwrap().settings);
        assert!(parse(&["--stop"]).unwrap().stop);
        assert!(parse(&["--list-monitors"]).unwrap().list_monitors);
        assert!(parse(&["--list-windows"]).unwrap().list_windows);
        assert!(parse(&["--doctor"]).unwrap().doctor);
        assert!(parse(&["--enable-extension"]).unwrap().enable_extension);
        assert_eq!(parse(&["--record", "1,2,3,4,0,1"]).unwrap().record.as_deref(), Some("1,2,3,4,0,1"));
    }

    #[test]
    fn every_kind_and_area_is_accepted_and_nothing_else() {
        for kind in ["photo", "video", "gif"] {
            assert!(parse(&["--kind", kind]).is_ok(), "{kind}");
        }
        for area in ["selection", "screen", "window"] {
            assert!(parse(&["--area", area]).is_ok(), "{area}");
        }
        assert!(parse(&["--kind", "movie"]).is_err());
        assert!(parse(&["--area", "desktop"]).is_err());
        assert!(parse(&["--delay", "soon"]).is_err());
        assert!(parse(&["--nonsense"]).is_err());
    }

    #[test]
    fn the_internal_option_stays_out_of_the_help() {
        let help = Cli::command().render_long_help().to_string();
        assert!(!help.contains("--record"), "{help}");
        for shown in [
            "--kind",
            "--area",
            "--monitor",
            "--settings",
            "--stop",
            "--list-monitors",
            "--list-windows",
            "--enable-extension",
            "--doctor",
            "--quick",
        ] {
            assert!(help.contains(shown), "{shown} is documented");
        }
    }
}
