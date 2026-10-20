//! A tray icon while recording, for desktops that have a system tray (KDE, Xfce, Cinnamon,
//! MATE, waybar, ...): the StatusNotifierItem protocol, through `ksni`. Clicking it, or its
//! menu item, stops the recording, as the GNOME Shell extension's indicator does.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ksni::blocking::{Handle, TrayMethods};
use ksni::{Icon, MenuItem, ToolTip};

use crate::i18n::t;

struct Indicator {
    started: Instant,
    stop: Mutex<Sender<()>>,
}

/// `0:07`, `12:30`, `1:02:03`.
fn elapsed(secs: u64) -> String {
    match (secs / 3600, secs / 60 % 60, secs % 60) {
        (0, m, s) => format!("{m}:{s:02}"),
        (h, m, s) => format!("{h}:{m:02}:{s:02}"),
    }
}

/// A red disc, as the ARGB32 bytes (in network order: alpha first) the protocol wants,
/// so there is an icon even where the theme has none to name.
fn red_dot(size: i32) -> Icon {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let r = size as f32 / 2.0 - 1.0;
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            // A one-pixel soft edge.
            let alpha = ((r + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u8;
            data.extend([alpha, 255, 69, 58]);
        }
    }
    Icon { width: size, height: size, data }
}

impl ksni::Tray for Indicator {
    fn id(&self) -> String {
        "queijo-recording".into()
    }

    fn title(&self) -> String {
        format!("{} {}", t("tray.title"), elapsed(self.started.elapsed().as_secs()))
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![red_dot(22), red_dot(44)]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.title(),
            description: t("tray.click_to_stop").into(),
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.stop.lock().unwrap().send(());
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![StandardItem {
            label: t("tray.stop").into(),
            icon_name: "media-playback-stop".into(),
            activate: Box::new(|this: &mut Self| ksni::Tray::activate(this, 0, 0)),
            ..Default::default()
        }
        .into()]
    }
}

/// The icon, for as long as this lives.
pub struct Tray {
    handle: Handle<Indicator>,
    done: Arc<Mutex<bool>>,
}

/// Puts the icon in the tray, if there is one. `stop` is told when it is clicked. `None`
/// (said on stderr) where nothing shows tray icons.
pub fn show(stop: Sender<()>) -> Option<Tray> {
    let indicator = Indicator { started: Instant::now(), stop: Mutex::new(stop) };
    let handle = match indicator.spawn() {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("queijo: no tray icon ({e})");
            return None;
        }
    };
    // The time on it moves: tell the tray once a second that the icon changed.
    let done = Arc::new(Mutex::new(false));
    {
        let (handle, done) = (handle.clone(), done.clone());
        std::thread::spawn(move || {
            while !*done.lock().unwrap() {
                std::thread::sleep(Duration::from_secs(1));
                if handle.update(|_| ()).is_none() {
                    break;
                }
            }
        });
    }
    Some(Tray { handle, done })
}

impl Drop for Tray {
    fn drop(&mut self) {
        *self.done.lock().unwrap() = true;
        self.handle.shutdown().wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_time_it_has_been_recording() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(7), "0:07");
        assert_eq!(elapsed(754), "12:34");
        assert_eq!(elapsed(3723), "1:02:03");
    }

    #[test]
    fn draws_a_red_disc_with_clear_corners() {
        let icon = red_dot(22);
        assert_eq!((icon.width, icon.height, icon.data.len()), (22, 22, 22 * 22 * 4));
        let pixel = |x: usize, y: usize| &icon.data[(y * 22 + x) * 4..][..4];
        assert_eq!(pixel(11, 11), [255, 255, 69, 58], "opaque red in the middle (alpha, red, green, blue)");
        assert_eq!(pixel(0, 0)[0], 0, "the corner is clear");
        assert_eq!(pixel(21, 21)[0], 0);
    }

    /// Stands in for the desktop's StatusNotifierWatcher: it says a host exists and notes who
    /// registers.
    struct Watcher(Mutex<Sender<String>>);

    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, service: String) {
            let _ = self.0.lock().unwrap().send(service);
        }

        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn protocol_version(&self) -> i32 {
            0
        }

        #[zbus(property)]
        fn registered_status_notifier_items(&self) -> Vec<String> {
            Vec::new()
        }
    }

    /// The icon's side of the test below: it runs as a process of its own because the tray
    /// library reaches the bus through the environment, which tests share.
    #[test]
    fn child_shows_the_icon() {
        if std::env::var_os("QUEIJO_TRAY_CHILD").is_none() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let tray = show(tx).expect("the icon is shown");
        rx.recv_timeout(Duration::from_secs(15)).expect("the icon was clicked");
        // Let the click's reply go out before the icon goes away.
        std::thread::sleep(Duration::from_millis(500));
        drop(tray);
    }

    #[test]
    fn shows_up_in_the_tray_and_a_click_stops_the_recording() {
        use crate::testutil::PrivateBus;
        use std::collections::HashMap;
        use zbus::zvariant::OwnedValue;

        let Some(bus) = PrivateBus::start() else { return };
        let (registered, items) = std::sync::mpsc::channel();
        let _watcher = zbus::blocking::connection::Builder::address(bus.address.as_str())
            .unwrap()
            .name("org.kde.StatusNotifierWatcher")
            .unwrap()
            .serve_at("/StatusNotifierWatcher", Watcher(Mutex::new(registered)))
            .unwrap()
            .build()
            .unwrap();

        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "record::tray::tests::child_shows_the_icon", "--nocapture", "--test-threads", "1"])
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("QUEIJO_TRAY_CHILD", "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let service = items.recv_timeout(Duration::from_secs(15)).expect("the icon registered itself");
        // Some hosts are given the name, some the name and the path.
        let name = service.split('/').next().unwrap().to_string();
        assert!(name.starts_with("org.kde.StatusNotifierItem-"), "{service}");

        let client = bus.connect();
        let all = client
            .call_method(
                Some(name.as_str()),
                "/StatusNotifierItem",
                Some("org.freedesktop.DBus.Properties"),
                "GetAll",
                &"org.kde.StatusNotifierItem",
            )
            .unwrap();
        let (props,): (HashMap<String, OwnedValue>,) = all.body().deserialize().unwrap();
        let text = |key: &str| String::try_from(props[key].try_clone().unwrap()).unwrap();
        assert_eq!(text("Id"), "queijo-recording");
        assert_eq!(text("Status"), "Active");
        assert_eq!(text("Category"), "ApplicationStatus");
        assert!(text("Title").starts_with("Queijo is recording"), "{}", text("Title"));
        assert!(props.contains_key("IconPixmap") && props.contains_key("ToolTip"));

        // A click on it is what stops the recording.
        client
            .call_method(
                Some(name.as_str()),
                "/StatusNotifierItem",
                Some("org.kde.StatusNotifierItem"),
                "Activate",
                &(0i32, 0i32),
            )
            .unwrap();
        let status = child.wait().unwrap();
        assert!(status.success(), "the icon's process saw the click: {status}");
    }
}
