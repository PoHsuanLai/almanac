//! The live objects declare the same interface as the frozen skeletons: the checked-in
//! introspection file is the contract for both.

use almanac_dbus::*;
use std::os::fd::OwnedFd;
use std::sync::Arc;

struct Nobody;

impl Serve for Nobody {
    async fn serve(
        &self,
        _sender: &str,
        _call: Call,
        _fd: Option<OwnedFd>,
    ) -> Result<Vec<String>, MemoryError> {
        Err(MemoryError::NotAllowed(String::new()))
    }
}

#[test]
fn the_served_objects_introspect_as_the_skeletons_do() {
    assert_eq!(served_introspection(&Arc::new(Nobody)), introspection());
}
