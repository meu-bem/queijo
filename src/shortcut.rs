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

/// Current bindings (GNOME accelerator strings) per action.
pub fn current() -> Vec<(Action, Vec<String>)> {
    let entries = entries();
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
    let paths = list_paths();
    let entries = entries();
    let (ours, others): (Vec<&Entry>, Vec<&Entry>) = entries.iter().partition(|e| is_queijo(&e.command));
    // Keep pointing at the installed binary the existing shortcuts use, so
    // running the settings from a development build doesn't redirect them.
    let installed = ours
        .iter()
        .filter_map(|e| e.command.split_whitespace().next())
        .map(|p| {
            if p.contains('/') {
                p.to_string()
            } else {
                format!("{}/.local/bin/{p}", crate::config::home().display())
            }
        })
        .find(|p| std::path::Path::new(p).is_file());
    let exe = match installed {
        Some(p) => p,
        None => std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned(),
    };

    let mut free: Vec<String> = ours.iter().map(|e| e.path.clone()).collect();
    let mut next = 0;
    // Everyone else's shortcuts stay exactly as they are.
    let mut new_list: Vec<String> = others.iter().map(|e| e.path.clone()).collect();
    for e in &ours {
        gsettings_reset(&e.path);
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
            let command = format!("{exe} {}", action.args()).trim().to_string();
            gsettings(&["set", &schema, "name", &format!("Queijo — {}", action.title())])?;
            gsettings(&["set", &schema, "command", &command])?;
            gsettings(&["set", &schema, "binding", key])?;
            new_list.push(path);
            free_builtin(key)?;
        }
    }
    gsettings(&["set", LIST_SCHEMA, LIST_KEY, &gvariant_list(&new_list)])?;
    Ok(())
}

/// Who else uses this key: (description, will Queijo take it over automatically).
pub fn conflict(binding: &str) -> Option<(String, bool)> {
    let want = normalize(binding);
    for (schema, key, what) in BUILTINS {
        if parse_list(&gsettings(&["get", schema, key]).unwrap_or_default()).iter().any(|b| normalize(b) == want) {
            return Some((what.to_string(), true));
        }
    }
    for path in list_paths() {
        let schema = format!("{ITEM_SCHEMA}:{path}");
        let command = get(&schema, "command");
        if is_queijo(&command) {
            continue;
        }
        if normalize(&get(&schema, "binding")) == want {
            return Some((format!("custom shortcut “{}”", get(&schema, "name")), false));
        }
    }
    None
}

fn free_builtin(binding: &str) -> Result<(), String> {
    let want = normalize(binding);
    for (schema, key, _) in BUILTINS {
        let list = parse_list(&gsettings(&["get", schema, key]).unwrap_or_default());
        if list.iter().any(|b| normalize(b) == want) {
            let kept: Vec<String> = list.into_iter().filter(|b| normalize(b) != want).collect();
            gsettings(&["set", schema, key, &gvariant_list(&kept)])?;
        }
    }
    Ok(())
}

fn is_queijo(command: &str) -> bool {
    command.split_whitespace().next().is_some_and(|program| program.ends_with("queijo"))
}

fn entries() -> Vec<Entry> {
    list_paths()
        .into_iter()
        .map(|path| {
            let schema = format!("{ITEM_SCHEMA}:{path}");
            Entry { command: get(&schema, "command"), binding: get(&schema, "binding"), path }
        })
        .collect()
}

fn list_paths() -> Vec<String> {
    parse_list(&gsettings(&["get", LIST_SCHEMA, LIST_KEY]).unwrap_or_default())
}

fn get(schema: &str, key: &str) -> String {
    let raw = gsettings(&["get", schema, key]).unwrap_or_default();
    raw.trim().trim_matches('\'').to_string()
}

fn gsettings_reset(path: &str) {
    let _ = Command::new("gsettings").args(["reset-recursively", &format!("{ITEM_SCHEMA}:{path}")]).status();
}

fn gsettings(args: &[&str]) -> Result<String, String> {
    let out = Command::new("gsettings").args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
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
