# Adding a language

Everything the program says comes from a catalog in `locales/`, one TOML file per
language, embedded in the binary. English (`locales/en.toml`) is the reference and
the fallback: a key a language leaves out is shown in English.

Brazilian Portuguese (`locales/pt-BR.toml`) is the worked example.

## 1. The catalog

Copy `locales/en.toml` to `locales/<code>.toml`, where `<code>` is a language tag
such as `es`, `fr` or `pt-BR`, and translate the values. Keep:

- **the keys** exactly as they are (nested tables become dotted keys:
  `[tool]` + `pen` is `tool.pen`);
- **the `{placeholders}`** exactly as they are: `"{key} was used by {who}"` may be
  reordered but both must appear. Values like `{date}` or `{width}` in file-name
  examples are not placeholders of the catalog and are never translated.
- Words that are key names (`Ctrl`, `Enter`, `Esc`, `Print Screen`) stay as they are.

Strings are shown in a few tight places (buttons, the bar under the mode bar, the
tooltips), so shorter is better; the settings rows wrap long descriptions.

## 2. Register it

In `src/i18n.rs`, add one line to `LANGUAGES`:

```rust
Language { code: "es", name: "Español", source: include_str!("../locales/es.toml") },
```

`name` is written in the language itself, so it can be recognized in any other.
`code` is what the settings store in `~/.config/queijo/config.toml` and what the
system locale is matched against (`es_AR.UTF-8` finds `es`; `pt_PT` finds `pt-BR`
because it is the only Portuguese we have).

That is all: the settings list it under **General → Language**, and the
**Automatic** choice picks it for people whose system is set to that language.

## 3. Check it

```sh
cargo test i18n
```

The tests fail when a catalog uses a key English doesn't have, is missing a key,
has an empty value or changes the `{placeholders}`, and when the code uses a key
that isn't in `locales/en.toml` or the English catalog has a key nothing uses.
Then look at it running:

```sh
LC_ALL=es_ES.UTF-8 queijo --settings
```

Look especially at the settings pages and the editor's tooltips, where text has
the least room.

## 4. The READMEs

Translated READMEs are named `README.<code>.md` and linked from the **Languages**
table at the top of `README.md` and of every translated README. They are not
required for a language to be added to the program.

## Adding text to the program

Never write user-visible text directly in the code. Add the key to `locales/en.toml`
(and to the other catalogs; the tests demand it), then use `t("section.key")` or
`tf("section.key", &[("name", value)])` from `crate::i18n`. Keys must be string
literals so the tests can find them.

Not translated on purpose: the command-line help (`--help`) and the option names,
the log lines printed to the terminal, and the name of the program.
