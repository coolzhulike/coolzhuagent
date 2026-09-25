use std::collections::HashSet;

use serde_json::Value;

const S0_GOLDEN_FIXTURE: &str = include_str!("fixtures/s0-golden/manifest.json");

#[test]
fn s0_golden_fixture_is_synthetic_complete_and_side_effect_free() {
    let fixture: Value = serde_json::from_str(S0_GOLDEN_FIXTURE).expect("fixture JSON must parse");

    assert_eq!(
        fixture["schema_version"].as_str(),
        Some("coolzhu.s0-golden-fixture.v1")
    );
    assert_eq!(
        fixture["data_classification"].as_str(),
        Some("synthetic-only")
    );
    assert_eq!(
        fixture["side_effect_policy"].as_str(),
        Some("recorded-only")
    );

    let scenarios = fixture["scenarios"]
        .as_array()
        .expect("fixture must contain scenarios");
    let required = [
        "BASE-STREAM-TOOL-PAIR",
        "BASE-NONSTREAM-TOOL-PAIR",
        "BASE-IMAGE-ROUTING",
        "BASE-EMPTY-SUMMARY",
        "BASE-CROSS-TURN-FILTER",
        "BASE-LONG-FILE-TASK",
    ];
    let ids = scenarios
        .iter()
        .map(|scenario| {
            scenario["id"]
                .as_str()
                .expect("scenario id must be a string")
        })
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), scenarios.len(), "scenario ids must be unique");
    for id in required {
        assert!(ids.contains(id), "missing required S0 scenario: {id}");
    }

    let serialized = fixture.to_string().to_ascii_lowercase();
    for forbidden in ["api_key", "authorization", "bearer ", "c:\\\\users\\\\"] {
        assert!(
            !serialized.contains(forbidden),
            "synthetic fixture must not contain {forbidden:?}"
        );
    }
}

#[test]
fn s0_golden_fixture_replays_only_recorded_tool_results() {
    let fixture: Value = serde_json::from_str(S0_GOLDEN_FIXTURE).expect("fixture JSON must parse");
    for scenario in fixture["scenarios"]
        .as_array()
        .expect("fixture must contain scenarios")
    {
        let mut outstanding_calls = HashSet::new();
        for event in scenario["events"]
            .as_array()
            .expect("scenario events must be an array")
        {
            match event["type"].as_str() {
                Some("assistant.tool_call") | Some("tool.intent") => {
                    let call_id = event["call_id"]
                        .as_str()
                        .expect("tool event must have a call_id");
                    assert!(
                        outstanding_calls.insert(call_id),
                        "a fixture cannot issue the same call id twice: {call_id}"
                    );
                }
                Some("tool.result") => {
                    let call_id = event["call_id"]
                        .as_str()
                        .expect("tool result must have a call_id");
                    assert!(
                        outstanding_calls.remove(call_id),
                        "tool result must match a recorded call: {call_id}"
                    );
                }
                _ => {}
            }
        }
        assert!(
            outstanding_calls.is_empty(),
            "fixture leaves a tool call unresolved: {}",
            scenario["id"]
        );
    }
}
