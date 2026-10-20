//! Talking to the X server directly.

use x11rb::protocol::xproto::ConnectionExt;
use x11rb::rust_connection::RustConnection;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};
    use x11rb::COPY_DEPTH_FROM_PARENT;

    /// An X server of its own. `None` without `Xvfb`; set QUEIJO_REQUIRE_XVFB=1 to make that a failure (CI does).
    pub struct VirtualDisplay {
        child: Child,
        pub name: String,
    }

    impl VirtualDisplay {
        pub fn start() -> Option<Self> {
            let child = Command::new("Xvfb")
                .args(["-displayfd", "1", "-screen", "0", "640x480x24", "-nolisten", "tcp"])
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
            Some(VirtualDisplay { child, name: format!(":{}", number.trim()) })
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
    }
}
