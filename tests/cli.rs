//! Runs the real binary. Everything here works without a display, and each run
//! gets its own empty config folder and no desktop notifications.

use std::path::Path;
use std::process::{Command, Output};

fn queijo(config_home: &Path, locale: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_queijo"));
    cmd.args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .env("QUEIJO_NO_NOTIFY", "1")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DISPLAY")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANGUAGE")
        .env_remove("LANG");
    if let Some(locale) = locale {
        cmd.env("LC_ALL", locale);
    }
    cmd.output().expect("the binary runs")
}

fn run(args: &[&str]) -> Output {
    queijo(tempfile::tempdir().unwrap().path(), None, args)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn prints_its_version() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(stdout(&out), format!("queijo {}\n", env!("CARGO_PKG_VERSION")));
}

#[test]
fn the_help_lists_the_options() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let help = stdout(&out);
    assert!(help.contains("Screenshot, screencast and GIF tool"), "{help}");
    for option in ["--kind", "--area", "--monitor", "--quick", "--settings", "--stop", "--list-monitors", "--doctor"] {
        assert!(help.contains(option), "{option} is missing from the help");
    }
    assert!(!help.contains("--record"), "the internal option stays hidden");
}

#[test]
fn rejects_bad_arguments() {
    for args in [&["--kind", "movie"][..], &["--area", "desktop"], &["--delay", "soon"], &["--nonsense"]] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
    }
}

#[test]
fn listing_monitors_works_without_a_display() {
    let out = run(&["--list-monitors"]);
    assert!(out.status.success(), "{}", stderr(&out));
}

#[test]
fn the_doctor_reports_without_a_display() {
    let out = run(&["--doctor"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.starts_with(&format!("queijo {}\n", env!("CARGO_PKG_VERSION"))), "{report}");
    for section in
        ["Session:", "Desktop:", "Distro:", "Drawing:", "Monitors:", "Screenshot, tried in this order:", "Tools:"]
    {
        assert!(report.contains(section), "{section} is missing: {report}");
    }
}

#[test]
fn refuses_a_bad_recording_area_in_english_by_default() {
    let out = run(&["--record", "garbage"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Bad recording area 'garbage'"), "{}", stderr(&out));
}

#[test]
fn follows_the_system_language() {
    let dir = tempfile::tempdir().unwrap();
    let out = queijo(dir.path(), Some("pt_BR.UTF-8"), &["--record", "garbage"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Área de gravação inválida 'garbage'"), "{}", stderr(&out));

    let other = queijo(dir.path(), Some("fr_FR.UTF-8"), &["--record", "garbage"]);
    assert!(stderr(&other).contains("Bad recording area"), "unknown languages fall back to English");
}

#[test]
fn the_configured_language_beats_the_system_one() {
    let dir = tempfile::tempdir().unwrap();
    let config = |language: &str| {
        std::fs::create_dir_all(dir.path().join("queijo")).unwrap();
        std::fs::write(dir.path().join("queijo/config.toml"), format!("language = \"{language}\"\n")).unwrap();
    };

    config("en");
    let out = queijo(dir.path(), Some("pt_BR.UTF-8"), &["--record", "garbage"]);
    assert!(stderr(&out).contains("Bad recording area"), "{}", stderr(&out));

    config("pt-BR");
    let out = queijo(dir.path(), None, &["--record", "garbage"]);
    assert!(stderr(&out).contains("Área de gravação inválida"), "{}", stderr(&out));

    config("klingon");
    let out = queijo(dir.path(), Some("pt_BR.UTF-8"), &["--record", "garbage"]);
    assert!(stderr(&out).contains("Bad recording area"), "an unknown language falls back to English");
}

#[test]
fn a_broken_config_does_not_stop_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("queijo")).unwrap();
    std::fs::write(dir.path().join("queijo/config.toml"), "this is [not toml").unwrap();
    let out = queijo(dir.path(), None, &["--record", "garbage"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("ignoring"), "it says so: {}", stderr(&out));
    assert!(stderr(&out).contains("Bad recording area"));
}
