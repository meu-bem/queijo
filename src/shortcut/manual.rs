//! Desktops whose keyboard shortcuts Queijo can't write: the ones kept in a configuration
//! file the user edits (Sway, Hyprland, i3), and any desktop it knows nothing about. It shows
//! what to add, ready to copy, instead of touching the user's configuration.

use crate::desktop::{Desktop, Env};

use super::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Sway,
    Hyprland,
    I3,
    /// KDE keeps shortcuts in `kglobalshortcutsrc`, and nothing found says how to make
    /// `kglobalacceld` notice a change, so it is left to System Settings.
    Kde,
    Other,
}

pub fn kind(env: &Env) -> Kind {
    match &env.desktop {
        Desktop::Sway => Kind::Sway,
        Desktop::Hyprland => Kind::Hyprland,
        Desktop::Kde => Kind::Kde,
        Desktop::Other(name) if name.eq_ignore_ascii_case("i3") || std::env::var_os("I3SOCK").is_some() => Kind::I3,
        _ => Kind::Other,
    }
}

/// The keys suggested, as (shift, ctrl, alt) held with Print Screen.
fn suggested(action: Action) -> (bool, bool, bool) {
    match action {
        Action::Open => (false, false, false),
        Action::Video => (true, false, false),
        Action::Gif => (false, true, false),
        Action::FullScreen => (false, false, true),
    }
}

/// What to add for `action`, as the configuration of `kind` writes it. `exe` is how to run Queijo.
pub fn line(kind: Kind, action: Action, exe: &str) -> String {
    let command = format!("{exe} {}", action.args()).trim().to_string();
    let (shift, ctrl, alt) = suggested(action);
    match kind {
        Kind::Sway | Kind::I3 => {
            let mut keys: Vec<&str> = Vec::new();
            if shift {
                keys.push("Shift");
            }
            if ctrl {
                keys.push("Ctrl");
            }
            if alt {
                keys.push("Mod1");
            }
            keys.push("Print");
            let startup = if kind == Kind::I3 { " --no-startup-id" } else { "" };
            format!("bindsym {} exec{startup} {command}", keys.join("+"))
        }
        Kind::Hyprland => {
            let mods: Vec<&str> = [(shift, "SHIFT"), (ctrl, "CTRL"), (alt, "ALT")]
                .into_iter()
                .filter(|(on, _)| *on)
                .map(|(_, m)| m)
                .collect();
            format!("bind = {}, Print, exec, {command}", mods.join(" "))
        }
        // No file to write: the command to give a shortcut.
        Kind::Kde | Kind::Other => command,
    }
}

/// All the lines, one per action, for copying.
pub fn lines(kind: Kind, exe: &str) -> Vec<(Action, String)> {
    Action::ALL.into_iter().map(|a| (a, line(kind, a, exe))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::Session;

    fn env(desktop: Desktop) -> Env {
        Env { session: Session::Wayland, desktop, compositing: true }
    }

    #[test]
    fn knows_the_desktops_that_keep_shortcuts_in_a_file() {
        assert_eq!(kind(&env(Desktop::Sway)), Kind::Sway);
        assert_eq!(kind(&env(Desktop::Hyprland)), Kind::Hyprland);
        assert_eq!(kind(&env(Desktop::Kde)), Kind::Kde);
        assert_eq!(kind(&env(Desktop::Other("i3".into()))), Kind::I3);
        assert_eq!(kind(&env(Desktop::Other("LXQt".into()))), Kind::Other);
    }

    #[test]
    fn writes_lines_in_each_configuration_s_own_words() {
        let all = |kind| lines(kind, "queijo").into_iter().map(|(_, l)| l).collect::<Vec<_>>();
        assert_eq!(
            all(Kind::Sway),
            [
                "bindsym Print exec queijo",
                "bindsym Shift+Print exec queijo --kind video",
                "bindsym Ctrl+Print exec queijo --kind gif",
                "bindsym Mod1+Print exec queijo --quick",
            ]
        );
        assert_eq!(line(Kind::I3, Action::Open, "queijo"), "bindsym Print exec --no-startup-id queijo");
        assert_eq!(
            all(Kind::Hyprland),
            [
                "bind = , Print, exec, queijo",
                "bind = SHIFT, Print, exec, queijo --kind video",
                "bind = CTRL, Print, exec, queijo --kind gif",
                "bind = ALT, Print, exec, queijo --quick",
            ]
        );
        // Where there is no file, the line is the command to give a shortcut.
        assert_eq!(all(Kind::Kde)[2], "queijo --kind gif");
        assert_eq!(all(Kind::Other)[0], "queijo");
        assert_eq!(
            line(Kind::Sway, Action::Open, "/home/ann/.local/bin/queijo"),
            "bindsym Print exec /home/ann/.local/bin/queijo"
        );
    }
}
