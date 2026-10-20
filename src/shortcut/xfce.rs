//! Shortcuts on Xfce, which keeps them in `xfconf`: the channel `xfce4-keyboard-shortcuts`,
//! where the key itself is the name of the property and the command is its value:
//! `/commands/custom/<Super>e` is `thunar`. What Xfce ships is under `/commands/default/`; a
//! custom one for the same key takes its place as long as `/commands/custom/override` is true.

use std::process::Command;

use super::{is_queijo, normalize, pretty, program, shell_quote, Action};

const CHANNEL: &str = "xfce4-keyboard-shortcuts";
const CUSTOM: &str = "/commands/custom/";
const DEFAULT: &str = "/commands/default/";

/// Where the shortcuts live: `xfconf-query`, or a map in tests.
pub trait Xfconf {
    /// `xfconf-query -l -v` for the channel; `None` if it can't be asked.
    fn list(&self) -> Option<String>;
    fn set(&self, property: &str, kind: &str, value: &str) -> Result<(), String>;
    fn remove(&self, property: &str) -> Result<(), String>;
}

/// The real thing, optionally with extra environment (a test points it at its own bus).
#[derive(Default)]
pub struct XfconfQuery {
    env: Vec<(String, String)>,
}

impl XfconfQuery {
    #[cfg(test)]
    pub fn with_env(env: Vec<(String, String)>) -> Self {
        XfconfQuery { env }
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("xfconf-query")
            .args(["-c", CHANNEL])
            .args(args)
            .envs(self.env.iter().cloned())
            .output()
            .map_err(|e| format!("xfconf-query: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

impl Xfconf for XfconfQuery {
    fn list(&self) -> Option<String> {
        self.run(&["-l", "-v"]).ok()
    }

    fn set(&self, property: &str, kind: &str, value: &str) -> Result<(), String> {
        self.run(&["-n", "-t", kind, "-p", property, "-s", value]).map(|_| ())
    }

    fn remove(&self, property: &str) -> Result<(), String> {
        self.run(&["-r", "-p", property]).map(|_| ())
    }
}

struct Shortcut {
    accel: String,
    command: String,
    custom: bool,
}

/// The shortcuts in `xfconf-query -l -v`, which prints each property and its value in
/// columns. The settings of a shortcut (like `startup-notify`) are properties below it, and
/// `override` is not a key.
fn parse(listing: &str) -> Vec<Shortcut> {
    listing
        .lines()
        .filter_map(|line| {
            let (path, value) = line.split_once(char::is_whitespace)?;
            let (custom, accel) = match (path.strip_prefix(CUSTOM), path.strip_prefix(DEFAULT)) {
                (Some(a), _) => (true, a),
                (_, Some(a)) => (false, a),
                _ => return None,
            };
            (!accel.is_empty() && !accel.contains('/') && accel != "override").then(|| Shortcut {
                accel: accel.to_string(),
                command: value.trim().to_string(),
                custom,
            })
        })
        .collect()
}

fn shortcuts(x: &dyn Xfconf) -> Vec<Shortcut> {
    x.list().map(|l| parse(&l)).unwrap_or_default()
}

/// Current keys per action.
pub fn current_in(x: &dyn Xfconf) -> Vec<(Action, Vec<String>)> {
    let all = shortcuts(x);
    Action::ALL
        .into_iter()
        .map(|action| {
            let keys = all
                .iter()
                .filter(|s| s.custom && is_queijo(&s.command) && Action::from_command(&s.command) == action)
                .map(|s| s.accel.clone())
                .collect();
            (action, keys)
        })
        .collect()
}

/// The program a shortcut of ours runs, if it is a file that exists.
pub fn installed_program(x: &dyn Xfconf) -> Option<String> {
    shortcuts(x)
        .iter()
        .filter(|s| s.custom && is_queijo(&s.command))
        .filter_map(|s| program(&s.command))
        .map(|p| if p.contains('/') { p } else { format!("{}/.local/bin/{p}", crate::config::home().display()) })
        .find(|p| std::path::Path::new(p).is_file())
}

fn what(command: &str) -> String {
    let name = program(command).unwrap_or_default();
    let name = name.rsplit('/').next().unwrap_or_default().to_string();
    format!("shortcut for “{name}”")
}

/// Who else uses this key: (description, will Queijo take it over automatically). What Xfce
/// ships is replaced by ours; one the user made is not, because it has the same property.
pub fn conflict_in(x: &dyn Xfconf, binding: &str) -> Option<(String, bool)> {
    let want = normalize(binding);
    let all = shortcuts(x);
    let theirs = |s: &&Shortcut| !is_queijo(&s.command) && normalize(&s.accel) == want;
    // One the user made counts before one Xfce ships.
    all.iter()
        .filter(theirs)
        .find(|s| s.custom)
        .or_else(|| all.iter().find(theirs))
        .map(|s| (what(&s.command), !s.custom))
}

/// Replaces all of Queijo's shortcuts with `bindings`. A key somebody made a custom shortcut for is
/// refused, as writing it would replace theirs; what Xfce ships for a key is replaced.
pub fn apply_in(x: &dyn Xfconf, exe: &str, bindings: &[(Action, Vec<String>)]) -> Result<(), String> {
    let all = shortcuts(x);
    let keys: Vec<(&Action, &String)> =
        bindings.iter().flat_map(|(a, keys)| keys.iter().filter(|k| !k.is_empty()).map(move |k| (a, k))).collect();
    for (_, key) in &keys {
        if let Some(theirs) =
            all.iter().find(|s| s.custom && !is_queijo(&s.command) && normalize(&s.accel) == normalize(key))
        {
            return Err(format!("{} is already used by a {}", pretty(key), what(&theirs.command)));
        }
    }

    for old in all.iter().filter(|s| s.custom && is_queijo(&s.command)) {
        x.remove(&format!("{CUSTOM}{}", old.accel))?;
    }
    let mut takes_over_a_default = false;
    for (action, key) in keys {
        let command = format!("{} {}", shell_quote(exe), action.args()).trim().to_string();
        x.set(&format!("{CUSTOM}{key}"), "string", &command)?;
        takes_over_a_default |= all.iter().any(|s| !s.custom && normalize(&s.accel) == normalize(key));
    }
    if takes_over_a_default {
        // A custom shortcut only wins over a default one with this switched on.
        x.set(&format!("{CUSTOM}override"), "bool", "true")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The channel as a list of (property, value), the way `xfconf-query -l -v` shows it.
    struct Fake(RefCell<Vec<(String, String)>>);

    impl Fake {
        /// An Xfce with its own screenshot key and terminal key, and one shortcut the user made.
        fn desktop() -> Self {
            let rows = [
                ("/commands/custom/override", "true"),
                ("/commands/custom/<Super>e", "thunar"),
                ("/commands/default/<Primary><Alt>t", "exo-open --launch TerminalEmulator"),
                ("/commands/default/<Primary><Alt>t/startup-notify", "true"),
                ("/commands/default/Print", "xfce4-screenshooter -f"),
            ];
            Fake(RefCell::new(rows.iter().map(|(p, v)| (p.to_string(), v.to_string())).collect()))
        }

        fn value(&self, property: &str) -> Option<String> {
            self.0.borrow().iter().find(|(p, _)| p == property).map(|(_, v)| v.clone())
        }
    }

    impl Xfconf for Fake {
        fn list(&self) -> Option<String> {
            // Padded into columns, as the real command does it.
            let rows = self.0.borrow();
            Some(rows.iter().map(|(p, v)| format!("{p:<48}{v}\n")).collect())
        }

        fn set(&self, property: &str, _kind: &str, value: &str) -> Result<(), String> {
            let mut rows = self.0.borrow_mut();
            rows.retain(|(p, _)| p != property);
            rows.push((property.to_string(), value.to_string()));
            Ok(())
        }

        fn remove(&self, property: &str) -> Result<(), String> {
            self.0.borrow_mut().retain(|(p, _)| p != property);
            Ok(())
        }
    }

    fn bindings(open: &[&str], gif: &[&str]) -> Vec<(Action, Vec<String>)> {
        let own = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        vec![(Action::Open, own(open)), (Action::Gif, own(gif)), (Action::Video, vec![]), (Action::FullScreen, vec![])]
    }

    fn keys_of(current: &[(Action, Vec<String>)], action: Action) -> Vec<String> {
        current.iter().find(|(a, _)| *a == action).unwrap().1.clone()
    }

    #[test]
    fn reads_shortcuts_and_leaves_out_their_settings() {
        let found = parse(&Fake::desktop().list().unwrap());
        let keys: Vec<_> = found.iter().map(|s| (s.custom, s.accel.as_str())).collect();
        assert_eq!(keys, [(true, "<Super>e"), (false, "<Primary><Alt>t"), (false, "Print")]);
        assert_eq!(found[1].command, "exo-open --launch TerminalEmulator", "a value with spaces is kept whole");
        assert!(parse("").is_empty());
        assert!(parse("garbage\n/other/channel  x\n").is_empty());
    }

    #[test]
    fn applying_adds_ours_and_takes_a_default_key_over() {
        let x = Fake::desktop();
        apply_in(&x, "/usr/bin/queijo", &bindings(&["Print"], &["<Primary><Shift>g"])).unwrap();
        let current = current_in(&x);
        assert_eq!(keys_of(&current, Action::Open), ["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), ["<Primary><Shift>g"]);
        assert_eq!(x.value("/commands/custom/<Primary><Shift>g").unwrap(), "/usr/bin/queijo --kind gif");
        assert_eq!(x.value("/commands/custom/Print").unwrap(), "/usr/bin/queijo");
        // Theirs, and Xfce's own, are untouched; the override switch is on.
        assert_eq!(x.value("/commands/custom/<Super>e").unwrap(), "thunar");
        assert_eq!(x.value("/commands/default/Print").unwrap(), "xfce4-screenshooter -f");
        assert_eq!(x.value("/commands/custom/override").unwrap(), "true");
    }

    #[test]
    fn reapplying_replaces_ours() {
        let x = Fake::desktop();
        apply_in(&x, "/usr/bin/queijo", &bindings(&["Print"], &["<Primary><Shift>g"])).unwrap();
        apply_in(&x, "/usr/bin/queijo", &bindings(&["<Alt>Print"], &[])).unwrap();
        let current = current_in(&x);
        assert_eq!(keys_of(&current, Action::Open), ["<Alt>Print"]);
        assert!(keys_of(&current, Action::Gif).is_empty());
        assert!(x.value("/commands/custom/Print").is_none());
        apply_in(&x, "/usr/bin/queijo", &bindings(&[], &[])).unwrap();
        assert!(current_in(&x).iter().all(|(_, keys)| keys.is_empty()));
        assert_eq!(x.value("/commands/custom/<Super>e").unwrap(), "thunar");
    }

    #[test]
    fn a_key_the_user_made_a_shortcut_for_is_refused_and_nothing_changes() {
        let x = Fake::desktop();
        apply_in(&x, "/usr/bin/queijo", &bindings(&["Print"], &[])).unwrap();
        let before = x.0.borrow().clone();
        let err = apply_in(&x, "/usr/bin/queijo", &bindings(&["Print", "<Super>E"], &[])).unwrap_err();
        assert!(err.contains("Super+E") && err.contains("thunar"), "{err}");
        assert_eq!(*x.0.borrow(), before, "it checked before it wrote");
    }

    #[test]
    fn says_who_else_has_a_key() {
        let x = Fake::desktop();
        assert_eq!(conflict_in(&x, "Print"), Some(("shortcut for “xfce4-screenshooter”".into(), true)));
        assert_eq!(conflict_in(&x, "<Super>e"), Some(("shortcut for “thunar”".into(), false)));
        assert_eq!(conflict_in(&x, "<Control><Alt>T").map(|c| c.1), Some(true), "notation does not matter");
        assert_eq!(conflict_in(&x, "<Super>q"), None);
        apply_in(&x, "/usr/bin/queijo", &bindings(&["<Super>q"], &[])).unwrap();
        assert_eq!(conflict_in(&x, "<Super>q"), None, "our own is not a conflict");
    }

    /// The same flow with the real `xfconf-query` and `xfconfd`, on a bus of its own. Needs the
    /// `xfconf` package. QUEIJO_REQUIRE_XFCONF=1 makes a missing one a failure (CI does); the
    /// daemon is looked for where distributions put it, or at QUEIJO_XFCONFD.
    #[test]
    fn works_with_the_real_xfconf() {
        let daemon = std::env::var("QUEIJO_XFCONFD").ok().into_iter().chain(
            [
                "/usr/lib/x86_64-linux-gnu/xfce4/xfconf/xfconfd",
                "/usr/lib/xfce4/xfconf/xfconfd",
                "/usr/lib64/xfce4/xfconf/xfconfd",
            ]
            .map(String::from),
        );
        let daemon = daemon.into_iter().find(|p| std::path::Path::new(p).is_file());
        let have_query = Command::new("xfconf-query").arg("--version").output().is_ok();
        let (Some(daemon), true) = (daemon, have_query) else {
            assert!(std::env::var_os("QUEIJO_REQUIRE_XFCONF").is_none(), "xfconf is required here");
            eprintln!("skipping: xfconf is not installed");
            return;
        };
        let Some(bus) = crate::testutil::PrivateBus::start() else { return };
        let config = tempfile::tempdir().unwrap();
        let env = vec![
            ("DBUS_SESSION_BUS_ADDRESS".to_string(), bus.address.clone()),
            ("XDG_CONFIG_HOME".to_string(), config.path().to_string_lossy().into_owned()),
        ];
        let mut child = Command::new(daemon).envs(env.iter().cloned()).spawn().unwrap();
        // Until the daemon has its name on the bus nothing can be asked.
        let up = (0..100).any(|_| {
            let ready = Command::new("xfconf-query")
                .arg("--list")
                .envs(env.iter().cloned())
                .output()
                .is_ok_and(|o| o.status.success());
            if !ready {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            ready
        });
        assert!(up, "xfconfd did not come up");
        let x = XfconfQuery::with_env(env);
        // Xfce's own shortcuts, as it would have them.
        x.set("/commands/default/Print", "string", "xfce4-screenshooter -f").unwrap();
        x.set("/commands/default/<Primary><Alt>t/startup-notify", "bool", "true").unwrap();
        x.set("/commands/custom/<Super>e", "string", "thunar").unwrap();

        apply_in(&x, "/opt/it's here/queijo", &bindings(&["Print"], &["<Primary><Shift>g"])).unwrap();
        let current = current_in(&x);
        assert_eq!(keys_of(&current, Action::Open), ["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), ["<Primary><Shift>g"]);
        let listing = x.list().unwrap();
        let gif = parse(&listing).into_iter().find(|s| s.command.ends_with("--kind gif")).unwrap();
        assert_eq!(program(&gif.command).as_deref(), Some("/opt/it's here/queijo"), "the quoting survives xfconf");
        assert!(parse(&listing).iter().any(|s| s.accel == "<Super>e" && s.command == "thunar"));
        assert_eq!(conflict_in(&x, "<Super>e").map(|c| c.1), Some(false));

        apply_in(&x, "/opt/it's here/queijo", &bindings(&[], &[])).unwrap();
        assert!(current_in(&x).iter().all(|(_, keys)| keys.is_empty()));
        assert!(parse(&x.list().unwrap()).iter().any(|s| s.accel == "<Super>e"));
        let _ = child.kill();
        let _ = child.wait();
    }
}
