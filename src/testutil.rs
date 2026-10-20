//! Helpers shared by the tests that need a bus of their own.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;

/// A session bus of its own, so a test can play the portal. It starts no
/// services: the default configuration would launch the real portal.
pub struct PrivateBus {
    child: Child,
    pub address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    /// `None` without `dbus-daemon`; set QUEIJO_REQUIRE_DBUS=1 to make that a failure (CI does).
    pub fn start() -> Option<Self> {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:path={}</listen>\
                 <policy context=\"default\"><allow send_destination=\"*\"/><allow own=\"*\"/>\
                 <allow receive_sender=\"*\"/><allow receive_type=\"method_return\"/>\
                 <allow receive_type=\"error\"/><allow receive_type=\"signal\"/></policy>\
                 </busconfig>",
                dir.path().join("bus.sock").display()
            ),
        )
        .unwrap();
        let child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .spawn();
        let Ok(mut child) = child else {
            assert!(std::env::var_os("QUEIJO_REQUIRE_DBUS").is_none(), "dbus-daemon is required here");
            eprintln!("skipping: dbus-daemon is not installed");
            return None;
        };
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut address).unwrap();
        assert!(address.starts_with("unix:"), "dbus-daemon did not start: {address:?}");
        Some(PrivateBus { child, address: address.trim().to_string(), _dir: dir })
    }

    pub fn connect(&self) -> Connection {
        Builder::address(self.address.as_str()).unwrap().build().unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
