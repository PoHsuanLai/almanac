//! The introspection XML of `org.quire.Memory1`, from the skeletons: the checked-in
//! `dbus/org.quire.Memory1.xml` must equal it.

use crate::control::ControlSkeleton;
use crate::recall::RecallSkeleton;
use crate::record::RecordSkeleton;
use zbus::fdo;
use zbus::object_server::Interface;

/// The file under `dbus/` holding the introspection.
pub const INTROSPECTION_FILE: &str = "org.quire.Memory1.xml";

/// The introspection document: every interface memoryd serves, in one `<node>`.
pub fn introspection() -> String {
    document([&RecordSkeleton, &RecallSkeleton, &ControlSkeleton])
}

/// The document of any three interfaces (the skeletons, or memoryd's served objects).
pub(crate) fn document(interfaces: [&dyn Interface; 3]) -> String {
    let mut xml = String::from(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\"\n \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n<node>\n",
    );
    for interface in interfaces {
        interface.introspect_to_writer(&mut xml, 1);
    }
    xml.push_str("</node>\n");
    xml
}

/// The answer of every skeleton method: the interface is frozen, its behaviour not built.
pub(crate) fn frozen() -> fdo::Error {
    fdo::Error::NotSupported("almanac: frozen interface, not implemented".into())
}
