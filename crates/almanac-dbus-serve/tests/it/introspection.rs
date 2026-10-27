//! The checked-in introspection file is the interface the skeletons declare. A change to a
//! signature changes the file in the same commit.

use crate::support::*;
use almanac_dbus::encode_request;
use almanac_dbus_serve::*;
use std::path::PathBuf;

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
        "<method name=\"RecordDurable\">",
        "<method name=\"RecordBatch\">",
        "<method name=\"ExplainFile\">",
        "<method name=\"Mark\">",
        "<method name=\"Search\">",
        "<method name=\"Facts\">",
        "<method name=\"Inject\">",
        "<method name=\"Recent\">",
        "<method name=\"Entries\">",
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
        "<method name=\"ApplyConsolidation\">",
        "<method name=\"DiscardConsolidation\">",
        "<method name=\"Rules\">",
        "<method name=\"SetRule\">",
        "<method name=\"RemoveRule\">",
        "<method name=\"Pause\">",
        "<method name=\"Resume\">",
        "<method name=\"Verify\">",
        "<method name=\"Rebuild\">",
        "<method name=\"Sweep\">",
        "<method name=\"RemoveSpace\">",
        "<method name=\"Export\">",
        "<method name=\"ConfirmSettings\">",
        "<method name=\"PendingSettings\">",
        "<signal name=\"Recorded\">",
        "<signal name=\"Forgotten\">",
        "<signal name=\"PendingChanged\">",
        "<signal name=\"ConsolidationReady\">",
        "<signal name=\"StatusChanged\">",
        "<property name=\"Confinement\" type=\"s\" access=\"read\"/>",
        "<property name=\"Version\" type=\"u\" access=\"read\"/>",
    ];
    for member in members {
        assert!(xml.contains(member), "missing {member}");
    }
    let methods = xml.matches("<method ").count();
    assert_eq!(
        methods,
        5 + 9 + 24,
        "memory.md section 3.10 declares 28 methods; the companion amendment adds Inject and Recent, the retention sweep adds Sweep, review-mode consolidation adds ApplyConsolidation and DiscardConsolidation, the durable session log adds RecordDurable and Entries, a removed Space's memories add RemoveSpace, the loosened-settings consent adds ConfirmSettings and PendingSettings"
    );
    assert_eq!(xml.matches("<signal ").count(), 5);
    // And the other direction: every member the codec encodes is declared.
    for request in requests() {
        let member = encode_request(&request).expect("encode").member;
        assert!(
            xml.contains(&format!("<method name=\"{member}\">")),
            "the codec encodes {member}, which the introspection does not declare"
        );
    }
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
