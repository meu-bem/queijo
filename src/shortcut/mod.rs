//! Queijo's global shortcuts: the custom keyboard shortcuts of the desktop, each of which
//! keeps them its own way. Keys are written GTK-style (`<Primary><Shift>Print`) everywhere here.

mod lists;

use std::process::Command;

use crate::desktop::{Desktop, Env};
use crate::i18n::t;

/// What a shortcut launches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Open,
    Video,
    Gif,
    FullScreen,
}

impl Action {
    pub const ALL: [Action; 4] = [Action::Open, Action::Video, Action::Gif, Action::FullScreen];

    pub fn args(self) -> &'static str {
        match self {
            Action::Open => "",
            Action::Video => "--kind video",
            Action::Gif => "--kind gif",
            Action::FullScreen => "--quick",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Action::Open => t("action.open_title"),
            Action::Video => t("action.video_title"),
            Action::Gif => t("action.gif_title"),
            Action::FullScreen => t("action.fullscreen_title"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::Open => t("action.open_desc"),
            Action::Video => t("action.video_desc"),
            Action::Gif => t("action.gif_desc"),
            Action::FullScreen => t("action.fullscreen_desc"),
        }
    }

    pub(super) fn from_command(command: &str) -> Self {
        if command.contains("--quick") {
            Action::FullScreen
        } else if command.contains("--kind gif") {
            Action::Gif
        } else if command.contains("--kind video") {
            Action::Video
        } else {
            Action::Open
        }
    }
}

/// Where the shortcuts live: GNOME's settings, or a map in tests.
/// Values are GVariant text, as `gsettings` prints and accepts them.
pub trait Store {
    fn get(&self, schema: &str, key: &str) -> Option<String>;
    fn set(&self, schema: &str, key: &str, value: &str) -> Result<(), String>;
    fn reset(&self, schema: &str);
}

/// The real thing: the `gsettings` command, optionally with extra environment
/// (tests point it at a throwaway keyfile backend instead of the user's GNOME).
#[derive(Default)]
struct Gsettings {
    env: Vec<(String, String)>,
}

impl Gsettings {
    #[cfg(test)]
    fn with_env(env: Vec<(String, String)>) -> Self {
        Gsettings { env }
    }

    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out =
            Command::new("gsettings").args(args).envs(self.env.iter().cloned()).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

impl Store for Gsettings {
    fn get(&self, schema: &str, key: &str) -> Option<String> {
        self.run(&["get", schema, key]).ok()
    }

    fn set(&self, schema: &str, key: &str, value: &str) -> Result<(), String> {
        self.run(&["set", schema, key, value]).map(|_| ())
    }

    fn reset(&self, schema: &str) {
        let _ = self.run(&["reset-recursively", schema]);
    }
}

/// How this desktop keeps custom shortcuts, and so what Queijo can do about them.
#[derive(Clone, Copy)]
pub enum Mechanism {
    /// In GSettings: GNOME, Cinnamon, MATE.
    Lists(&'static lists::Layout),
    /// Nothing Queijo knows how to write to.
    Unsupported,
}

pub fn mechanism(env: &Env) -> Mechanism {
    match env.desktop {
        Desktop::Gnome => Mechanism::Lists(&lists::GNOME),
        Desktop::Cinnamon => Mechanism::Lists(&lists::CINNAMON),
        Desktop::Mate => Mechanism::Lists(&lists::MATE),
        _ => Mechanism::Unsupported,
    }
}

impl Mechanism {
    /// Where the shortcuts are kept, for the doctor and the settings.
    pub fn name(self) -> &'static str {
        match self {
            Mechanism::Lists(layout) => layout.name,
            Mechanism::Unsupported => "none Queijo can write to",
        }
    }
}

fn mechanism_here() -> Mechanism {
    mechanism(Env::current())
}

/// Current bindings (accelerator strings) per action.
pub fn current() -> Vec<(Action, Vec<String>)> {
    match mechanism_here() {
        Mechanism::Lists(layout) => layout.current_in(&Gsettings::default()),
        Mechanism::Unsupported => Action::ALL.into_iter().map(|a| (a, Vec::new())).collect(),
    }
}

/// The shortcut that opens Queijo, for display ("Print Screen").
pub fn find() -> Option<String> {
    let open = current().into_iter().find(|(a, _)| *a == Action::Open)?.1;
    open.iter().map(|b| pretty(b)).find(|b| b == "Print Screen").or_else(|| open.first().map(|b| pretty(b)))
}

/// Replaces all of Queijo's shortcuts with `bindings`. The desktop's own screenshot
/// shortcuts that use one of the same keys are switched off.
pub fn apply(bindings: &[(Action, Vec<String>)]) -> Result<(), String> {
    match mechanism_here() {
        Mechanism::Lists(layout) => {
            let store = Gsettings::default();
            // Keep pointing at the installed binary the existing shortcuts use, so
            // running the settings from a development build doesn't redirect them.
            let exe = match layout.installed_program(&store) {
                Some(p) => p,
                None => std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned(),
            };
            layout.apply_in(&store, &exe, bindings)
        }
        Mechanism::Unsupported => Err("this desktop has no shortcut settings Queijo can write to".into()),
    }
}

/// Who else uses this key: (description, will Queijo take it over automatically).
pub fn conflict(binding: &str) -> Option<(String, bool)> {
    match mechanism_here() {
        Mechanism::Lists(layout) => layout.conflict_in(&Gsettings::default(), binding),
        Mechanism::Unsupported => None,
    }
}

fn is_queijo(command: &str) -> bool {
    program(command).is_some_and(|program| program.ends_with("queijo"))
}

/// The program a shortcut's command runs: its first word, shell quoting honoured.
fn program(command: &str) -> Option<String> {
    let mut word = String::new();
    let mut quote = None;
    let mut chars = command.trim_start().chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, c) if c.is_whitespace() => break,
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None | Some('"'), '\\') => word.extend(chars.next()),
            (_, c) => word.push(c),
        }
    }
    (!word.is_empty()).then_some(word)
}

/// A path as one shell word, for the command GNOME runs.
fn shell_quote(path: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "_-./+@%:=,".contains(c);
    if !path.is_empty() && path.chars().all(plain) {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

/// A GVariant string literal: `it's` → `'it\'s'`.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The inverse of [`quote`]; also reads the double-quoted form GVariant prints
/// for strings that contain a single quote.
fn unquote(raw: &str) -> String {
    let raw = raw.trim();
    let inner = match raw.chars().next() {
        Some(q @ ('\'' | '"')) if raw.len() >= 2 && raw.ends_with(q) => &raw[1..raw.len() - 1],
        _ => return raw.to_string(),
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some('n')) => {
                out.push('\n');
                chars.next();
            }
            ('\\', Some(next @ ('\\' | '\'' | '"'))) => {
                out.push(next);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// `['a', 'b']` / `@as []` → `["a", "b"]`.
pub(crate) fn parse_list(raw: &str) -> Vec<String> {
    raw.split('\'').skip(1).step_by(2).map(str::to_string).collect()
}

pub(crate) fn gvariant_list(items: &[String]) -> String {
    if items.is_empty() {
        "@as []".into()
    } else {
        format!("[{}]", items.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", "))
    }
}

/// Comparable form: `<Control><shift>P` and `<Primary><Shift>p` are the same key.
fn normalize(binding: &str) -> String {
    let (mods, key) = split(binding);
    let mut mods: Vec<String> = mods
        .into_iter()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" => "ctrl".into(),
            other => other.to_string(),
        })
        .collect();
    mods.sort();
    mods.dedup();
    format!("{}+{}", mods.join("+"), key.to_ascii_lowercase())
}

fn split(binding: &str) -> (Vec<String>, String) {
    let mut mods = Vec::new();
    let mut rest = binding.trim();
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else { break };
        mods.push(rest[start + 1..start + end].to_string());
        rest = &rest[start + end + 1..];
    }
    (mods, rest.to_string())
}

/// `<Primary><Shift>p` → `Ctrl+Shift+P`, `Print` → `Print Screen`.
pub fn pretty(binding: &str) -> String {
    let (mods, key) = split(binding);
    let mut parts: Vec<String> = mods
        .iter()
        .map(|m| match m.to_ascii_lowercase().as_str() {
            "primary" | "control" | "ctrl" => "Ctrl".to_string(),
            "super" => "Super".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            other => other.to_string(),
        })
        .collect();
    parts.push(match key.as_str() {
        "Print" => "Print Screen".to_string(),
        "Page_Up" => "Page Up".to_string(),
        "Page_Down" => "Page Down".to_string(),
        "Return" => "Enter".to_string(),
        "space" => "Space".to_string(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_string(),
    });
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bindings() {
        assert_eq!(pretty("Print"), "Print Screen");
        assert_eq!(pretty("<Primary><Shift>p"), "Ctrl+Shift+P");
        assert_eq!(pretty("<Super>F12"), "Super+F12");
    }

    #[test]
    fn compares_bindings() {
        assert_eq!(normalize("<Control><shift>P"), normalize("<Primary><Shift>p"));
        assert_ne!(normalize("Print"), normalize("<Shift>Print"));
        assert_eq!(parse_list("['a', 'b']"), vec!["a", "b"]);
        assert!(parse_list("@as []").is_empty());
        assert_eq!(gvariant_list(&[]), "@as []");
    }

    #[test]
    fn recognizes_actions() {
        assert_eq!(Action::from_command("/x/queijo --kind gif"), Action::Gif);
        assert_eq!(Action::from_command("queijo"), Action::Open);
        assert!(is_queijo("/home/u/.local/bin/queijo --quick"));
        assert!(!is_queijo("gedit --new-window"));
    }

    #[test]
    fn commands_survive_paths_with_spaces_and_quotes() {
        for exe in ["/usr/bin/queijo", "/home/Maria Silva/.local/bin/queijo", "/opt/it's here/queijo", "queijo"] {
            let command = format!("{} --kind gif", shell_quote(exe));
            assert_eq!(program(&command).as_deref(), Some(exe), "{command}");
            assert!(is_queijo(&command), "{command}");
        }
        assert_eq!(shell_quote("/usr/bin/queijo"), "/usr/bin/queijo");
        assert_eq!(shell_quote("/a b/queijo"), "'/a b/queijo'");
        assert_eq!(program("  gedit --new-window").as_deref(), Some("gedit"));
        assert_eq!(program("\"/x y/queijo\" --quick").as_deref(), Some("/x y/queijo"));
        assert_eq!(program(""), None);
        assert!(!is_queijo("'/home/queijo fan/bin/gedit' --new-window"));
    }

    #[test]
    fn strings_survive_quoting() {
        for text in ["plain", "it's", r"back\slash", "Enregistrer l'écran", "a \"quoted\" word", "two\nlines"] {
            assert_eq!(unquote(&quote(text)), text);
        }
        // GVariant prints strings that contain an apostrophe with double quotes.
        assert_eq!(unquote("\"it's\""), "it's");
        assert_eq!(unquote("'x'"), "x");
        assert_eq!(unquote("  'padded'  "), "padded");
        assert_eq!(unquote("unquoted"), "unquoted");
        assert_eq!(unquote(""), "");
    }

    #[test]
    fn each_desktop_has_its_own_way() {
        let env = |desktop| Env { session: crate::desktop::Session::X11, desktop, compositing: true };
        assert!(matches!(mechanism(&env(Desktop::Gnome)), Mechanism::Lists(l) if l.name.starts_with("GNOME")));
        assert!(matches!(mechanism(&env(Desktop::Cinnamon)), Mechanism::Lists(l) if l.name.starts_with("Cinnamon")));
        assert!(matches!(mechanism(&env(Desktop::Mate)), Mechanism::Lists(l) if l.name.starts_with("MATE")));
    }
}
