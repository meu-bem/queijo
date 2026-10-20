//! Windows on Sway, from `swaymsg -t get_tree`.

use std::process::Command;

use serde_json::Value;

use super::{WindowInfo, WindowSource};

pub struct Sway;

impl WindowSource for Sway {
    fn name(&self) -> &'static str {
        "Sway"
    }

    fn list(&self) -> Option<Vec<WindowInfo>> {
        let out = Command::new("swaymsg").args(["-t", "get_tree", "-r"]).output().ok()?;
        out.status.success().then(|| parse(&String::from_utf8_lossy(&out.stdout))).flatten()
    }
}

/// The windows in a `get_tree` answer that are on screen: the leaves of the tree
/// (containers holding no others) whose workspace is shown. Floating windows are
/// above the tiled ones, and the last of them is the topmost.
fn parse(json: &str) -> Option<Vec<WindowInfo>> {
    let tree: Value = serde_json::from_str(json).ok()?;
    let (mut tiled, mut floating) = (Vec::new(), Vec::new());
    collect(&tree, &mut tiled, &mut floating);
    floating.reverse();
    floating.extend(tiled);
    Some(floating)
}

fn collect(node: &Value, tiled: &mut Vec<WindowInfo>, floating: &mut Vec<WindowInfo>) {
    let children = |key: &str| node.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let (nodes, floats) = (children("nodes"), children("floating_nodes"));
    // The scratchpad is a workspace of its own output, which is never shown.
    if node.get("type").and_then(Value::as_str) == Some("output")
        && node.get("name").and_then(Value::as_str) == Some("__i3")
    {
        return;
    }
    if nodes.is_empty() && floats.is_empty() {
        let kind = node.get("type").and_then(Value::as_str);
        if matches!(kind, Some("con" | "floating_con")) && node.get("visible").and_then(Value::as_bool) == Some(true) {
            if let Some(info) = window(node) {
                if kind == Some("floating_con") {
                    floating.push(info)
                } else {
                    tiled.push(info)
                }
            }
        }
        return;
    }
    for child in &nodes {
        collect(child, tiled, floating);
    }
    for child in &floats {
        collect(child, tiled, floating);
    }
}

fn window(node: &Value) -> Option<WindowInfo> {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
    // A Wayland window has an app_id; an X11 one (through Xwayland) has a class.
    let mut app = text(node.get("app_id"));
    if app.is_empty() {
        app = text(node.get("window_properties").and_then(|p| p.get("class")));
    }
    if app.eq_ignore_ascii_case("queijo") {
        return None;
    }
    let rect = node.get("rect")?;
    let n = |key: &str| rect.get(key).and_then(Value::as_i64).map(|v| v as i32);
    let frame = [n("x")?, n("y")?, n("width")?, n("height")?];
    (frame[2] > 0 && frame[3] > 0).then(|| WindowInfo {
        id: node.get("id").and_then(Value::as_u64).unwrap_or_default(),
        title: text(node.get("name")),
        app,
        frame,
        buffer: frame,
        focused: node.get("focused").and_then(Value::as_bool).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a headless Sway answered with two windows open (captured, not written).
    const REAL: &str = include_str!("../../tests/fixtures/sway-tree-real.json");
    /// The same shape, grown to what a busy screen has.
    const MIXED: &str = include_str!("../../tests/fixtures/sway-tree-mixed.json");

    #[test]
    fn reads_what_a_real_sway_answered() {
        let windows = parse(REAL).unwrap();
        assert_eq!(windows.len(), 2);
        assert!(windows.iter().all(|w| w.app == "queijo-settings" && w.title == "Queijo Settings"));
        assert!(windows.iter().all(|w| w.frame == [210, 40, 860, 640]));
        // Of two floating windows the last is on top; it is the one that has the focus.
        assert!(windows[0].focused && !windows[1].focused);
    }

    #[test]
    fn lists_the_windows_that_are_shown_topmost_first() {
        let windows = parse(MIXED).unwrap();
        let titles: Vec<_> = windows.iter().map(|w| w.title.as_str()).collect();
        // The floating Calculator is on top; then the tiled ones in the order they sit.
        // Not here: the editor itself, the workspace that is not shown, the scratchpad.
        assert_eq!(titles, ["Calculator", "Terminal", "notes.md — Editor", "Browser"]);
        let by = |t: &str| windows.iter().find(|w| w.title == t).unwrap();
        assert_eq!(by("Calculator").frame, [300, 200, 320, 400]);
        assert!(by("Calculator").focused && !by("Terminal").focused);
        assert_eq!(by("Terminal").app, "foot");
        assert_eq!(by("Browser").app, "Firefox", "an Xwayland window has a class, not an app_id");
    }

    #[test]
    fn answers_that_are_not_a_tree_give_nothing() {
        assert!(parse("").is_none());
        assert!(parse("not json").is_none());
        assert_eq!(parse("{}").unwrap().len(), 0);
        assert_eq!(parse(r#"{"type":"root","nodes":[]}"#).unwrap().len(), 0);
    }
}
