//! The checked-in introspection file is the interface the skeletons declare. A change to a
//! signature changes the file in the same commit.

use almanac_core::Refusal;
use almanac_dbus::*;
use std::path::PathBuf;
use zbus::DBusError;

#[test]
fn checked_in_introspection_matches_the_interfaces() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dbus")
        .join(INTROSPECTION_FILE);
    let expected =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let actual = introspection();
    assert!(
        actual == expected,
        "{} differs from the interfaces; it should read:\n{actual}",
        path.display()
    );
}

#[test]
fn every_member_is_declared() {
    let xml = introspection();
    let members = [
        "<interface name=\"org.quire.Memory1.Record\">",
        "<interface name=\"org.quire.Memory1.Recall\">",
        "<interface name=\"org.quire.Memory1.Control\">",
        "<method name=\"Record\">",
        "<method name=\"RecordBatch\">",
        "<method name=\"ExplainFile\">",
        "<method name=\"Mark\">",
        "<method name=\"Search\">",
        "<method name=\"Facts\">",
        "<method name=\"Related\">",
        "<method name=\"Provenance\">",
        "<method name=\"Primer\">",
        "<method name=\"Propose\">",
        "<method name=\"Spaces\">",
        "<method name=\"Status\">",
        "<method name=\"Timeline\">",
        "<method name=\"PlanForget\">",
        "<method name=\"Forget\">",
        "<method name=\"Pending\">",
        "<method name=\"Settle\">",
        "<method name=\"Consolidation\">",
        "<method name=\"RunConsolidation\">",
        "<method name=\"Revert\">",
        "<method name=\"Rules\">",
        "<method name=\"SetRule\">",
        "<method name=\"RemoveRule\">",
        "<method name=\"Pause\">",
        "<method name=\"Resume\">",
        "<method name=\"Verify\">",
        "<method name=\"Rebuild\">",
        "<method name=\"Export\">",
        "<signal name=\"Recorded\">",
        "<signal name=\"Forgotten\">",
        "<signal name=\"PendingChanged\">",
        "<signal name=\"ConsolidationReady\">",
        "<signal name=\"StatusChanged\">",
        "<property name=\"Version\" type=\"u\" access=\"read\"/>",
    ];
    for member in members {
        assert!(xml.contains(member), "missing {member}");
    }
    let methods = xml.matches("<method ").count();
    assert_eq!(
        methods,
        4 + 6 + 18,
        "memory.md section 3.10 declares 28 methods"
    );
    assert_eq!(xml.matches("<signal ").count(), 5);
}

#[test]
fn the_multi_value_members_return_two_out_arguments() {
    let xml = introspection();
    for member in ["RecordBatch", "Propose"] {
        let at = xml
            .find(&format!("<method name=\"{member}\">"))
            .expect("member");
        let end = xml[at..].find("</method>").expect("end") + at;
        assert_eq!(
            xml[at..end].matches("direction=\"out\"").count(),
            2,
            "{member}"
        );
    }
    let export = xml.find("<method name=\"Export\">").expect("export");
    assert!(
        xml[export..].contains("type=\"h\""),
        "the export fd travels as h"
    );
}

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
    assert_eq!(names.len(), 10);
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
