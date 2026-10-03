//! A private session bus: `dbus-daemon` on a socket in a scratch directory, with its own
//! configuration. A test never reaches the real session bus.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// A running private bus; it is killed when this is dropped.
pub struct PrivateBus {
    pub address: String,
    child: Child,
}

fn config(socket: &Path) -> String {
    format!(
        r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
        socket.display()
    )
}

impl PrivateBus {
    /// Starts a bus whose socket and configuration live in `dir`.
    pub fn start(dir: &Path) -> PrivateBus {
        let socket = dir.join("bus.sock");
        let conf = dir.join("bus.conf");
        std::fs::write(&conf, config(&socket)).expect("bus config");
        let mut child = Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address=1")
            .arg(format!("--config-file={}", conf.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon is installed");
        let stdout = child.stdout.take().expect("stdout");
        let mut address = String::new();
        BufReader::new(stdout)
            .read_line(&mut address)
            .expect("the bus prints its address");
        PrivateBus {
            address: address.trim().to_owned(),
            child,
        }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}
