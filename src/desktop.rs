//! Which desktop Queijo runs on. Most of what it can do depends on the answer:
//! screenshots, recording and shortcuts are each done a different way on GNOME,
//! KDE, wlroots compositors and plain X11.

use std::process::Command;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Wayland,
    X11,
    /// Neither `WAYLAND_DISPLAY` nor `DISPLAY` is set (a terminal, SSH, CI).
    Headless,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Desktop {
    Gnome,
    Kde,
    Sway,
    Hyprland,
    Xfce,
    Cinnamon,
    Mate,
    /// Anything else, as the desktop calls itself (empty when it doesn't say).
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Env {
    pub session: Session,
    pub desktop: Desktop,
    /// Whether a window can be see-through. Not on an X11 screen with no compositor,
    /// nor on Sway, which paints a black backdrop behind every fullscreen window.
    pub compositing: bool,
}

impl Env {
    /// The environment this process runs in, worked out once.
    pub fn current() -> &'static Env {
        static CURRENT: OnceLock<Env> = OnceLock::new();
        CURRENT.get_or_init(|| {
            let mut env = detect_from(&|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
            if env.session == Session::X11 {
                // If the server can't be asked, assume the usual: there is one.
                env.compositing = crate::x11::has_compositor(None).unwrap_or(true);
            }
            env
        })
    }
}

impl Desktop {
    pub fn name(&self) -> &str {
        match self {
            Desktop::Gnome => "GNOME",
            Desktop::Kde => "KDE Plasma",
            Desktop::Sway => "Sway",
            Desktop::Hyprland => "Hyprland",
            Desktop::Xfce => "Xfce",
            Desktop::Cinnamon => "Cinnamon",
            Desktop::Mate => "MATE",
            Desktop::Other(name) if name.is_empty() => "unknown",
            Desktop::Other(name) => name,
        }
    }

    /// The major version of the desktop's shell, where it can be told. This
    /// starts a program, so it is only for diagnostics, never for start-up.
    pub fn major_version(&self) -> Option<u32> {
        let (program, flag) = match self {
            Desktop::Gnome => ("gnome-shell", "--version"),
            Desktop::Kde => ("plasmashell", "--version"),
            _ => return None,
        };
        let out = Command::new(program).arg(flag).output().ok()?;
        parse_major(&String::from_utf8_lossy(&out.stdout))
    }
}

/// The major version in `GNOME Shell 42.9` or `plasmashell 5.27.4`.
fn parse_major(output: &str) -> Option<u32> {
    let version = output.split_whitespace().last()?;
    version.split('.').next()?.parse().ok()
}

fn desktop_named(name: &str) -> Option<Desktop> {
    match name.to_ascii_lowercase().as_str() {
        "gnome" | "gnome-xorg" | "gnome-classic" => Some(Desktop::Gnome),
        "kde" | "plasma" | "plasmawayland" | "plasmax11" => Some(Desktop::Kde),
        "sway" => Some(Desktop::Sway),
        "hyprland" => Some(Desktop::Hyprland),
        "xfce" => Some(Desktop::Xfce),
        "x-cinnamon" | "cinnamon" => Some(Desktop::Cinnamon),
        "mate" => Some(Desktop::Mate),
        _ => None,
    }
}

/// `var` reads an environment variable; it is a parameter so the rules can be
/// tested with made-up environments.
pub fn detect_from(var: &dyn Fn(&str) -> Option<String>) -> Env {
    let session = if var("WAYLAND_DISPLAY").is_some() {
        Session::Wayland
    } else if var("DISPLAY").is_some() {
        Session::X11
    } else {
        Session::Headless
    };

    // XDG_CURRENT_DESKTOP lists the desktop and what it is based on ("ubuntu:GNOME");
    // the first name we know wins.
    let names: Vec<String> = ["XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION"]
        .iter()
        .filter_map(|name| var(name))
        .flat_map(|list| list.split(':').map(str::to_owned).collect::<Vec<_>>())
        .collect();
    let desktop = names
        .iter()
        .find_map(|name| desktop_named(name))
        .or_else(|| var("SWAYSOCK").map(|_| Desktop::Sway))
        .or_else(|| var("HYPRLAND_INSTANCE_SIGNATURE").map(|_| Desktop::Hyprland))
        .unwrap_or_else(|| Desktop::Other(names.into_iter().find(|n| !n.is_empty()).unwrap_or_default()));

    let compositing = !(session == Session::Wayland && desktop == Desktop::Sway);
    Env { session, desktop, compositing }
}

/// Whether to draw with the CPU instead of the GPU. Mutter shows see-through
/// fullscreen windows black when they come from the GPU, so GNOME on Wayland
/// needs it; other desktops are left on the GPU. `force` is `QUEIJO_SOFTWARE_GL`:
/// `1` or `0` overrides that, for working around a desktop we have not tried.
pub fn wants_software_gl(env: &Env, force: Option<&str>) -> bool {
    match force {
        Some("1") => true,
        Some("0") => false,
        _ => env.session == Session::Wayland && env.desktop == Desktop::Gnome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &[(&str, &str)]) -> Env {
        detect_from(&|name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string()))
    }

    #[test]
    fn recognizes_the_common_desktops() {
        let cases = [
            ("ubuntu:GNOME", Desktop::Gnome),
            ("pop:GNOME", Desktop::Gnome),
            ("GNOME", Desktop::Gnome),
            ("KDE", Desktop::Kde),
            ("sway", Desktop::Sway),
            ("Hyprland", Desktop::Hyprland),
            ("X-Cinnamon", Desktop::Cinnamon),
            ("XFCE", Desktop::Xfce),
            ("MATE", Desktop::Mate),
        ];
        for (current, expected) in cases {
            assert_eq!(env(&[("XDG_CURRENT_DESKTOP", current)]).desktop, expected, "{current}");
        }
    }

    #[test]
    fn a_lone_ubuntu_is_not_gnome() {
        assert_eq!(env(&[("XDG_CURRENT_DESKTOP", "ubuntu")]).desktop, Desktop::Other("ubuntu".into()));
    }

    #[test]
    fn falls_back_to_the_other_variables() {
        assert_eq!(env(&[("XDG_SESSION_DESKTOP", "plasma")]).desktop, Desktop::Kde);
        assert_eq!(env(&[("DESKTOP_SESSION", "xfce")]).desktop, Desktop::Xfce);
        assert_eq!(env(&[("SWAYSOCK", "/run/user/1000/sway-ipc.sock")]).desktop, Desktop::Sway);
        assert_eq!(env(&[("HYPRLAND_INSTANCE_SIGNATURE", "abc")]).desktop, Desktop::Hyprland);
        // What the desktop says about itself comes before the compositor's sockets.
        let both = env(&[("XDG_CURRENT_DESKTOP", "KDE"), ("SWAYSOCK", "/x")]);
        assert_eq!(both.desktop, Desktop::Kde);
    }

    #[test]
    fn keeps_the_name_of_a_desktop_it_does_not_know() {
        assert_eq!(env(&[("XDG_CURRENT_DESKTOP", "LXQt")]).desktop, Desktop::Other("LXQt".into()));
        assert_eq!(env(&[]).desktop, Desktop::Other(String::new()));
        assert_eq!(env(&[]).desktop.name(), "unknown");
    }

    #[test]
    fn sway_cannot_show_the_screen_through_a_window() {
        let sway = env(&[("WAYLAND_DISPLAY", "wayland-1"), ("XDG_CURRENT_DESKTOP", "sway")]);
        assert!(!sway.compositing);
        assert!(env(&[("WAYLAND_DISPLAY", "wayland-1"), ("XDG_CURRENT_DESKTOP", "KDE")]).compositing);
        assert!(env(&[("WAYLAND_DISPLAY", "wayland-1"), ("XDG_CURRENT_DESKTOP", "Hyprland")]).compositing);
        // On X11 it is the server that is asked, not the desktop's name.
        assert!(env(&[("DISPLAY", ":0"), ("XDG_CURRENT_DESKTOP", "sway")]).compositing);
    }

    #[test]
    fn tells_the_session_from_the_display_variables() {
        assert_eq!(env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")]).session, Session::Wayland);
        assert_eq!(env(&[("DISPLAY", ":0")]).session, Session::X11);
        assert_eq!(env(&[]).session, Session::Headless);
    }

    #[test]
    fn software_drawing_is_for_gnome_on_wayland_unless_forced() {
        let gnome = env(&[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_CURRENT_DESKTOP", "GNOME")]);
        let gnome_x11 = env(&[("DISPLAY", ":0"), ("XDG_CURRENT_DESKTOP", "GNOME")]);
        let kde = env(&[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_CURRENT_DESKTOP", "KDE")]);
        assert!(wants_software_gl(&gnome, None));
        assert!(!wants_software_gl(&gnome_x11, None));
        assert!(!wants_software_gl(&kde, None));
        assert!(wants_software_gl(&kde, Some("1")));
        assert!(!wants_software_gl(&gnome, Some("0")));
        assert!(wants_software_gl(&gnome, Some("anything else")));
        assert!(!wants_software_gl(&kde, Some("")));
    }

    #[test]
    fn reads_the_major_version() {
        assert_eq!(parse_major("GNOME Shell 42.9\n"), Some(42));
        assert_eq!(parse_major("plasmashell 5.27.4\n"), Some(5));
        assert_eq!(parse_major("GNOME Shell 47.alpha"), Some(47));
        assert_eq!(parse_major(""), None);
        assert_eq!(parse_major("no version here"), None);
    }
}
