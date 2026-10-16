//! Translations.
//!
//! Every user-visible string lives in a TOML catalog under `locales/`, embedded
//! in the binary. English is the reference: a language may leave keys out and
//! those fall back to English. See `docs/TRANSLATING.md` for how to add one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

pub struct Language {
    /// Value stored in the config and matched against the system locale (`pt-BR`).
    pub code: &'static str,
    /// Written in the language itself, so it is recognizable in any other.
    pub name: &'static str,
    source: &'static str,
}

/// Add new languages here (and a catalog next to the others).
pub const LANGUAGES: &[Language] = &[
    Language { code: "en", name: "English", source: include_str!("../locales/en.toml") },
    Language { code: "pt-BR", name: "Português (Brasil)", source: include_str!("../locales/pt-BR.toml") },
];

/// What the config holds when the user hasn't picked a language.
pub const AUTO: &str = "auto";

type Catalog = HashMap<&'static str, &'static str>;

static CATALOGS: OnceLock<Vec<Catalog>> = OnceLock::new();
static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn catalogs() -> &'static [Catalog] {
    CATALOGS.get_or_init(|| LANGUAGES.iter().map(|l| parse(l.code, l.source)).collect())
}

/// Nested tables become dotted keys: `[tool]` + `pen = "…"` is `tool.pen`.
pub fn flatten(source: &str) -> Result<Vec<(String, String)>, String> {
    fn walk(prefix: &str, table: &toml::Table, out: &mut Vec<(String, String)>) -> Result<(), String> {
        for (key, value) in table {
            let name = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
            match value {
                toml::Value::String(s) => out.push((name, s.clone())),
                toml::Value::Table(t) => walk(&name, t, out)?,
                _ => return Err(format!("`{name}` must be a string")),
            }
        }
        Ok(())
    }
    let table: toml::Table = source.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let mut out = Vec::new();
    walk("", &table, &mut out)?;
    Ok(out)
}

fn parse(code: &str, source: &str) -> Catalog {
    match flatten(source) {
        // The catalogs live for the whole run, so leaking them gives `&'static str` lookups.
        Ok(pairs) => {
            pairs.into_iter().map(|(k, v)| (&*Box::leak(k.into_boxed_str()), &*Box::leak(v.into_boxed_str()))).collect()
        }
        Err(e) => {
            eprintln!("queijo: broken {code} translation: {e}");
            Catalog::new()
        }
    }
}

/// Picks the interface language: a code from [`LANGUAGES`], or [`AUTO`] to follow the system.
pub fn set_language(setting: &str) {
    let index = if setting == AUTO { detect() } else { find(setting) }.unwrap_or(0);
    CURRENT.store(index, Ordering::Relaxed);
}

/// Index of the language the user's locale asks for, if we have it.
fn detect() -> Option<usize> {
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .flat_map(|value| value.split(':').map(str::to_string).collect::<Vec<_>>())
        .filter(|locale| !locale.is_empty() && locale != "C" && locale != "POSIX")
        .find_map(|locale| find(&locale))
}

/// `pt_BR.UTF-8` → the `pt-BR` language; `pt_PT` → the first `pt…` language; `en_GB` → `en`.
fn find(locale: &str) -> Option<usize> {
    let normalized = locale.split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    let language = normalized.split('-').next().unwrap_or("");
    LANGUAGES
        .iter()
        .position(|l| l.code.eq_ignore_ascii_case(&normalized))
        .or_else(|| LANGUAGES.iter().position(|l| l.code.to_ascii_lowercase().split('-').next() == Some(language)))
}

/// The text for `key` in the current language, else in English, else the key itself.
pub fn t(key: &'static str) -> &'static str {
    lookup(catalogs(), CURRENT.load(Ordering::Relaxed), key)
}

fn lookup(cats: &[Catalog], current: usize, key: &'static str) -> &'static str {
    cats[current].get(key).or_else(|| cats[0].get(key)).copied().unwrap_or(key)
}

/// Like [`t`], filling `{name}` placeholders.
pub fn tf(key: &'static str, args: &[(&str, &str)]) -> String {
    fill(t(key), args)
}

fn fill(text: &str, args: &[(&str, &str)]) -> String {
    args.iter().fold(text.to_string(), |text, (name, value)| text.replace(&format!("{{{name}}}"), value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    /// Tests that change the global language must not run side by side.
    static LANGUAGE_LOCK: Mutex<()> = Mutex::new(());

    fn catalog(code: &str) -> Vec<(String, String)> {
        let language = LANGUAGES.iter().find(|l| l.code == code).expect("known language");
        flatten(language.source).unwrap_or_else(|e| panic!("{code} does not parse: {e}"))
    }

    fn keys(code: &str) -> BTreeSet<String> {
        catalog(code).into_iter().map(|(k, _)| k).collect()
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else { break };
            found.insert(rest[start + 1..start + end].to_string());
            rest = &rest[start + end + 1..];
        }
        found
    }

    #[test]
    fn language_codes_are_unique_and_english_is_first() {
        assert_eq!(LANGUAGES[0].code, "en", "English is the fallback and must come first");
        let codes: BTreeSet<_> = LANGUAGES.iter().map(|l| l.code).collect();
        assert_eq!(codes.len(), LANGUAGES.len());
        assert!(!codes.contains(AUTO));
    }

    #[test]
    fn translations_only_use_keys_that_english_has() {
        let english = keys("en");
        for language in LANGUAGES {
            let extra: Vec<_> = keys(language.code).difference(&english).cloned().collect();
            assert!(extra.is_empty(), "{} has keys English doesn't: {extra:?}", language.code);
        }
    }

    #[test]
    fn translations_are_complete() {
        // A missing key falls back to English, which is fine for a new language in progress,
        // but the bundled ones are kept complete.
        let english = keys("en");
        for language in LANGUAGES {
            let missing: Vec<_> = english.difference(&keys(language.code)).cloned().collect();
            assert!(missing.is_empty(), "{} is missing: {missing:?}", language.code);
        }
    }

    #[test]
    fn translations_keep_the_placeholders() {
        let english: std::collections::HashMap<_, _> = catalog("en").into_iter().collect();
        for language in LANGUAGES {
            for (key, text) in catalog(language.code) {
                assert_eq!(
                    placeholders(&text),
                    placeholders(&english[&key]),
                    "{}: `{key}` must have the same {{placeholders}} as English",
                    language.code
                );
            }
        }
    }

    #[test]
    fn no_empty_translations() {
        for language in LANGUAGES {
            for (key, text) in catalog(language.code) {
                assert!(!text.trim().is_empty(), "{}: `{key}` is empty", language.code);
            }
        }
    }

    /// Every `t("key")` / `tf("key", …)` in the sources.
    fn keys_used_in_sources() -> BTreeSet<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut used = BTreeSet::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for call in ["t(\"", "tf(\""] {
                for (at, _) in source.match_indices(call) {
                    let before = source[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue; // part of another identifier, like `format(`
                    }
                    let rest = &source[at + call.len()..];
                    used.insert(rest[..rest.find('"').unwrap()].to_string());
                }
            }
        }
        used
    }

    #[test]
    fn every_key_used_in_the_code_exists() {
        let english = keys("en");
        let unknown: Vec<_> = keys_used_in_sources().difference(&english).cloned().collect();
        assert!(unknown.is_empty(), "keys used in the code but missing from locales/en.toml: {unknown:?}");
    }

    #[test]
    fn every_key_in_the_catalog_is_used() {
        let unused: Vec<_> = keys("en").difference(&keys_used_in_sources()).cloned().collect();
        assert!(unused.is_empty(), "keys in locales/en.toml that nothing uses: {unused:?}");
    }

    #[test]
    fn matches_system_locales() {
        assert_eq!(find("pt_BR.UTF-8"), find("pt-BR"));
        assert_eq!(LANGUAGES[find("pt_BR.UTF-8").unwrap()].code, "pt-BR");
        assert_eq!(LANGUAGES[find("pt_PT").unwrap()].code, "pt-BR", "other Portuguese falls back to ours");
        assert_eq!(LANGUAGES[find("PT-br").unwrap()].code, "pt-BR");
        assert_eq!(find("en_GB.UTF-8"), Some(0));
        assert_eq!(find("fr_FR.UTF-8"), None);
        assert_eq!(find("C"), None);
        assert_eq!(find(""), None);
    }

    #[test]
    fn missing_keys_fall_back_to_english_then_the_key() {
        let english: Catalog = [("a", "Hello"), ("b", "Bye")].into();
        let other: Catalog = [("a", "Olá")].into();
        let cats = [english, other];
        assert_eq!(lookup(&cats, 1, "a"), "Olá");
        assert_eq!(lookup(&cats, 1, "b"), "Bye");
        assert_eq!(lookup(&cats, 1, "nope"), "nope");
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("{who} took {key}", &[("key", "Print"), ("who", "GNOME")]), "GNOME took Print");
        assert_eq!(fill("no placeholders", &[("x", "y")]), "no placeholders");
        assert_eq!(fill("{unknown} stays", &[("x", "y")]), "{unknown} stays");
    }

    #[test]
    fn switching_language_changes_the_text() {
        let _guard = LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_language("pt-BR");
        assert_eq!(t("tool.pen"), "Caneta");
        assert_eq!(tf("error.no_monitor", &[("name", "X")]), "Nenhum monitor chamado 'X' (veja --list-monitors)");
        set_language("en");
        assert_eq!(t("tool.pen"), "Pen");
        set_language("klingon"); // unknown codes use English
        assert_eq!(t("tool.pen"), "Pen");
    }
}
