//! `queijo --doctor`: what Queijo found on this system, what it will use for each
//! job, and what is missing. The first thing to ask for in a bug report. It is
//! written in English whatever the language, so reports read the same everywhere.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::capture::{screenshot_backends, Backend};
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
    Kde,
    Wlroots,
}

impl When {
    #[cfg(test)]
    fn describe(self) -> &'static str {
        match self {
            When::Always => "always",
            When::Wayland => "Wayland",
            When::X11 => "X11",
            When::Kde => "KDE Plasma",
            When::Wlroots => "Sway and Hyprland",
        }
    }

    fn applies(self, env: &Env) -> bool {
        match self {
            When::Always => true,
            When::Wayland => env.session == Session::Wayland,
            When::X11 => env.session == Session::X11,
            When::Kde => env.desktop == Desktop::Kde,
            When::Wlroots => matches!(env.desktop, Desktop::Sway | Desktop::Hyprland),
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
        purpose: "recording on X11, and makes GIFs from recordings",
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
        program: "gsettings",
        purpose: "keyboard shortcuts on GNOME, Cinnamon and MATE, and `queijo --enable-extension`",
        when: When::Always,
        required: false,
        packages: ["libglib2.0-bin", "glib2", "glib2-tools", "glib2", "glib"],
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
        program: "gst-launch-1.0",
        purpose: "screen recording through the portal (with GStreamer's pipewire plugin)",
        when: When::Wayland,
        required: false,
        packages: ["gstreamer1.0-tools", "gstreamer1", "gstreamer-utils", "gstreamer", "gstreamer-tools"],
    },
    Tool {
        program: "wf-recorder",
        purpose: "screen recording on Sway and Hyprland",
        when: When::Wlroots,
        required: true,
        packages: ["wf-recorder"; 5],
    },
    Tool {
        program: "spectacle",
        purpose: "screenshot fallback on KDE",
        when: When::Kde,
        required: false,
        packages: ["kde-spectacle", "spectacle", "spectacle", "spectacle", "spectacle"],
    },
    Tool {
        program: "grim",
        purpose: "screenshot fallback on Wayland",
        when: When::Wayland,
        required: false,
        packages: ["grim"; 5],
    },
];

/// A library the editor opens when it starts. They are found at run time, so no package
/// manager sees them as dependencies and each package lists them by hand.
pub struct Library {
    pub soname: &'static str,
    when: When,
    /// Package names for apt, dnf, zypper, pacman and apk, in that order.
    packages: [&'static str; 5],
}

pub const LIBRARIES: &[Library] = &[
    Library {
        soname: "libwayland-client.so.0",
        when: When::Wayland,
        packages: ["libwayland-client0", "libwayland-client", "libwayland-client0", "wayland", "wayland-libs-client"],
    },
    Library {
        soname: "libxkbcommon.so.0",
        when: When::Always,
        packages: ["libxkbcommon0", "libxkbcommon", "libxkbcommon0", "libxkbcommon", "libxkbcommon"],
    },
    Library {
        soname: "libxkbcommon-x11.so.0",
        when: When::X11,
        packages: [
            "libxkbcommon-x11-0",
            "libxkbcommon-x11",
            "libxkbcommon-x11-0",
            "libxkbcommon-x11",
            "libxkbcommon-x11",
        ],
    },
    Library {
        soname: "libEGL.so.1",
        when: When::Always,
        packages: ["libegl1", "libglvnd-egl", "libglvnd", "libglvnd", "mesa-egl"],
    },
    Library {
        soname: "libGL.so.1",
        when: When::Always,
        packages: ["libgl1", "libglvnd-glx", "libglvnd", "libglvnd", "mesa-gl"],
    },
    Library {
        soname: "libX11.so.6",
        when: When::X11,
        packages: ["libx11-6", "libX11", "libX11-6", "libx11", "libx11"],
    },
    Library { soname: "libxcb.so.1", when: When::X11, packages: ["libxcb1", "libxcb", "libxcb1", "libxcb", "libxcb"] },
    Library {
        soname: "libXcursor.so.1",
        when: When::X11,
        packages: ["libxcursor1", "libXcursor", "libXcursor1", "libxcursor", "libxcursor"],
    },
    Library { soname: "libXi.so.6", when: When::X11, packages: ["libxi6", "libXi", "libXi6", "libxi", "libxi"] },
    Library {
        soname: "libXrandr.so.2",
        when: When::X11,
        packages: ["libxrandr2", "libXrandr", "libXrandr2", "libxrandr", "libxrandr"],
    },
];

impl Library {
    fn package(&self, manager: Manager) -> &'static str {
        self.packages[manager as usize]
    }

    /// Whether the library is installed, looked for where the dynamic linker looks.
    fn present(&self) -> bool {
        let mut dirs: Vec<std::path::PathBuf> =
            std::env::var_os("LD_LIBRARY_PATH").iter().flat_map(std::env::split_paths).collect();
        for root in ["/usr/lib", "/lib", "/usr/lib64", "/lib64", "/usr/local/lib"] {
            dirs.push(root.into());
            // Debian and Ubuntu keep them in a folder named for the machine, like x86_64-linux-gnu.
            if let Ok(entries) = std::fs::read_dir(root) {
                dirs.extend(
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("-linux-gnu"))),
                );
            }
        }
        dirs.iter().any(|dir| dir.join(self.soname).exists())
    }
}

/// `packaging/dependencies.md`, which says the same as the tables above for whoever packages
/// Queijo; a test keeps the two equal.
#[cfg(test)]
fn dependencies_markdown() -> String {
    let columns = "Debian, Ubuntu (apt) | Fedora, RHEL (dnf) | openSUSE (zypper) | Arch (pacman) | Alpine (apk)";
    let mut out = String::from(
        "# Dependencies, by distribution\n\n\
         Generated from `src/doctor.rs` (`QUEIJO_UPDATE_DOCS=1 cargo test dependencies_table` rewrites it), which is also\n\
         what `queijo --doctor` checks and what the packages declare. The Alpine names are from memory: its\n\
         musl C library can't run the release binary.\n\n\
         ## Libraries\n\n\
         Opened when the editor starts, so no package manager finds them as dependencies; every package lists them.\n\n",
    );
    out.push_str(&format!("| Library | Needed on | {columns} |\n|---|---|---|---|---|---|---|\n"));
    for lib in LIBRARIES {
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            lib.soname,
            lib.when.describe(),
            lib.packages.map(|p| format!("`{p}`")).join(" | ")
        ));
    }
    out.push_str("\n## Programs\n\n\"Required\" ones are what the feature needs to work at all; the others are fallbacks or extras.\n\n");
    out.push_str(&format!(
        "| Program | Used for | Needed on | Required | {columns} |\n|---|---|---|---|---|---|---|---|---|\n"
    ));
    for tool in TOOLS {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} |\n",
            tool.program,
            tool.purpose,
            tool.when.describe(),
            if tool.required { "yes" } else { "no" },
            tool.packages.map(|p| format!("`{p}`")).join(" | ")
        ));
    }
    out
}

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

/// Whether `name` is on the session bus: owned now, or started when first used (some
/// services, like GNOME's screencast, only take their name then). `None` if there is no bus.
fn bus_name_taken(name: &'static str) -> Option<bool> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&conn).ok()?;
    let wanted = zbus::names::BusName::try_from(name).ok()?;
    if bus.name_has_owner(wanted).ok()? {
        return Some(true);
    }
    let activatable = bus.list_activatable_names().ok()?;
    Some(activatable.iter().any(|n| n.as_str() == name))
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
        Backend::GnomeShell | Backend::X11 if env.session == Session::Headless => "no display",
        Backend::X11 => yes_no(Some(crate::x11::screenshot(None).is_ok())),
        Backend::Portal => yes_no(bus_name_taken("org.freedesktop.portal.Desktop")),
        Backend::Extension => yes_no(Some(crate::shell_ext::ShellExtension::connect().is_some())),
        Backend::KWin => yes_no(bus_name_taken("org.kde.KWin")),
        Backend::Spectacle => yes_no(Some(on_path("spectacle"))),
        Backend::GnomeShell => yes_no(bus_name_taken("org.gnome.Shell.Screenshot")),
        Backend::GnomeScreenshot => yes_no(Some(on_path("gnome-screenshot"))),
        Backend::Grim => yes_no(Some(on_path("grim"))),
    };
    format!("{} ({state})", backend.name())
}

/// Whether GStreamer has the element (`gst-inspect-1.0` finds it).
fn gst_element(name: &str) -> bool {
    std::process::Command::new("gst-inspect-1.0").arg(name).output().is_ok_and(|out| out.status.success())
}

fn recording_line(backend: crate::record::Backend, env: &Env) -> String {
    use crate::record::Backend::*;
    let state = match backend {
        GnomeShell if env.session == Session::Headless => "no display",
        GnomeShell => yes_no(bus_name_taken("org.gnome.Shell.Screencast")),
        X11 => yes_no(Some(on_path("ffmpeg"))),
        Wlr => yes_no(Some(on_path("wf-recorder"))),
        Portal => {
            let ready = on_path("gst-launch-1.0") && gst_element("pipewiresrc");
            match (bus_name_taken("org.freedesktop.portal.Desktop"), ready) {
                (Some(true), true) => "found",
                (Some(true), false) => "the portal is there, but gst-launch-1.0 or its pipewire plugin is not",
                (Some(false), _) => "no portal on the bus",
                (None, _) => "could not tell",
            }
        }
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

    let compositor = match (env.session, env.compositing) {
        (Session::X11, false) => "none (the editor shows a still picture of the screen)",
        (Session::Wayland, false) => "windows are opaque here (the editor shows a still picture of the screen)",
        (Session::X11, true) => "yes",
        _ => "built into the compositor",
    };
    line(format!("See-through windows: {compositor}"));
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
    for (i, backend) in screenshot_backends(env).iter().enumerate() {
        line(format!("  {}. {}", i + 1, backend_line(*backend, env)));
    }

    line(String::new());
    line("Recording, tried in this order:".into());
    for (i, backend) in crate::record::backends(env).iter().enumerate() {
        line(format!("  {}. {}", i + 1, recording_line(*backend, env)));
    }

    line(String::new());
    line("Everything else:".into());
    let picker = match env.session {
        Session::Headless => "no display".to_string(),
        _ => match crate::windows::find(env) {
            Some((source, list)) => format!("{} lists {} windows", source.name(), list.len()),
            None if env.desktop == Desktop::Gnome => {
                "no window list (the Queijo extension is not running: `queijo --enable-extension`, then log out and in)"
                    .to_string()
            }
            None => "no window list (only the focused window can be picked)".to_string(),
        },
    };
    line(format!("  Window picker:    {picker}"));
    line(format!("  Shortcuts:        {}", crate::shortcut::mechanism(env).name()));
    let tray = match (
        crate::shell_ext::ShellExtension::connect().is_some(),
        bus_name_taken("org.kde.StatusNotifierWatcher"),
    ) {
        (true, _) => "the GNOME Shell extension shows the recording indicator",
        (false, Some(true)) => "a tray icon while recording (a system tray is there)",
        (false, Some(false)) => "no system tray found, so no indicator while recording",
        (false, None) => "could not tell",
    };
    line(format!("  Recording icon:   {tray}"));

    line(String::new());
    line("Libraries (opened when the editor starts):".into());
    let mut missing_libs = Vec::new();
    for lib in LIBRARIES.iter().filter(|l| l.when.applies(env)) {
        let found = lib.present();
        line(format!("  {:<24}{}", lib.soname, if found { "ok" } else { "MISSING" }));
        if !found {
            missing_libs.push(lib);
        }
    }
    if let (false, Some(manager)) = (missing_libs.is_empty(), manager) {
        let packages: Vec<&str> = missing_libs.iter().map(|l| l.package(manager)).collect();
        line(format!("To install the missing libraries:  {}", manager.install(&packages)));
    }

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
        let x11 = Env { session: Session::X11, desktop: Desktop::Xfce, compositing: true };
        let wayland = Env { session: Session::Wayland, desktop: Desktop::Sway, compositing: true };
        let wanted = |env: &Env| TOOLS.iter().filter(|t| t.when.applies(env)).map(|t| t.program).collect::<Vec<_>>();
        assert!(wanted(&x11).contains(&"xclip") && !wanted(&x11).contains(&"wl-copy"));
        assert!(wanted(&wayland).contains(&"wl-copy") && !wanted(&wayland).contains(&"xclip"));
    }

    #[test]
    fn the_report_names_the_environment() {
        let text =
            report(&Env { session: Session::Headless, desktop: Desktop::Other("LXQt".into()), compositing: true });
        assert!(text.starts_with(&format!("queijo {}\n", env!("CARGO_PKG_VERSION"))), "{text}");
        assert!(text.contains("Desktop:  LXQt"), "{text}");
        assert!(text.contains("1. GNOME Shell (no display)"), "{text}");
        assert!(text.contains("ffmpeg"), "{text}");
    }

    #[test]
    fn every_library_has_a_package_for_every_manager() {
        for lib in LIBRARIES {
            for manager in [Manager::Apt, Manager::Dnf, Manager::Zypper, Manager::Pacman, Manager::Apk] {
                assert!(!lib.package(manager).is_empty(), "{} on {}", lib.soname, manager.name());
            }
        }
    }

    /// `packaging/dependencies.md` says what the code says. `QUEIJO_UPDATE_DOCS=1` rewrites it.
    #[test]
    fn dependencies_table_is_up_to_date() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/dependencies.md");
        let wanted = dependencies_markdown();
        if std::env::var_os("QUEIJO_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &wanted).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == wanted,
            "packaging/dependencies.md is out of date: run QUEIJO_UPDATE_DOCS=1 cargo test dependencies_table"
        );
    }

    #[test]
    fn finds_a_library_that_is_there_and_not_one_that_is_not() {
        let libc = Library { soname: "libc.so.6", when: When::Always, packages: [""; 5] };
        assert!(libc.present(), "libc is on every machine this runs on");
        let nope = Library { soname: "libqueijo-nothing.so.9", when: When::Always, packages: [""; 5] };
        assert!(!nope.present());
    }

    /// The packages say the same as the table: every library is named in the `.deb`'s
    /// dependencies (by its apt package), the `.rpm`'s requirements (by its soname) and the
    /// PKGBUILD's (by its pacman package), and every program a package needs to work at all
    /// is one of its dependencies.
    #[test]
    fn the_packages_declare_what_the_table_says() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let cargo = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        let pkgbuild = std::fs::read_to_string(root.join("packaging/aur/PKGBUILD.in")).unwrap();
        let deb = &cargo[cargo.find("depends = \"\"\"").unwrap()..];
        let deb = &deb[..deb.find("\"\"\"\n").unwrap() + 3];
        let (rpm_requires, _) = cargo.split_once("[package.metadata.generate-rpm.recommends]").unwrap();
        let rpm_requires = &rpm_requires[rpm_requires.find("[package.metadata.generate-rpm.requires]").unwrap()..];

        for lib in LIBRARIES {
            let apt = lib.package(Manager::Apt);
            assert!(deb.split([',', ' ', '\\', '"', '\n']).any(|w| w == apt), "the .deb does not depend on {apt}");
            assert!(
                rpm_requires.contains(&format!("\"{}()(64bit)\"", lib.soname)),
                "the .rpm does not require {}",
                lib.soname
            );
            let arch = lib.package(Manager::Pacman);
            assert!(pkgbuild.contains(&format!("'{arch}'")), "the PKGBUILD does not depend on {arch}");
        }
        // ffmpeg and a clipboard tool are what the .deb and the PKGBUILD insist on.
        assert!(deb.contains("ffmpeg") && deb.contains("wl-clipboard | xclip"));
        assert!(pkgbuild.contains("'ffmpeg'") && pkgbuild.contains("'wl-clipboard'"));
    }
}
