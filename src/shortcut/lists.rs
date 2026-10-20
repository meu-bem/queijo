//! Custom keyboard shortcuts that a desktop keeps in GSettings: GNOME, Cinnamon and MATE
//! all do, with the same shape (a name, a command, a key) and different names for it.

use super::{gvariant_list, is_queijo, normalize, parse_list, program, quote, shell_quote, unquote, Action, Store};

/// A desktop's own screenshot shortcut, which one of ours takes the key of.
pub struct Builtin {
    pub schema: &'static str,
    pub key: &'static str,
    pub label: &'static str,
}

/// How a desktop keeps its custom shortcuts.
pub struct Layout {
    pub name: &'static str,
    /// The setting that lists the shortcuts (schema, key); `None` where there is no list
    /// and they are found by trying `custom0`, `custom1`, ….
    pub list: Option<(&'static str, &'static str)>,
    /// The list holds whole paths (GNOME), or only the `customN` names (Cinnamon).
    pub list_has_paths: bool,
    pub item_schema: &'static str,
    /// Where the shortcuts' settings live; a shortcut `customN` is `item_prefix + "customN/"`.
    pub item_prefix: &'static str,
    /// What the setting holding the command is called.
    pub command_key: &'static str,
    /// A shortcut's `binding` is a list of keys (Cinnamon) and not one key.
    pub binding_is_list: bool,
    /// The desktop's own screenshot shortcuts hold a list of keys (GNOME, Cinnamon), or one key (MATE).
    pub builtins_are_lists: bool,
    pub builtins: &'static [Builtin],
}

pub const GNOME: Layout = Layout {
    name: "GNOME keyboard settings (gsettings)",
    list: Some(("org.gnome.settings-daemon.plugins.media-keys", "custom-keybindings")),
    list_has_paths: true,
    item_schema: "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding",
    item_prefix: "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/",
    command_key: "command",
    binding_is_list: false,
    builtins_are_lists: true,
    builtins: &[
        Builtin { schema: "org.gnome.shell.keybindings", key: "show-screenshot-ui", label: "GNOME screenshot tool" },
        Builtin { schema: "org.gnome.shell.keybindings", key: "screenshot", label: "GNOME “Take a screenshot”" },
        Builtin {
            schema: "org.gnome.shell.keybindings",
            key: "screenshot-window",
            label: "GNOME “Screenshot of a window”",
        },
        Builtin {
            schema: "org.gnome.shell.keybindings",
            key: "show-screen-recording-ui",
            label: "GNOME screen recorder",
        },
    ],
};

/// Written from how Cinnamon keeps them; never run against a Cinnamon.
pub const CINNAMON: Layout = Layout {
    name: "Cinnamon keyboard settings (gsettings)",
    list: Some(("org.cinnamon.desktop.keybindings", "custom-list")),
    list_has_paths: false,
    item_schema: "org.cinnamon.desktop.keybindings.custom-keybinding",
    item_prefix: "/org/cinnamon/desktop/keybindings/custom-keybindings/",
    command_key: "command",
    binding_is_list: true,
    builtins_are_lists: true,
    builtins: &[
        Builtin {
            schema: "org.cinnamon.desktop.keybindings.media-keys",
            key: "screenshot",
            label: "Cinnamon screenshot",
        },
        Builtin {
            schema: "org.cinnamon.desktop.keybindings.media-keys",
            key: "area-screenshot",
            label: "Cinnamon “Screenshot of an area”",
        },
        Builtin {
            schema: "org.cinnamon.desktop.keybindings.media-keys",
            key: "window-screenshot",
            label: "Cinnamon “Screenshot of a window”",
        },
        Builtin {
            schema: "org.cinnamon.desktop.keybindings.media-keys",
            key: "screenshot-clip",
            label: "Cinnamon “Screenshot to the clipboard”",
        },
    ],
};

/// Written from how MATE keeps them; never run against a MATE.
pub const MATE: Layout = Layout {
    name: "MATE keyboard settings (gsettings)",
    list: None,
    list_has_paths: true,
    item_schema: "org.mate.control-center.keybinding",
    item_prefix: "/org/mate/desktop/keybindings/",
    command_key: "action",
    binding_is_list: false,
    builtins_are_lists: false,
    builtins: &[
        Builtin { schema: "org.mate.SettingsDaemon.plugins.media-keys", key: "screenshot", label: "MATE screenshot" },
        Builtin {
            schema: "org.mate.SettingsDaemon.plugins.media-keys",
            key: "window-screenshot",
            label: "MATE “Screenshot of a window”",
        },
    ],
};

/// Without a list, this many `customN` are tried.
const PROBED: usize = 24;

struct Entry {
    path: String,
    command: String,
    bindings: Vec<String>,
}

impl Layout {
    fn schema_of(&self, path: &str) -> String {
        format!("{}:{path}", self.item_schema)
    }

    fn path_named(&self, name: &str) -> String {
        format!("{}{name}/", self.item_prefix)
    }

    /// What goes in the list for the shortcut at `path`.
    fn list_item(&self, path: &str) -> String {
        if self.list_has_paths {
            path.to_string()
        } else {
            path.trim_start_matches(self.item_prefix).trim_end_matches('/').to_string()
        }
    }

    /// The shortcuts that exist: those in the list, or those of `custom0`, … that have a command.
    fn paths(&self, store: &dyn Store) -> Vec<String> {
        match self.list {
            Some((schema, key)) => parse_list(&store.get(schema, key).unwrap_or_default())
                .into_iter()
                .map(|item| if self.list_has_paths { item } else { self.path_named(&item) })
                .collect(),
            None => (0..PROBED)
                .map(|n| self.path_named(&format!("custom{n}")))
                .filter(|path| !self.get(store, &self.schema_of(path), self.command_key).is_empty())
                .collect(),
        }
    }

    fn get(&self, store: &dyn Store, schema: &str, key: &str) -> String {
        unquote(&store.get(schema, key).unwrap_or_default())
    }

    fn bindings_of(&self, store: &dyn Store, path: &str) -> Vec<String> {
        let raw = store.get(&self.schema_of(path), "binding").unwrap_or_default();
        let keys = if self.binding_is_list { parse_list(&raw) } else { vec![unquote(&raw)] };
        keys.into_iter().filter(|k| !k.is_empty()).collect()
    }

    fn entries(&self, store: &dyn Store) -> Vec<Entry> {
        self.paths(store)
            .into_iter()
            .map(|path| Entry {
                command: self.get(store, &self.schema_of(&path), self.command_key),
                bindings: self.bindings_of(store, &path),
                path,
            })
            .collect()
    }

    /// The keys one of the desktop's own screenshot shortcuts has.
    fn builtin_keys(&self, store: &dyn Store, b: &Builtin) -> Vec<String> {
        let raw = store.get(b.schema, b.key).unwrap_or_default();
        if self.builtins_are_lists {
            parse_list(&raw)
        } else {
            vec![unquote(&raw)]
        }
    }

    /// Current keys (accelerator strings) per action.
    pub fn current_in(&self, store: &dyn Store) -> Vec<(Action, Vec<String>)> {
        let entries = self.entries(store);
        Action::ALL
            .into_iter()
            .map(|action| {
                let keys = entries
                    .iter()
                    .filter(|e| is_queijo(&e.command) && Action::from_command(&e.command) == action)
                    .flat_map(|e| e.bindings.clone())
                    .collect();
                (action, keys)
            })
            .collect()
    }

    /// The program path in our existing shortcuts, if there are any and it is a file.
    pub fn installed_program(&self, store: &dyn Store) -> Option<String> {
        self.entries(store)
            .iter()
            .filter(|e| is_queijo(&e.command))
            .filter_map(|e| program(&e.command))
            .map(|p| if p.contains('/') { p } else { format!("{}/.local/bin/{p}", crate::config::home().display()) })
            .find(|p| std::path::Path::new(p).is_file())
    }

    /// Replaces all of Queijo's shortcuts with `bindings`. The desktop's own screenshot
    /// shortcuts that use one of the same keys are switched off.
    pub fn apply_in(&self, store: &dyn Store, exe: &str, bindings: &[(Action, Vec<String>)]) -> Result<(), String> {
        let paths = self.paths(store);
        let entries = self.entries(store);
        let (ours, others): (Vec<&Entry>, Vec<&Entry>) = entries.iter().partition(|e| is_queijo(&e.command));

        let mut free: Vec<String> = ours.iter().map(|e| e.path.clone()).collect();
        let mut next = 0;
        // Everyone else's shortcuts stay exactly as they are.
        let mut new_list: Vec<String> = others.iter().map(|e| e.path.clone()).collect();
        for e in &ours {
            store.reset(&self.schema_of(&e.path));
        }

        for (action, keys) in bindings {
            for key in keys.iter().filter(|k| !k.is_empty()) {
                let path = free.pop().unwrap_or_else(|| loop {
                    let candidate = self.path_named(&format!("custom{next}"));
                    next += 1;
                    if !paths.contains(&candidate) && !new_list.contains(&candidate) {
                        break candidate;
                    }
                });
                let schema = self.schema_of(&path);
                let command = format!("{} {}", shell_quote(exe), action.args()).trim().to_string();
                store.set(&schema, "name", &quote(&format!("Queijo — {}", action.title())))?;
                store.set(&schema, self.command_key, &quote(&command))?;
                let binding = if self.binding_is_list { gvariant_list(std::slice::from_ref(key)) } else { quote(key) };
                store.set(&schema, "binding", &binding)?;
                new_list.push(path);
                self.free_builtin(store, key)?;
            }
        }
        match self.list {
            Some((schema, key)) => {
                let items: Vec<String> = new_list.iter().map(|p| self.list_item(p)).collect();
                store.set(schema, key, &gvariant_list(&items))
            }
            None => Ok(()),
        }
    }

    /// Who else uses this key: (description, will Queijo take it over automatically).
    pub fn conflict_in(&self, store: &dyn Store, binding: &str) -> Option<(String, bool)> {
        let want = normalize(binding);
        for b in self.builtins {
            if self.builtin_keys(store, b).iter().any(|k| normalize(k) == want) {
                return Some((b.label.to_string(), true));
            }
        }
        self.entries(store)
            .into_iter()
            .filter(|e| !is_queijo(&e.command))
            .find(|e| e.bindings.iter().any(|k| normalize(k) == want))
            .map(|e| {
                let name = self.get(store, &self.schema_of(&e.path), "name");
                (format!("custom shortcut “{name}”"), false)
            })
    }

    fn free_builtin(&self, store: &dyn Store, binding: &str) -> Result<(), String> {
        let want = normalize(binding);
        for b in self.builtins {
            let keys = self.builtin_keys(store, b);
            if !keys.iter().any(|k| normalize(k) == want) {
                continue;
            }
            if self.builtins_are_lists {
                let kept: Vec<String> = keys.into_iter().filter(|k| normalize(k) != want).collect();
                store.set(b.schema, b.key, &gvariant_list(&kept))?;
            } else {
                // A desktop with one key per shortcut says "disabled" for none.
                store.set(b.schema, b.key, &quote("disabled"))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// A desktop's settings as a map.
    pub struct Fake(RefCell<HashMap<(String, String), String>>);

    impl Fake {
        fn empty() -> Self {
            Fake(RefCell::new(HashMap::new()))
        }
    }

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
        format!("{}:{path}", GNOME.item_schema)
    }

    /// GNOME with a typical desktop already in it.
    fn desktop() -> Fake {
        let fake = Fake::empty();
        for (path, name, command, binding) in
            [(P0, "Text editor", "gedit --new-window", "<Shift><Alt>s"), (P1, "Explorer", "nautilus", "<Super>e")]
        {
            fake.set(&item(path), "name", &quote(name)).unwrap();
            fake.set(&item(path), "command", &quote(command)).unwrap();
            fake.set(&item(path), "binding", &quote(binding)).unwrap();
        }
        let (schema, key) = GNOME.list.unwrap();
        fake.set(schema, key, &gvariant_list(&[P0.into(), P1.into()])).unwrap();
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
        GNOME
            .apply_in(&store, "/usr/bin/queijo", &bindings(&["Print", "<Primary><Shift>p"], &["<Primary><Alt>g"]))
            .unwrap();

        let current = GNOME.current_in(&store);
        assert_eq!(keys_of(&current, Action::Open).len(), 2);
        assert!(keys_of(&current, Action::Open).contains(&"Print".to_string()));
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        assert!(keys_of(&current, Action::Video).is_empty());

        // Other people's shortcuts are untouched, and GNOME's own Print is switched off.
        let entries = GNOME.entries(&store);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].command, "gedit --new-window");
        assert_eq!(entries[1].bindings, vec!["<Super>e"]);
        assert_eq!(store.get("org.gnome.shell.keybindings", "show-screenshot-ui").unwrap(), "@as []");
        assert_eq!(store.get("org.gnome.shell.keybindings", "screenshot").unwrap(), "['<Shift>Print']");

        let gif = entries.iter().find(|e| e.command.contains("--kind gif")).unwrap();
        assert_eq!(gif.command, "/usr/bin/queijo --kind gif");
        let name = GNOME.get(&store, &item(&gif.path), "name");
        assert!(name.starts_with("Queijo — "), "{name}");
    }

    #[test]
    fn reapplying_replaces_instead_of_piling_up() {
        let store = desktop();
        GNOME
            .apply_in(&store, "/usr/bin/queijo", &bindings(&["Print", "<Primary><Shift>p"], &["<Primary><Alt>g"]))
            .unwrap();
        GNOME.apply_in(&store, "/usr/bin/queijo", &bindings(&["Print"], &[])).unwrap();

        let current = GNOME.current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert!(keys_of(&current, Action::Gif).is_empty());
        assert_eq!(GNOME.paths(&store).len(), 3, "two of theirs and one of ours");
        // The removed entries leave nothing behind.
        let leftovers = store.0.borrow().keys().filter(|(s, _)| s.starts_with(GNOME.item_schema)).count();
        assert_eq!(leftovers, 3 * 3, "name, command and binding for each of the 3 entries");

        GNOME.apply_in(&store, "/usr/bin/queijo", &bindings(&[], &[])).unwrap();
        assert_eq!(GNOME.paths(&store), vec![P0, P1]);
    }

    #[test]
    fn reports_who_else_uses_a_key() {
        let store = desktop();
        assert_eq!(GNOME.conflict_in(&store, "<Shift>Print"), Some(("GNOME “Take a screenshot”".to_string(), true)));
        assert_eq!(GNOME.conflict_in(&store, "<Super>e"), Some(("custom shortcut “Explorer”".to_string(), false)));
        assert_eq!(GNOME.conflict_in(&store, "<Control><shift>P"), None);
        // Notation doesn't matter, and our own entries are never a conflict.
        assert_eq!(GNOME.conflict_in(&store, "<Alt><Shift>S").map(|c| c.1), Some(false));
        GNOME.apply_in(&store, "/usr/bin/queijo", &bindings(&["<Primary><Shift>p"], &[])).unwrap();
        assert_eq!(GNOME.conflict_in(&store, "<Primary><Shift>p"), None);
    }

    const CINNAMON_ITEM: &str = "/org/cinnamon/desktop/keybindings/custom-keybindings/";

    /// Cinnamon, as its settings are laid out: a list of names, and each key in a list of its own.
    fn cinnamon() -> Fake {
        let fake = Fake::empty();
        let schema = format!("{}:{CINNAMON_ITEM}custom0/", CINNAMON.item_schema);
        fake.set(&schema, "name", &quote("Terminal")).unwrap();
        fake.set(&schema, "command", &quote("gnome-terminal")).unwrap();
        fake.set(&schema, "binding", "['<Primary><Alt>t', '<Super>t']").unwrap();
        fake.set("org.cinnamon.desktop.keybindings", "custom-list", "['custom0']").unwrap();
        fake.set("org.cinnamon.desktop.keybindings.media-keys", "screenshot", "['Print', '<Super>s']").unwrap();
        fake.set("org.cinnamon.desktop.keybindings.media-keys", "area-screenshot", "['<Shift>Print']").unwrap();
        fake
    }

    #[test]
    fn cinnamon_keeps_names_in_its_list_and_keys_in_lists() {
        let store = cinnamon();
        // Every key of a shortcut counts for conflicts, not just the first.
        assert_eq!(CINNAMON.conflict_in(&store, "<Super>t").map(|c| c.0), Some("custom shortcut “Terminal”".into()));
        assert_eq!(CINNAMON.conflict_in(&store, "<Super>s"), Some(("Cinnamon screenshot".into(), true)));

        CINNAMON.apply_in(&store, "/usr/bin/queijo", &bindings(&["Print"], &["<Primary><Alt>g"])).unwrap();
        // The list holds names, not paths, and ours got the next free ones.
        assert_eq!(
            store.get("org.cinnamon.desktop.keybindings", "custom-list").unwrap(),
            "['custom0', 'custom1', 'custom2']"
        );
        let first = format!("{}:{CINNAMON_ITEM}custom1/", CINNAMON.item_schema);
        assert!(store.get(&first, "binding").unwrap().starts_with("['"), "a key is written as a list");
        let current = CINNAMON.current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        // Print is taken from the screenshot shortcut, which keeps its other key.
        assert_eq!(store.get("org.cinnamon.desktop.keybindings.media-keys", "screenshot").unwrap(), "['<Super>s']");
        // Theirs is untouched.
        assert_eq!(CINNAMON.entries(&store)[0].bindings, vec!["<Primary><Alt>t", "<Super>t"]);

        CINNAMON.apply_in(&store, "/usr/bin/queijo", &bindings(&[], &[])).unwrap();
        assert_eq!(store.get("org.cinnamon.desktop.keybindings", "custom-list").unwrap(), "['custom0']");
    }

    const MATE_ITEM: &str = "/org/mate/desktop/keybindings/";

    /// MATE has no list: each shortcut is a `customN` that has an `action`.
    fn mate() -> Fake {
        let fake = Fake::empty();
        let schema = format!("{}:{MATE_ITEM}custom0/", MATE.item_schema);
        fake.set(&schema, "name", &quote("Terminal")).unwrap();
        fake.set(&schema, "action", &quote("mate-terminal")).unwrap();
        fake.set(&schema, "binding", &quote("<Mod4>t")).unwrap();
        // A hole at custom1 and another at custom3: they are found anyway.
        let other = format!("{}:{MATE_ITEM}custom2/", MATE.item_schema);
        fake.set(&other, "name", &quote("Files")).unwrap();
        fake.set(&other, "action", &quote("caja")).unwrap();
        fake.set(&other, "binding", &quote("<Mod4>e")).unwrap();
        fake.set("org.mate.SettingsDaemon.plugins.media-keys", "screenshot", "'Print'").unwrap();
        fake.set("org.mate.SettingsDaemon.plugins.media-keys", "window-screenshot", "'<Mod1>Print'").unwrap();
        fake
    }

    #[test]
    fn mate_finds_its_shortcuts_without_a_list_and_says_disabled() {
        let store = mate();
        assert_eq!(MATE.paths(&store).len(), 2);
        assert_eq!(MATE.conflict_in(&store, "Print"), Some(("MATE screenshot".into(), true)));
        assert_eq!(MATE.conflict_in(&store, "<Mod4>e").map(|c| c.1), Some(false));

        MATE.apply_in(&store, "/usr/bin/queijo", &bindings(&["Print"], &["<Primary><Alt>g"])).unwrap();
        let current = MATE.current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        // Ours took the free numbers (1 and 3) and not their custom0 or custom2.
        assert_eq!(MATE.paths(&store).len(), 4);
        assert_eq!(MATE.entries(&store)[0].command, "mate-terminal");
        assert_eq!(store.get("org.mate.SettingsDaemon.plugins.media-keys", "screenshot").unwrap(), "'disabled'");
        assert_eq!(
            store.get("org.mate.SettingsDaemon.plugins.media-keys", "window-screenshot").unwrap(),
            "'<Mod1>Print'"
        );

        // Removing ours leaves exactly theirs.
        MATE.apply_in(&store, "/usr/bin/queijo", &bindings(&[], &[])).unwrap();
        assert_eq!(MATE.paths(&store).len(), 2);
    }

    /// The same flow against the real `gsettings` command, writing to a throwaway
    /// keyfile instead of the user's settings. Skipped where GNOME's schemas
    /// aren't installed (set QUEIJO_REQUIRE_GSETTINGS=1 to make that an error).
    #[test]
    fn works_with_the_real_gsettings_command() {
        let dir = std::env::temp_dir().join(format!("queijo-gsettings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = super::super::Gsettings::with_env(vec![
            ("GSETTINGS_BACKEND".into(), "keyfile".into()),
            ("XDG_CONFIG_HOME".into(), dir.to_string_lossy().into_owned()),
        ]);
        let list_schema = GNOME.list.unwrap().0;
        let has_schemas = store.run(&["list-keys", list_schema]).is_ok()
            && store.run(&["list-keys", "org.gnome.shell.keybindings"]).is_ok();
        if !has_schemas {
            assert!(std::env::var_os("QUEIJO_REQUIRE_GSETTINGS").is_none(), "GNOME schemas are required here");
            eprintln!("skipping: gsettings or the GNOME schemas are not available");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        store.set("org.gnome.shell.keybindings", "show-screenshot-ui", "['Print']").unwrap();
        GNOME.apply_in(&store, "/opt/it's here/queijo", &bindings(&["Print"], &["<Primary><Alt>g"])).unwrap();

        let current = GNOME.current_in(&store);
        assert_eq!(keys_of(&current, Action::Open), vec!["Print"]);
        assert_eq!(keys_of(&current, Action::Gif), vec!["<Primary><Alt>g"]);
        let entries = GNOME.entries(&store);
        let gif = entries.iter().find(|e| e.command.ends_with("--kind gif")).expect("the GIF shortcut is ours");
        assert_eq!(program(&gif.command).as_deref(), Some("/opt/it's here/queijo"));
        assert_eq!(store.get("org.gnome.shell.keybindings", "show-screenshot-ui").unwrap(), "@as []");

        GNOME.apply_in(&store, "/opt/it's here/queijo", &bindings(&[], &[])).unwrap();
        assert!(GNOME.paths(&store).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
