//! Talking to the X server directly.

use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::rust_connection::RustConnection;

use crate::monitors::Monitor;

/// Whether a compositing manager is running on the screen (it owns the
/// `_NET_WM_CM_S<n>` selection). Without one, see-through windows are drawn
/// black. `display` is as in `$DISPLAY`; `None` uses that. `None` is returned if
/// the server can't be asked.
pub fn has_compositor(display: Option<&str>) -> Option<bool> {
    let (conn, screen) = RustConnection::connect(display).ok()?;
    let atom = conn.intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes()).ok()?.reply().ok()?.atom;
    let owner = conn.get_selection_owner(atom).ok()?.reply().ok()?.owner;
    Some(owner != x11rb::NONE)
}

/// The monitors, in the order `xrandr --listmonitors` and the windowing backend
/// list them (RandR 1.5). `None` if the server can't be asked or is too old.
pub fn monitors(display: Option<&str>) -> Option<Vec<Monitor>> {
    let (conn, screen) = RustConnection::connect(display).ok()?;
    let root = conn.setup().roots[screen].root;
    let reply = conn.randr_get_monitors(root, true).ok()?.reply().ok()?;
    reply
        .monitors
        .iter()
        .enumerate()
        .map(|(index, m)| {
            let name = conn.get_atom_name(m.name).ok()?.reply().ok()?.name;
            Some(Monitor {
                index,
                name: String::from_utf8_lossy(&name).into_owned(),
                description: String::new(),
                x: m.x.into(),
                y: m.y.into(),
                width: m.width.into(),
                height: m.height.into(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};
    use x11rb::COPY_DEPTH_FROM_PARENT;

    /// An X server of its own. `None` without `Xvfb`; set QUEIJO_REQUIRE_XVFB=1 to make that a failure (CI does).
    pub struct VirtualDisplay {
        child: Child,
        pub name: String,
        /// Servers starting side by side can pick the same display number, so
        /// the tests that use one take turns.
        _turn: std::sync::MutexGuard<'static, ()>,
    }

    impl VirtualDisplay {
        pub fn start() -> Option<Self> {
            static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
            let turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
            let child = Command::new("Xvfb")
                // -noreset: by default the server restarts when its last client leaves, and a
                // connection made meanwhile is reset.
                .args(["-displayfd", "1", "-noreset", "-screen", "0", "640x480x24", "-nolisten", "tcp"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                assert!(std::env::var_os("QUEIJO_REQUIRE_XVFB").is_none(), "Xvfb is required here");
                eprintln!("skipping: Xvfb is not installed");
                return None;
            };
            let mut number = String::new();
            BufReader::new(child.stdout.take().unwrap()).read_line(&mut number).unwrap();
            assert!(!number.trim().is_empty(), "Xvfb did not start");
            Some(VirtualDisplay { child, name: format!(":{}", number.trim()), _turn: turn })
        }
    }

    impl Drop for VirtualDisplay {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn sees_whether_a_compositor_owns_the_selection() {
        let Some(display) = VirtualDisplay::start() else { return };
        assert_eq!(has_compositor(Some(&display.name)), Some(false));

        // Become the compositor, as picom or a window manager's own would.
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
        let atom = conn.intern_atom(false, format!("_NET_WM_CM_S{screen}").as_bytes()).unwrap().reply().unwrap().atom;
        conn.set_selection_owner(window, atom, x11rb::CURRENT_TIME).unwrap();
        conn.flush().unwrap();
        assert_eq!(has_compositor(Some(&display.name)), Some(true));
    }

    #[test]
    fn cannot_tell_without_a_server() {
        assert_eq!(has_compositor(Some(":999")), None);
        assert!(monitors(Some(":999")).is_none());
    }

    #[test]
    fn lists_the_monitors_with_their_geometry() {
        use x11rb::protocol::randr::MonitorInfo;

        let Some(display) = VirtualDisplay::start() else { return };
        let all = monitors(Some(&display.name)).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].index, all[0].x, all[0].y, all[0].width, all[0].height), (0, 0, 0, 640, 480));
        assert!(!all[0].name.is_empty());

        // Cut the screen into two monitors, like a second output would.
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        for (name, x) in [("left", 0), ("right", 320)] {
            let atom = conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom;
            let info = MonitorInfo {
                name: atom,
                primary: false,
                automatic: false,
                x,
                y: 0,
                width: 320,
                height: 480,
                width_in_millimeters: 100,
                height_in_millimeters: 150,
                outputs: Vec::new(),
            };
            conn.randr_set_monitor(root, info).unwrap().check().unwrap();
        }
        let all = monitors(Some(&display.name)).unwrap();
        let find = |name: &str| all.iter().find(|m| m.name == name).unwrap_or_else(|| panic!("{name} in {all:?}"));
        assert_eq!((find("left").x, find("left").width, find("left").height), (0, 320, 480));
        assert_eq!((find("right").x, find("right").width, find("right").height), (320, 320, 480));
        // The index is the position in the list, which is how the windowing backend counts.
        assert!(all.iter().enumerate().all(|(i, m)| m.index == i));
    }
}
