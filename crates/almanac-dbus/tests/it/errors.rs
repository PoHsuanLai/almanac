//! The error names and the bus names are the documented ones.

use almanac_core::Refusal;
use almanac_dbus::*;
use zbus::DBusError;

#[test]
fn refusals_map_one_to_one_to_error_names() {
    let refusals = [
        (Refusal::NotAllowed, "NotAllowed"),
        (Refusal::SpaceLocked, "SpaceLocked"),
        (Refusal::SpaceUnknown, "SpaceUnknown"),
        (Refusal::OutsideSpace, "OutsideSpace"),
        (Refusal::PlanStale, "PlanStale"),
        (Refusal::PlanExpired, "PlanExpired"),
        (Refusal::NoSuchFact, "NoSuchFact"),
        (Refusal::NotPending, "NotPending"),
        (Refusal::Busy, "Busy"),
        (Refusal::SpaceFull, "SpaceFull"),
        (Refusal::Unavailable, "Unavailable"),
        (
            Refusal::NotKept(almanac_core::DropReason::Paused),
            "NotKept",
        ),
        (Refusal::Invalid("x".into()), "Invalid"),
    ];
    let mut names = std::collections::BTreeSet::new();
    for (refusal, variant) in refusals {
        let error = MemoryError::from(&refusal);
        let name = error.name().to_string();
        assert_eq!(name, format!("{ERROR_PREFIX}.{variant}"));
        assert_eq!(error.refusal(), Some(refusal));
        names.insert(name);
    }
    assert_eq!(names.len(), 13);
}

#[test]
fn names_are_the_documented_ones() {
    assert_eq!(MEMORY_BUS, "org.quire.Memory1");
    assert_eq!(MEMORY_PATH, "/org/quire/Memory1");
    assert!(zbus::zvariant::ObjectPath::try_from(MEMORY_PATH).is_ok());
    for interface in [RECORD_INTERFACE, RECALL_INTERFACE, CONTROL_INTERFACE] {
        assert!(interface.starts_with(MEMORY_BUS));
    }
}
