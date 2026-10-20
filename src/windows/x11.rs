//! Windows on an X11 screen, from the window manager's EWMH properties.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, MapState, Window};
use x11rb::rust_connection::RustConnection;

use super::{WindowInfo, WindowSource};

pub struct X11Windows {
    /// `$DISPLAY`, or another one to ask (a test's).
    display: Option<String>,
}

impl X11Windows {
    pub fn new() -> Self {
        X11Windows { display: None }
    }

    #[cfg(test)]
    fn on(display: &str) -> Self {
        X11Windows { display: Some(display.into()) }
    }
}

impl WindowSource for X11Windows {
    fn name(&self) -> &'static str {
        "the X11 window manager"
    }

    fn list(&self) -> Option<Vec<WindowInfo>> {
        let (conn, screen) = RustConnection::connect(self.display.as_deref()).ok()?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)?;

        // The window manager lists them bottom to top; we want the topmost first.
        let mut ids = words(&conn, root, atoms.client_list_stacking);
        if ids.is_empty() {
            ids = words(&conn, root, atoms.client_list);
        }
        if ids.is_empty() {
            return None; // no EWMH window manager, or nothing is open
        }
        ids.reverse();
        let desktop = words(&conn, root, atoms.current_desktop).first().copied();
        let active = words(&conn, root, atoms.active_window).first().copied();

        let windows = ids.into_iter().filter_map(|id| window(&conn, root, &atoms, id, desktop, active)).collect();
        Some(windows)
    }
}

struct Atoms {
    client_list: Atom,
    client_list_stacking: Atom,
    active_window: Atom,
    current_desktop: Atom,
    wm_desktop: Atom,
    wm_name: Atom,
    utf8: Atom,
    frame_extents: Atom,
    window_type: Atom,
    wm_state: Atom,
    hidden: Atom,
    /// The window types worth listing.
    shown_types: [Atom; 3],
}

impl Atoms {
    fn new(conn: &RustConnection) -> Option<Self> {
        let names: [&[u8]; 14] = [
            b"_NET_CLIENT_LIST",
            b"_NET_CLIENT_LIST_STACKING",
            b"_NET_ACTIVE_WINDOW",
            b"_NET_CURRENT_DESKTOP",
            b"_NET_WM_DESKTOP",
            b"_NET_WM_NAME",
            b"UTF8_STRING",
            b"_NET_FRAME_EXTENTS",
            b"_NET_WM_WINDOW_TYPE",
            b"_NET_WM_STATE",
            b"_NET_WM_STATE_HIDDEN",
            b"_NET_WM_WINDOW_TYPE_NORMAL",
            b"_NET_WM_WINDOW_TYPE_DIALOG",
            b"_NET_WM_WINDOW_TYPE_UTILITY",
        ];
        // Ask for all of them, then read the answers.
        let cookies: Vec<_> = names.iter().map(|n| conn.intern_atom(false, n)).collect::<Result<_, _>>().ok()?;
        let a: Vec<Atom> = cookies.into_iter().map(|c| c.reply().map(|r| r.atom)).collect::<Result<_, _>>().ok()?;
        Some(Atoms {
            client_list: a[0],
            client_list_stacking: a[1],
            active_window: a[2],
            current_desktop: a[3],
            wm_desktop: a[4],
            wm_name: a[5],
            utf8: a[6],
            frame_extents: a[7],
            window_type: a[8],
            wm_state: a[9],
            hidden: a[10],
            shown_types: [a[11], a[12], a[13]],
        })
    }
}

/// A property made of 32-bit words (windows, atoms, numbers); empty if it isn't there.
fn words(conn: &RustConnection, window: Window, property: Atom) -> Vec<u32> {
    let Some(reply) =
        conn.get_property(false, window, property, AtomEnum::ANY, 0, 4096).ok().and_then(|c| c.reply().ok())
    else {
        return Vec::new();
    };
    reply.value32().map(|w| w.collect()).unwrap_or_default()
}

/// A text property, as the program gave it.
fn text(conn: &RustConnection, window: Window, property: Atom, kind: Atom) -> Option<String> {
    let reply = conn.get_property(false, window, property, kind, 0, 1024).ok()?.reply().ok()?;
    (!reply.value.is_empty()).then(|| String::from_utf8_lossy(&reply.value).into_owned())
}

/// `None` for a window that should not be offered: not on screen, on another desktop, or
/// not a window a person would want a picture of (a panel, the desktop, Queijo itself).
fn window(
    conn: &RustConnection,
    root: Window,
    atoms: &Atoms,
    id: Window,
    current_desktop: Option<u32>,
    active: Option<u32>,
) -> Option<WindowInfo> {
    if conn.get_window_attributes(id).ok()?.reply().ok()?.map_state != MapState::VIEWABLE {
        return None;
    }
    let types = words(conn, id, atoms.window_type);
    if !types.is_empty() && !types.iter().any(|t| atoms.shown_types.contains(t)) {
        return None;
    }
    if words(conn, id, atoms.wm_state).contains(&atoms.hidden) {
        return None;
    }
    // 0xFFFFFFFF: on every desktop.
    if let (Some(on), Some(now)) = (words(conn, id, atoms.wm_desktop).first(), current_desktop) {
        if *on != now && *on != 0xFFFF_FFFF {
            return None;
        }
    }
    // WM_CLASS is the instance and the class, each ending in a NUL.
    let class = text(conn, id, AtomEnum::WM_CLASS.into(), AtomEnum::STRING.into()).unwrap_or_default();
    let mut names = class.split('\0').filter(|s| !s.is_empty());
    let (instance, app) = (names.next().unwrap_or_default().to_string(), names.next().unwrap_or_default().to_string());
    if instance.eq_ignore_ascii_case("queijo") || app.eq_ignore_ascii_case("queijo") {
        return None;
    }
    let title = text(conn, id, atoms.wm_name, atoms.utf8)
        .or_else(|| text(conn, id, AtomEnum::WM_NAME.into(), AtomEnum::STRING.into()))
        .unwrap_or_default();

    // The window's own place, then the window manager's frame around it.
    let size = conn.get_geometry(id).ok()?.reply().ok()?;
    let origin = conn.translate_coordinates(id, root, 0, 0).ok()?.reply().ok()?;
    let extents = words(conn, id, atoms.frame_extents);
    let (left, right, top, bottom) = match extents[..] {
        [l, r, t, b] => (l as i32, r as i32, t as i32, b as i32),
        _ => (0, 0, 0, 0),
    };
    let frame = [
        i32::from(origin.dst_x) - left,
        i32::from(origin.dst_y) - top,
        i32::from(size.width) + left + right,
        i32::from(size.height) + top + bottom,
    ];
    if frame[2] <= 0 || frame[3] <= 0 {
        return None;
    }
    Some(WindowInfo {
        id: u64::from(id),
        title,
        app: if app.is_empty() { instance } else { app },
        frame,
        buffer: frame,
        focused: active == Some(id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::VirtualDisplay;
    use x11rb::protocol::xproto::{CreateWindowAux, PropMode, WindowClass};
    use x11rb::wrapper::ConnectionExt as _;
    use x11rb::COPY_DEPTH_FROM_PARENT;

    struct Fake<'a> {
        conn: &'a RustConnection,
        root: Window,
    }

    impl Fake<'_> {
        fn atom(&self, name: &str) -> Atom {
            self.conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom
        }

        /// A window at `x`, `y` with a client area of `w` by `h`, as a window manager would see it.
        fn window(&self, (x, y, w, h): (i16, i16, u16, u16), class: &str, title: &str) -> Window {
            let id = self.conn.generate_id().unwrap();
            self.conn
                .create_window(
                    COPY_DEPTH_FROM_PARENT,
                    id,
                    self.root,
                    x,
                    y,
                    w,
                    h,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    0,
                    &CreateWindowAux::new(),
                )
                .unwrap();
            let utf8 = self.atom("UTF8_STRING");
            self.conn
                .change_property8(PropMode::REPLACE, id, AtomEnum::WM_CLASS, AtomEnum::STRING, class.as_bytes())
                .unwrap();
            self.conn
                .change_property8(PropMode::REPLACE, id, self.atom("_NET_WM_NAME"), utf8, title.as_bytes())
                .unwrap();
            self.conn.map_window(id).unwrap();
            id
        }

        fn set(&self, window: Window, property: &str, values: &[u32]) {
            self.conn
                .change_property32(PropMode::REPLACE, window, self.atom(property), AtomEnum::CARDINAL, values)
                .unwrap();
        }

        fn set_atoms(&self, window: Window, property: &str, values: &[&str]) {
            let atoms: Vec<u32> = values.iter().map(|v| self.atom(v)).collect();
            self.conn
                .change_property32(PropMode::REPLACE, window, self.atom(property), AtomEnum::ATOM, &atoms)
                .unwrap();
        }
    }

    #[test]
    fn lists_the_windows_a_window_manager_would() {
        let Some(display) = VirtualDisplay::start() else { return };
        let (conn, screen) = RustConnection::connect(Some(&display.name)).unwrap();
        let root = conn.setup().roots[screen].root;
        let fake = Fake { conn: &conn, root };

        let editor = fake.window((10, 40, 300, 200), "code\0Code\0", "main.rs — project");
        fake.set(editor, "_NET_FRAME_EXTENTS", &[1, 1, 24, 1]); // left, right, top, bottom
        fake.set_atoms(editor, "_NET_WM_WINDOW_TYPE", &["_NET_WM_WINDOW_TYPE_NORMAL"]);
        let terminal = fake.window((200, 100, 320, 240), "xterm\0XTerm\0", "ann@host: ~");
        let panel = fake.window((0, 0, 640, 24), "panel\0Panel\0", "panel");
        fake.set_atoms(panel, "_NET_WM_WINDOW_TYPE", &["_NET_WM_WINDOW_TYPE_DOCK"]);
        let own = fake.window((0, 0, 100, 100), "queijo\0queijo\0", "Queijo");
        let minimized = fake.window((0, 0, 100, 100), "gone\0Gone\0", "minimized");
        fake.set_atoms(minimized, "_NET_WM_STATE", &["_NET_WM_STATE_HIDDEN"]);
        let elsewhere = fake.window((0, 0, 100, 100), "away\0Away\0", "on desktop two");
        fake.set(elsewhere, "_NET_WM_DESKTOP", &[1]);
        let everywhere = fake.window((400, 300, 50, 60), "pin\0Pin\0", "everywhere");
        fake.set(everywhere, "_NET_WM_DESKTOP", &[0xFFFF_FFFF]);

        // The window manager's root properties: bottom to top, the current desktop, the focus.
        let stacking = [editor, panel, own, minimized, elsewhere, terminal, everywhere];
        let list = fake.atom("_NET_CLIENT_LIST_STACKING");
        conn.change_property32(PropMode::REPLACE, root, list, AtomEnum::WINDOW, &stacking).unwrap();
        fake.set(root, "_NET_CURRENT_DESKTOP", &[0]);
        conn.change_property32(PropMode::REPLACE, root, fake.atom("_NET_ACTIVE_WINDOW"), AtomEnum::WINDOW, &[terminal])
            .unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();

        let windows = X11Windows::on(&display.name).list().unwrap();
        let titles: Vec<_> = windows.iter().map(|w| w.title.as_str()).collect();
        assert_eq!(
            titles,
            ["everywhere", "ann@host: ~", "main.rs — project"],
            "topmost first, and no panel, Queijo, minimized or other-desktop window"
        );

        let by_title = |t: &str| windows.iter().find(|w| w.title == t).unwrap();
        // The frame is the client area grown by the window manager's borders and title bar.
        assert_eq!(by_title("main.rs — project").frame, [9, 16, 302, 225]);
        assert_eq!(by_title("main.rs — project").app, "Code");
        assert_eq!(by_title("ann@host: ~").frame, [200, 100, 320, 240]);
        assert!(by_title("ann@host: ~").focused && !by_title("everywhere").focused);
    }

    #[test]
    fn without_a_window_manager_there_is_no_list() {
        let Some(display) = VirtualDisplay::start() else { return };
        assert!(X11Windows::on(&display.name).list().is_none());
        assert!(X11Windows::on(":999").list().is_none());
    }
}
