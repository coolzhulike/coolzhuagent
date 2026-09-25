use computer_use::{
    anchor_to_physical_pixel, default_regression_scenarios, standard_resolution_cases,
};
use runtime::{ConversationMessage, Session};

#[test]
fn computer_use_anchor_matrix_is_available_from_root_workspace() {
    let scenario = &default_regression_scenarios()[0];
    let resolution = standard_resolution_cases()
        .iter()
        .find(|case| case.name == "fhd-100")
        .copied()
        .expect("fhd-100 resolution should exist");

    let point = anchor_to_physical_pixel(scenario.anchor, resolution)
        .expect("anchor should map to physical point");

    assert_eq!(point, (86, 173));
}

#[test]
fn vision_grounding_parser_is_available_from_root_workspace() {
    assert_eq!(
        vision::parse_relative_point("target center: [0.25, 0.75]"),
        Some((0.25, 0.75))
    );
}

#[test]
fn server_app_can_be_constructed_from_root_workspace() {
    let _app = server::app(server::AppState::default());
}

#[test]
fn runtime_session_message_type_is_available_from_root_workspace() {
    let mut session = Session::new();
    session
        .messages
        .push(ConversationMessage::user_text("hello coolzhu"));

    assert_eq!(session.messages.len(), 1);
}
