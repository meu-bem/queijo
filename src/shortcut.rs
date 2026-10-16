//! Queijo's global shortcuts, which are GNOME custom keyboard shortcuts.

use std::process::Command;

use crate::i18n::t;

const LIST_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
const LIST_KEY: &str = "custom-keybindings";
const ITEM_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const ITEM_PREFIX: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom";

/// GNOME's own screenshot shortcuts; a Queijo shortcut takes their key over.
const BUILTINS: &[(&str, &str, &str)] = &[
    ("org.gnome.shell.keybindings", "show-screenshot-ui", "GNOME screenshot tool"),
    ("org.gnome.shell.keybindings", "screenshot", "GNOME “Take a screenshot”"),
    ("org.gnome.shell.keybindings", "screenshot-window", "GNOME “Screenshot of a window”"),
    ("org.gnome.shell.keybindings", "show-screen-recording-ui", "GNOME screen recorder"),
];

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

    fn from_command(command: &str) -> Self {
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

struct Entry {
    path: String,
    command: String,
    binding: String,
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

/// Current bindings (GNOME accelerator strings) per action.
pub fn current() -> Vec<(Action, Vec<String>)> {
    current_in(&Gsettings::default())
}

fn current_in(store: &dyn Store) -> Vec<(Action, Vec<String>)> {
    let entries = entries(store);
    Action::ALL
        .into_iter()
        .map(|action| {
            let keys = entries
                .iter()
                .filter(|e| {
                    is_queijo(&e.command) && Action::from_command(&e.command) == action && !e.binding.is_empty()
                })
                .map(|e| e.binding.clone())
                .collect();
            (action, keys)
        })
        .collect()
}

/// The GNOME shortcut that opens Queijo, for display ("Print Screen").
pub fn find() -> Option<String> {
    let open = current().into_iter().find(|(a, _)| *a == Action::Open)?.1;
    open.iter().map(|b| pretty(b)).find(|b| b == "Print Screen").or_else(|| open.first().map(|b| pretty(b)))
}

/// Replaces all of Queijo's GNOME shortcuts with `bindings`. GNOME's own
/// screenshot shortcuts that use one of the same keys are switched off.
pub fn apply(bindings: &[(Action, Vec<String>)]) -> Result<(), String> {
    let entries = entries(&Gsettings::default());
    // Keep pointing at the installed binary the existing shortcuts use, so
    // running the settings from a development build doesn't redirect them.
    let installed = entries
        .iter()
        .filter(|e| is_queijo(&e.command))
        .filter_map(|e| program(&e.command))
        .map(|p| if p.contains('/') { p } else { format!("{}/.local/bin/{p}", crate::config::home().display()) })
        .find(|p| std::path::Path::new(p).is_file());
    let exe = match installed {
        Some(p) => p,
        None => std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned(),
    };
    apply_in(&Gsettings::default(), &exe, bindings)
}

fn apply_in(store: &dyn Store, exe: &str, bindings: &[(Action, Vec<String>)]) -> Result<(), String> {
    let paths = list_paths(store);
    let entries = entries(store);
    let (ours, others): (Vec<&Entry>, Vec<&Entry>) = entries.iter().partition(|e| is_queijo(&e.command));

    let mut free: Vec<String> = ours.iter().map(|e| e.path.clone()).collect();
    let mut next = 0;
    // Everyone else's shortcuts stay exactly as they are.
    let mut new_list: Vec<String> = others.iter().map(|e| e.path.clone()).collect();
    for e in &ours {
        store.reset(&format!("{ITEM_SCHEMA}:{}", e.path));
    }

    for (action, keys) in bindings {
        for key in keys.iter().filter(|k| !k.is_empty()) {
            let path = free.pop().unwrap_or_else(|| loop {
                let candidate = format!("{ITEM_PREFIX}{next}/");
                next += 1;
                if !paths.contains(&candidate) && !new_list.contains(&candidate) {
                    break candidate;
                }
            });
            let schema = format!("{ITEM_SCHEMA}:{path}");
            let command = format!("{} {}", shell_quote(exe), action.args()).trim().to_string();
            store.set(&schema, "name", &quote(&format!("Queijo — {}", action.title())))?;
            store.set(&schema, "command", &quote(&command))?;
            store.set(&schema, "binding", &quote(key))?;
            new_list.push(path);
            free_builtin(store, key)?;
        }
    }
    store.set(LIST_SCHEMA, LIST_KEY, &gvariant_list(&new_list))
}

/// Who else uses this key: (description, will Queijo take it over automatically).
pub fn conflict(binding: &str) -> Option<(String, bool)> {
    conflict_in(&Gsettings::default(), binding)
}

fn conflict_in(store: &dyn Store, binding: &str) -> Option<(String, bool)> {
    let want = normalize(binding);
    for (schema, key, what) in BUILTINS {
        if parse_list(&store.get(schema, key).unwrap_or_default()).iter().any(|b| normalize(b) == want) {
            return Some((what.to_string(), true));
        }
    }
    entries(store)
        .into_iter()
        .filter(|e| !is_queijo(&e.command))
        .find(|e| normalize(&e.binding) == want)
        .map(|e| (format!("custom shortcut “{}”", get(store, &format!("{ITEM_SCHEMA}:{}", e.path), "name")), false))
}

fn free_builtin(store: &dyn Store, binding: &str) -> Result<(), String> {
    let want = normalize(binding);
    for (schema, key, _) in BUILTINS {
        let list = parse_list(&store.get(schema, key).unwrap_or_default());
        if list.iter().any(|b| normalize(b) == want) {
            let kept: Vec<String> = list.into_iter().filter(|b| normalize(b) != want).collect();
            store.set(schema, key, &gvariant_list(&kept))?;
        }
    }
    Ok(())
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

fn entries(store: &dyn Store) -> Vec<Entry> {
    list_paths(store)
        .into_iter()
        .map(|path| {
            let schema = format!("{ITEM_SCHEMA}:{path}");
            Entry { command: get(store, &schema, "command"), binding: get(store, &schema, "binding"), path }
        })
        .collect()
}

fn list_paths(store: &dyn Store) -> Vec<String> {
    parse_list(&store.get(LIST_SCHEMA, LIST_KEY).unwrap_or_default())
}

fn get(store: &dyn Store, schema: &str, key: &str) -> String {
    unquote(&store.get(schema, key).unwrap_or_default())
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
fn parse_list(raw: &str) -> Vec<String> {
    raw.split('\'').skip(1).step_by(2).map(str::to_string).collect()
}

fn gvariant_list(items: &[String]) -> String {
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
mod store_tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// GNOME's settings as a map, with a typical desktop already in it.
    struct Fake(RefCell<HashMap<(String, String), String>>);

    impl Store for Fake {
        fn get(&self, schema: &str, key: &str) -> Option<String> {
            self.0.borrow().get(&(schema.to_string(), key.to_string())).cloned()
        }

        fn set(&self, schema: &str, key: &str, value: &str) -> Result<(), String> {
            self.0.borrow_mut().insert((schema.to_string(), key.to_string()), value.to_string());
            Ok(())
        }

        fn reset(&self, schema: &str) {
            self.0.borrow_mut().retain(|(s, _), _| s != schema);
        }
    }

    const P0: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/";
    const P1: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom1/";

    fn item(path: &str) -> String {
        format!("{ITEM_SCHEMA}:{path}")
    }

    fn desktop() -> Fake {
        let fake = Fake(RefCell::new(HashMap::new()));
        for (path, name, command, binding) in
            [(P0, "Text editor", "gedit --new-window", "<Shift><Alt>s"), (P1, "Explorer", "nautilus", "<Super>e")]
        {
            fake.set(&item(path), "name", &quote(name)).unwrap();
            fake.set(&item(path), "command", &quote(command)).unwrap();
            fake.set(&item(path), "binding", &quote(binding)).unwrap();
        }
        fake.set(LIST_SCHEMA, LIST_KEY, &gvariant_list(&[P0.into(), P1.into()])).unwrap();
        fake.set("org.gnome.shell.keybindings", "show-screenshot-ui", "['Print']").unwrap();
        fake.set("org.gnome.shell.keybindings", "screenshot", "['<Shift>Print']").unwrap();
        fake
    }

    fn bindings(open: &[&str], gif: &[&str]) -> Vec<(Action, Vec<String>)> {
        let own = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        vec![(Action::Open, own(open)), (Action::Gif, own(gif)), (Action::Video, vec![]), (Action::FullScreen, vec![])]
    }

    fn keys_of(current: &[(Action, Vec<String>)], action: Action) -> Vec<String> {
        current.iter().find(|(a, _)| *a == action).unwrap().1.clone()
    }

    #[test]
    fn applying_adds_our_shortcuts_and_frees_gnomes_key() {
        let store = desktop();
        apply_in(&store, "/usr/bin/queijo", &bindings(&["Print", "<Primary><Shift>p"], &["<Primary><Alt>g"])).unwrap();

        let current = current_in(&store);
        assert_eq!(keys_of(&current, Action::Open).len(), 2);
        assert!(keys_of(&current, Action::Open).contains(&"Print".to_string()));
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        assert!(keys_of(&current, Action::Video).is_empty());

        // Other people's shortcuts are untouched, and GNOME's own Print is switched off.
        let entries = entries(&store);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].command, "gedit --new-window");
        assert_eq!(entries[1].binding, "<Super>e");
        assert_eq!(store.get("org.gnome.shell.keybindings", "show-screenshot-ui").unwrap(), "@as []");
        assert_eq!(store.get("org.gnome.shell.keybindings", "screenshot").unwrap(), "['<Shift>Print']");

        let gif = entries.iter().find(|e| e.command.contains("--kind gif")).unwrap();
        assert_eq!(gif.command, "/usr/bin/queijo --kind gif");
        let name = get(&store, &item(&gif.path), "name");
        assert!(name.starts_with("Queijo — "), "{name}");
    }

    #[test]
    fn reapplying_replaces_instead_of_piling_up() {
        let store = desktop();
        apply_in(&store, "/usr/bin/queijo", &bindings(&["Print", "<Primary><Shift>p"], &["<Primary><Alt>g"])).unwrap();
        apply_in(&store, "/usr/bin/queijo", &bindings(&["Print"], &[])).unwrap();

        let current = current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert!(keys_of(&current, Action::Gif).is_empty());
        assert_eq!(list_paths(&store).len(), 3, "two of theirs and one of ours");
        // The removed entries leave nothing behind.
        let leftovers = store.0.borrow().keys().filter(|(s, _)| s.starts_with(ITEM_SCHEMA)).count();
        assert_eq!(leftovers, 3 * 3, "name, command and binding for each of the 3 entries");

        apply_in(&store, "/usr/bin/queijo", &bindings(&[], &[])).unwrap();
        assert_eq!(list_paths(&store), vec![P0, P1]);
    }

    #[test]
    fn reports_who_else_uses_a_key() {
        let store = desktop();
        assert_eq!(conflict_in(&store, "<Shift>Print"), Some(("GNOME “Take a screenshot”".to_string(), true)));
        assert_eq!(conflict_in(&store, "<Super>e"), Some(("custom shortcut “Explorer”".to_string(), false)));
        assert_eq!(conflict_in(&store, "<Control><shift>P"), None);
        // Notation doesn't matter, and our own entries are never a conflict.
        assert_eq!(conflict_in(&store, "<Alt><Shift>S").map(|c| c.1), Some(false));
        apply_in(&store, "/usr/bin/queijo", &bindings(&["<Primary><Shift>p"], &[])).unwrap();
        assert_eq!(conflict_in(&store, "<Primary><Shift>p"), None);
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

    /// The same flow against the real `gsettings` command, writing to a throwaway
    /// keyfile instead of the user's settings. Skipped where GNOME's schemas
    /// aren't installed (set QUEIJO_REQUIRE_GSETTINGS=1 to make that an error).
    #[test]
    fn works_with_the_real_gsettings_command() {
        let dir = std::env::temp_dir().join(format!("queijo-gsettings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Gsettings {
            env: vec![
                ("GSETTINGS_BACKEND".into(), "keyfile".into()),
                ("XDG_CONFIG_HOME".into(), dir.to_string_lossy().into_owned()),
            ],
        };
        let has_schemas = store.run(&["list-keys", LIST_SCHEMA]).is_ok()
            && store.run(&["list-keys", "org.gnome.shell.keybindings"]).is_ok();
        if !has_schemas {
            assert!(std::env::var_os("QUEIJO_REQUIRE_GSETTINGS").is_none(), "GNOME schemas are required here");
            eprintln!("skipping: gsettings or the GNOME schemas are not available");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        store.set("org.gnome.shell.keybindings", "show-screenshot-ui", "['Print']").unwrap();
        apply_in(&store, "/opt/it's here/queijo", &bindings(&["Print"], &["<Primary><Alt>g"])).unwrap();

        let current = current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        let entries = entries(&store);
        let gif = entries.iter().find(|e| e.command.ends_with("--kind gif")).expect("the GIF shortcut is ours");
        assert_eq!(program(&gif.command).as_deref(), Some("/opt/it's here/queijo"));
        assert_eq!(store.get("org.gnome.shell.keybindings", "show-screenshot-ui").unwrap(), "@as []");

        apply_in(&store, "/opt/it's here/queijo", &bindings(&[], &[])).unwrap();
        assert!(list_paths(&store).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
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
}
