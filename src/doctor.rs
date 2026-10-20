//! `queijo --doctor`: what Queijo found on this system, what it will use for each
//! job, and what is missing. The first thing to ask for in a bug report. It is
//! written in English whatever the language, so reports read the same everywhere.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::capture::{Backend, SCREENSHOT_BACKENDS};
use crate::desktop::{Desktop, Env, Session};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Manager {
    Apt,
    Dnf,
    Zypper,
    Pacman,
    Apk,
}

impl Manager {
    /// The first of `ids` (the distribution, then the ones it is based on) that is known.
    fn from_ids(ids: &[String]) -> Option<Manager> {
        ids.iter().find_map(|id| match id.as_str() {
            "debian" | "ubuntu" | "linuxmint" | "raspbian" => Some(Manager::Apt),
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => Some(Manager::Dnf),
            "suse" | "sles" => Some(Manager::Zypper),
            id if id.starts_with("opensuse") => Some(Manager::Zypper),
            "arch" | "manjaro" | "endeavouros" => Some(Manager::Pacman),
            "alpine" => Some(Manager::Apk),
            _ => None,
        })
    }

    /// When the distribution is not recognized, whichever package manager is installed.
    fn from_path() -> Option<Manager> {
        [("apt-get", Manager::Apt), ("dnf", Manager::Dnf), ("zypper", Manager::Zypper), ("pacman", Manager::Pacman)]
            .into_iter()
            .chain([("apk", Manager::Apk)])
            .find_map(|(program, manager)| on_path(program).then_some(manager))
    }

    fn name(self) -> &'static str {
        match self {
            Manager::Apt => "apt",
            Manager::Dnf => "dnf",
            Manager::Zypper => "zypper",
            Manager::Pacman => "pacman",
            Manager::Apk => "apk",
        }
    }

    fn install(self, packages: &[&str]) -> String {
        let verb = match self {
            Manager::Apt => "apt install",
            Manager::Dnf => "dnf install",
            Manager::Zypper => "zypper install",
            Manager::Pacman => "pacman -S",
            Manager::Apk => "apk add",
        };
        format!("sudo {verb} {}", packages.join(" "))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct OsRelease {
    pretty_name: String,
    /// `ID`, then everything in `ID_LIKE`.
    ids: Vec<String>,
}

fn parse_os_release(text: &str) -> OsRelease {
    let mut release = OsRelease::default();
    let (mut id, mut like) = (String::new(), String::new());
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches(['"', '\'']).to_string();
        match key.trim() {
            "ID" => id = value,
            "ID_LIKE" => like = value,
            "PRETTY_NAME" => release.pretty_name = value,
            _ => {}
        }
    }
    release.ids =
        std::iter::once(id).chain(like.split_whitespace().map(str::to_owned)).filter(|s| !s.is_empty()).collect();
    release
}

#[derive(Clone, Copy)]
enum When {
    Always,
    Wayland,
    X11,
}

impl When {
    fn applies(self, env: &Env) -> bool {
        match self {
            When::Always => true,
            When::Wayland => env.session == Session::Wayland,
            When::X11 => env.session == Session::X11,
        }
    }
}

struct Tool {
    program: &'static str,
    purpose: &'static str,
    when: When,
    required: bool,
    /// Package names for apt, dnf, zypper, pacman and apk, in that order.
    packages: [&'static str; 5],
}

const TOOLS: &[Tool] = &[
    Tool {
        program: "ffmpeg",
        purpose: "makes GIFs from recordings",
        when: When::Always,
        required: true,
        packages: ["ffmpeg", "ffmpeg-free", "ffmpeg", "ffmpeg", "ffmpeg"],
    },
    Tool {
        program: "wl-copy",
        purpose: "copies to the clipboard on Wayland",
        when: When::Wayland,
        required: true,
        packages: ["wl-clipboard"; 5],
    },
    Tool {
        program: "xclip",
        purpose: "copies to the clipboard on X11",
        when: When::X11,
        required: true,
        packages: ["xclip"; 5],
    },
    Tool {
        program: "gifsicle",
        purpose: "makes GIFs a little smaller",
        when: When::Always,
        required: false,
        packages: ["gifsicle"; 5],
    },
    Tool {
        program: "notify-send",
        purpose: "notifications, if D-Bus has none",
        when: When::Always,
        required: false,
        packages: ["libnotify-bin", "libnotify", "libnotify-tools", "libnotify", "libnotify"],
    },
    Tool {
        program: "zenity",
        purpose: "folder picker, if the portal has none",
        when: When::Always,
        required: false,
        packages: ["zenity"; 5],
    },
    Tool {
        program: "gnome-screenshot",
        purpose: "screenshot fallback",
        when: When::Always,
        required: false,
        packages: ["gnome-screenshot"; 5],
    },
    Tool {
        program: "grim",
        purpose: "screenshot fallback on Wayland",
        when: When::Wayland,
        required: false,
        packages: ["grim"; 5],
    },
];

impl Tool {
    fn package(&self, manager: Manager) -> &'static str {
        self.packages[manager as usize]
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
}

fn is_executable(path: &Path) -> bool {
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Whether something owns `name` on the session bus; `None` if there is no bus.
fn bus_name_taken(name: &'static str) -> Option<bool> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
    bus.name_has_owner(zbus::names::BusName::try_from(name).ok()?).ok()
}

fn yes_no(found: Option<bool>) -> &'static str {
    match found {
        Some(true) => "found",
        Some(false) => "not found",
        None => "could not tell",
    }
}

fn backend_line(backend: Backend, env: &Env) -> String {
    let state = match backend {
        Backend::GnomeShell if env.session == Session::Headless => "no display",
        Backend::GnomeShell => yes_no(bus_name_taken("org.gnome.Shell.Screenshot")),
        Backend::GnomeScreenshot => yes_no(Some(on_path("gnome-screenshot"))),
        Backend::Grim => yes_no(Some(on_path("grim"))),
    };
    format!("{} ({state})", backend.name())
}

/// The report, as text.
fn report(env: &Env) -> String {
    let mut out = String::new();
    let mut line = |text: String| {
        out.push_str(&text);
        out.push('\n');
    };

    let release = std::fs::read_to_string("/etc/os-release").map(|t| parse_os_release(&t)).unwrap_or_default();
    let manager = Manager::from_ids(&release.ids).or_else(Manager::from_path);

    line(format!("queijo {}", env!("CARGO_PKG_VERSION")));
    let session = match env.session {
        Session::Wayland => "Wayland",
        Session::X11 => "X11",
        Session::Headless => "none (no WAYLAND_DISPLAY or DISPLAY)",
    };
    line(format!("Session:  {session}"));
    let version = env.desktop.major_version().map(|v| format!(" {v}")).unwrap_or_default();
    line(format!("Desktop:  {}{version}", env.desktop.name()));
    let distro = if release.pretty_name.is_empty() { "unknown" } else { &release.pretty_name };
    line(format!("Distro:   {distro} (package manager: {})", manager.map_or("unknown", Manager::name)));

    let forced = std::env::var("QUEIJO_SOFTWARE_GL").ok();
    let drawing = if crate::desktop::wants_software_gl(env, forced.as_deref()) { "CPU (software)" } else { "GPU" };
    let reason = if forced.is_some() { " (QUEIJO_SOFTWARE_GL)" } else { "" };
    line(format!("Drawing:  {drawing}{reason}"));

    line(String::new());
    line("Monitors:".into());
    let monitors = crate::monitors::detect();
    for m in &monitors {
        line(format!("  {}: {}  {}x{}+{}+{}", m.index, m.label(), m.width, m.height, m.x, m.y));
    }
    if monitors.is_empty() {
        line("  none found".into());
    }

    line(String::new());
    line("Screenshot, tried in this order:".into());
    for (i, backend) in SCREENSHOT_BACKENDS.iter().enumerate() {
        line(format!("  {}. {}", i + 1, backend_line(*backend, env)));
    }

    line(String::new());
    line("Everything else:".into());
    let recording = if env.desktop == Desktop::Gnome {
        "GNOME Shell screencast".to_string()
    } else {
        "not available on this desktop yet".to_string()
    };
    line(format!("  Recording:        {recording}"));
    let picker = match env.session {
        Session::Headless => "no display".to_string(),
        _ => {
            let conn = zbus::blocking::Connection::session().ok();
            match conn.and_then(|c| crate::shell_ext::list_windows(&c)) {
                Some(_) => "the GNOME Shell extension answers".to_string(),
                None => "no window list (the GNOME Shell extension is not running)".to_string(),
            }
        }
    };
    line(format!("  Window picker:    {picker}"));
    line("  Shortcuts:        GNOME keyboard settings (gsettings)".into());

    line(String::new());
    line("Tools:".into());
    let mut missing = Vec::new();
    for tool in TOOLS.iter().filter(|t| t.when.applies(env)) {
        let found = on_path(tool.program);
        let state = match (found, tool.required) {
            (true, _) => "ok",
            (false, true) => "MISSING",
            (false, false) => "missing (optional)",
        };
        line(format!("  {:<18}{:<20}{}", tool.program, state, tool.purpose));
        if !found {
            missing.push(tool);
        }
    }
    if !missing.is_empty() {
        line(String::new());
        match manager {
            Some(manager) => {
                let packages: Vec<&str> = missing.iter().map(|t| t.package(manager)).collect();
                line(format!("To install what is missing:  {}", manager.install(&packages)));
            }
            None => {
                let names: Vec<&str> = missing.iter().map(|t| t.program).collect();
                line(format!("Install with your package manager: {}", names.join(" ")));
            }
        }
    }
    out
}

pub fn run() {
    print!("{}", report(Env::current()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_os_release() {
        let text =
            "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 22.04.5 LTS\"\nID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"22.04\"\n";
        let release = parse_os_release(text);
        assert_eq!(release.pretty_name, "Ubuntu 22.04.5 LTS");
        assert_eq!(release.ids, ids(&["ubuntu", "debian"]));
        assert_eq!(parse_os_release(""), OsRelease::default());
        assert_eq!(parse_os_release("ID='arch'\n").ids, ids(&["arch"]));
    }

    #[test]
    fn picks_the_package_manager_of_the_distribution() {
        let cases = [
            (&["ubuntu", "debian"][..], Manager::Apt),
            (&["pop", "ubuntu", "debian"], Manager::Apt),
            (&["fedora"], Manager::Dnf),
            (&["rocky", "rhel", "centos", "fedora"], Manager::Dnf),
            (&["opensuse-tumbleweed", "opensuse", "suse"], Manager::Zypper),
            (&["sles", "suse"], Manager::Zypper),
            (&["arch"], Manager::Pacman),
            (&["manjaro", "arch"], Manager::Pacman),
            (&["alpine"], Manager::Apk),
        ];
        for (list, expected) in cases {
            assert_eq!(Manager::from_ids(&ids(list)), Some(expected), "{list:?}");
        }
        assert_eq!(Manager::from_ids(&ids(&["nixos"])), None);
        assert_eq!(Manager::from_ids(&[]), None);
    }

    #[test]
    fn writes_the_install_command() {
        assert_eq!(Manager::Apt.install(&["xclip", "zenity"]), "sudo apt install xclip zenity");
        assert_eq!(Manager::Pacman.install(&["grim"]), "sudo pacman -S grim");
        assert_eq!(Manager::Apk.install(&["grim"]), "sudo apk add grim");
    }

    #[test]
    fn every_tool_has_a_package_for_every_manager() {
        for tool in TOOLS {
            for manager in [Manager::Apt, Manager::Dnf, Manager::Zypper, Manager::Pacman, Manager::Apk] {
                assert!(!tool.package(manager).is_empty(), "{} on {}", tool.program, manager.name());
            }
        }
        let notify = TOOLS.iter().find(|t| t.program == "notify-send").unwrap();
        assert_eq!(notify.package(Manager::Apt), "libnotify-bin");
        assert_eq!(notify.package(Manager::Zypper), "libnotify-tools");
        assert_eq!(notify.package(Manager::Pacman), "libnotify");
    }

    #[test]
    fn only_asks_for_the_clipboard_tool_of_the_session() {
        let x11 = Env { session: Session::X11, desktop: Desktop::Xfce };
        let wayland = Env { session: Session::Wayland, desktop: Desktop::Sway };
        let wanted = |env: &Env| TOOLS.iter().filter(|t| t.when.applies(env)).map(|t| t.program).collect::<Vec<_>>();
        assert!(wanted(&x11).contains(&"xclip") && !wanted(&x11).contains(&"wl-copy"));
        assert!(wanted(&wayland).contains(&"wl-copy") && !wanted(&wayland).contains(&"xclip"));
    }

    #[test]
    fn the_report_names_the_environment() {
        let text = report(&Env { session: Session::Headless, desktop: Desktop::Other("LXQt".into()) });
        assert!(text.starts_with(&format!("queijo {}\n", env!("CARGO_PKG_VERSION"))), "{text}");
        assert!(text.contains("Desktop:  LXQt"), "{text}");
        assert!(text.contains("1. GNOME Shell (no display)"), "{text}");
        assert!(text.contains("ffmpeg"), "{text}");
    }
}
