//! Windows on Hyprland, from `hyprctl -j clients` and `-j monitors`.

use std::process::Command;

use serde_json::Value;

use super::{WindowInfo, WindowSource};

pub struct Hyprland;

fn hyprctl(what: &str) -> Option<String> {
    let out = Command::new("hyprctl").args(["-j", what]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

impl WindowSource for Hyprland {
    fn name(&self) -> &'static str {
        "Hyprland"
    }

    fn list(&self) -> Option<Vec<WindowInfo>> {
        parse(&hyprctl("clients")?, &hyprctl("monitors")?)
    }
}

/// The windows that are on screen: mapped, and on the workspace their monitor shows.
/// Floating ones come first, then the most recently used, which is as close to
/// "topmost first" as Hyprland says.
fn parse(clients: &str, monitors: &str) -> Option<Vec<WindowInfo>> {
    let clients: Vec<Value> = serde_json::from_str(clients).ok()?;
    let monitors: Vec<Value> = serde_json::from_str(monitors).ok()?;
    let mut found: Vec<(bool, i64, WindowInfo)> = clients.iter().filter_map(|c| client(c, &monitors)).collect();
    found.sort_by_key(|(floating, recency, _)| (!*floating, *recency));
    Some(found.into_iter().map(|(_, _, w)| w).collect())
}

/// One window, with whether it floats and how recently it was used; `None` if it is not
/// on screen or Hyprland left out something needed to place it.
fn client(c: &Value, monitors: &[Value]) -> Option<(bool, i64, WindowInfo)> {
    let flag = |key: &str| c.get(key).and_then(Value::as_bool).unwrap_or(false);
    let text = |key: &str| c.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let workspace = c.get("workspace")?.get("id")?.as_i64()?;
    let monitor = c.get("monitor")?.as_i64()?;
    let shown = monitors
        .iter()
        .find(|m| m.get("id").and_then(Value::as_i64) == Some(monitor))
        .and_then(|m| m.get("activeWorkspace")?.get("id")?.as_i64());
    // Not mapped, hidden, in a special workspace (the scratchpad, negative), or on a
    // workspace its monitor isn't showing.
    if !flag("mapped") || flag("hidden") || workspace < 0 || shown != Some(workspace) {
        return None;
    }
    let pair = |key: &str| {
        let v = c.get(key)?.as_array()?;
        Some((v.first()?.as_i64()? as i32, v.get(1)?.as_i64()? as i32))
    };
    let ((x, y), (w, h)) = (pair("at")?, pair("size")?);
    let app = text("class");
    if w <= 0 || h <= 0 || app.eq_ignore_ascii_case("queijo") {
        return None;
    }
    let recency = c.get("focusHistoryID").and_then(Value::as_i64).unwrap_or(i64::MAX);
    let address = text("address");
    let info = WindowInfo {
        id: u64::from_str_radix(address.trim_start_matches("0x"), 16).unwrap_or_default(),
        title: text("title"),
        app,
        frame: [x, y, w, h],
        buffer: [x, y, w, h],
        focused: recency == 0,
    };
    Some((flag("floating"), recency, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written from the fields Hyprland's `clients` and `monitors` give; never captured from a running Hyprland.
    const CLIENTS: &str = include_str!("../../tests/fixtures/hyprland-clients.json");
    const MONITORS: &str = include_str!("../../tests/fixtures/hyprland-monitors.json");

    #[test]
    fn lists_the_windows_on_the_workspaces_that_are_shown() {
        let windows = parse(CLIENTS, MONITORS).unwrap();
        let titles: Vec<_> = windows.iter().map(|w| w.title.as_str()).collect();
        // The floating Volume Control first, then by how recently each was used. Not here: the
        // editor itself, the unmapped one, the scratchpad, workspace 2 (not shown), no monitor.
        assert_eq!(titles, ["Volume Control", "~/src — kitty", "Queijo — Mozilla Firefox"]);
        let by = |t: &str| windows.iter().find(|w| w.title == t).unwrap();
        assert_eq!(by("Volume Control").frame, [400, 300, 500, 400]);
        assert!(by("Volume Control").focused && !by("~/src — kitty").focused);
        assert_eq!(by("~/src — kitty").app, "kitty");
        assert_eq!(
            by("Queijo — Mozilla Firefox").frame,
            [1280, 30, 1920, 1050],
            "on the second monitor, whose workspace 3 is shown"
        );
        assert_eq!(by("Volume Control").id, 0x5581a0c52bc0);
    }

    #[test]
    fn answers_that_are_not_lists_give_nothing() {
        assert!(parse("", MONITORS).is_none());
        assert!(parse(CLIENTS, "oops").is_none());
        assert_eq!(parse("[]", "[]").unwrap().len(), 0);
        // A client with fields missing is skipped, not a failure of the whole list.
        assert_eq!(parse(r#"[{"class":"x"}]"#, MONITORS).unwrap().len(), 0);
    }
}
