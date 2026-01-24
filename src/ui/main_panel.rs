use crate::app::{ContestApp, ContestState, InputField, RadioId, RadioState, Score};
use egui::{Color32, RichText, Vec2};

pub fn render_main_panel(ui: &mut egui::Ui, app: &mut ContestApp) {
    // Contest type display with 2BSIQ indicator
    ui.horizontal(|ui| {
        ui.label(RichText::new("Contest:").strong());
        ui.label(app.settings.contest.contest_type.display_name());

        if app.settings.user.two_bsiq_enabled {
            ui.add_space(20.0);
            ui.label(
                RichText::new("[2BSIQ Mode]")
                    .color(Color32::LIGHT_BLUE)
                    .strong(),
            );
        }
    });

    ui.add_space(4.0);

    // Top bar: Score display
    render_score_bar(ui, &app.score, app.settings.user.wpm);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    if app.settings.user.two_bsiq_enabled {
        // 2BSIQ mode: show two radio panels side by side
        render_dual_radio_panels(ui, app);
    } else {
        // Single radio mode (original behavior)
        render_single_radio_panel(ui, app);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Function key hints
    render_key_hints(ui);

    // 2BSIQ-specific key hints
    if app.settings.user.two_bsiq_enabled {
        ui.add_space(4.0);
        render_2bsiq_key_hints(ui, app.stereo_enabled, app.focused_radio);
    }

    ui.add_space(8.0);

    // Last QSO info (show from focused radio in 2BSIQ mode)
    let last_qso = if app.settings.user.two_bsiq_enabled {
        match app.focused_radio {
            RadioId::Radio1 => app.radio1.last_qso_result.as_ref(),
            RadioId::Radio2 => app.radio2.last_qso_result.as_ref(),
        }
    } else {
        app.last_qso_result.as_ref()
    };

    if let Some(last) = last_qso {
        render_last_qso(ui, last);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Bottom buttons
    ui.horizontal(|ui| {
        if ui.button("Reset Stats").clicked() {
            app.reset_score();
            app.session_stats.clear();
        }

        ui.add_space(10.0);

        let noise_label = if app.noise_enabled {
            "Toggle Static (ON)"
        } else {
            "Toggle Static (OFF)"
        };
        if ui.button(noise_label).clicked() {
            app.toggle_noise();
        }

        ui.add_space(10.0);

        if ui.button("Session Stats").clicked() {
            app.show_stats = !app.show_stats;
        }
    });
}

/// Render the original single-radio panel
fn render_single_radio_panel(ui: &mut egui::Ui, app: &mut ContestApp) {
    // Status indicator
    if app.settings.user.show_status_line {
        render_status(ui, &app.state);
        ui.add_space(12.0);
    }

    // Input fields
    render_input_fields(ui, app);

    ui.add_space(12.0);
    ui.separator();
}

/// Render dual radio panels for 2BSIQ mode
fn render_dual_radio_panels(ui: &mut egui::Ui, app: &mut ContestApp) {
    ui.horizontal(|ui| {
        // Radio 1 panel (left)
        let r1_focused = app.focused_radio == RadioId::Radio1;
        render_radio_panel(
            ui,
            RadioId::Radio1,
            &app.radio1,
            r1_focused,
            app.settings.user.show_status_line,
            app.show_settings,
            &app.settings.contest.contest_type,
        );

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(16.0);

        // Radio 2 panel (right)
        let r2_focused = app.focused_radio == RadioId::Radio2;
        render_radio_panel(
            ui,
            RadioId::Radio2,
            &app.radio2,
            r2_focused,
            app.settings.user.show_status_line,
            app.show_settings,
            &app.settings.contest.contest_type,
        );
    });

    ui.add_space(12.0);
    ui.separator();
}

/// Render a single radio panel for 2BSIQ mode
fn render_radio_panel(
    ui: &mut egui::Ui,
    radio_id: RadioId,
    radio_state: &RadioState,
    is_focused: bool,
    show_status: bool,
    settings_open: bool,
    contest_type: &crate::contest::ContestType,
) {
    let panel_width = 280.0;

    // Create a frame with highlighting for focused radio
    let frame = if is_focused {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .inner_margin(8.0)
            .corner_radius(4.0)
    } else {
        egui::Frame::new().inner_margin(8.0)
    };

    frame.show(ui, |ui| {
        ui.set_min_width(panel_width);

        // Radio header with focus indicator
        ui.horizontal(|ui| {
            let (label, channel) = match radio_id {
                RadioId::Radio1 => ("RADIO 1", "(Left)"),
                RadioId::Radio2 => ("RADIO 2", "(Right)"),
            };

            if is_focused {
                ui.label(RichText::new("▶").color(Color32::GREEN));
            } else {
                ui.label(RichText::new(" ").monospace());
            }

            ui.label(RichText::new(label).strong());
            ui.label(RichText::new(channel).weak().small());
        });

        ui.add_space(4.0);

        // Status indicator
        if show_status {
            render_status(ui, &radio_state.state);
            ui.add_space(8.0);
        }

        // Input fields (read-only display in 2BSIQ mode for now)
        render_radio_input_fields(ui, radio_state, is_focused, settings_open, contest_type);
    });
}

/// Render input fields for a radio in 2BSIQ mode
fn render_radio_input_fields(
    ui: &mut egui::Ui,
    radio_state: &RadioState,
    _is_focused: bool,
    _settings_open: bool,
    contest_type: &crate::contest::ContestType,
) {
    // For now, display the current values (editing will come in later phases)
    ui.horizontal(|ui| {
        ui.label(RichText::new("Call:").strong());
        let call_display = if radio_state.callsign_input.is_empty() {
            "________".to_string()
        } else {
            format!("{:8}", radio_state.callsign_input)
        };
        ui.label(RichText::new(call_display).monospace());
    });

    ui.horizontal(|ui| {
        ui.label(RichText::new("Exch:").strong());
        let exch_display = if radio_state.exchange_input.is_empty() {
            "________".to_string()
        } else {
            format!("{:8}", radio_state.exchange_input)
        };
        ui.label(RichText::new(exch_display).monospace());
    });

    // Show exchange format hint
    ui.horizontal(|ui| {
        let hint = match contest_type {
            crate::contest::ContestType::CqWw => "RST ZONE",
            crate::contest::ContestType::Sweepstakes => "NR PREC CK SEC",
            crate::contest::ContestType::Cwt => "NAME NUM",
        };
        ui.label(RichText::new(hint).small().weak());
    });
}

/// Render 2BSIQ-specific key hints
fn render_2bsiq_key_hints(ui: &mut egui::Ui, stereo_enabled: bool, focused_radio: RadioId) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Pause").strong().monospace());
        ui.label("Swap");
        ui.add_space(8.0);

        ui.label(RichText::new("`").strong().monospace());
        ui.label("Stereo");
        ui.add_space(8.0);

        ui.label(RichText::new("Ctrl+←/→").strong().monospace());
        ui.label("Focus");
        ui.add_space(16.0);

        // Status indicators
        let stereo_text = if stereo_enabled { "STEREO" } else { "MONO" };
        let stereo_color = if stereo_enabled {
            Color32::GREEN
        } else {
            Color32::YELLOW
        };
        ui.label(RichText::new(format!("[{}]", stereo_text)).color(stereo_color));

        ui.add_space(8.0);

        let focus_text = match focused_radio {
            RadioId::Radio1 => "R1",
            RadioId::Radio2 => "R2",
        };
        ui.label(RichText::new(format!("[Focus: {}]", focus_text)).color(Color32::LIGHT_BLUE));
    });
}

fn render_score_bar(ui: &mut egui::Ui, score: &Score, user_wpm: u8) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("QSOs:").strong());
        ui.label(format!("{}", score.qso_count));

        ui.add_space(20.0);

        ui.label(RichText::new("Points:").strong());
        ui.label(format!("{}", score.total_points));

        ui.add_space(20.0);

        ui.label(RichText::new("Rate:").strong());
        ui.label(format!("{}/hr", score.hourly_rate()));

        ui.add_space(20.0);

        ui.label(RichText::new("Run WPM:").strong());
        ui.label(format!("{}", user_wpm));
    });
}

fn render_status(ui: &mut egui::Ui, state: &ContestState) {
    let (status_text, status_color) = match state {
        ContestState::Idle => ("Press F1/Enter to call CQ", Color32::GRAY),
        ContestState::CallingCq => ("Calling CQ...", Color32::YELLOW),
        ContestState::WaitingForCallers => ("Waiting for callers...", Color32::LIGHT_BLUE),
        ContestState::StationsCalling { .. } => {
            ("Station calling - enter callsign", Color32::GREEN)
        }
        ContestState::QueryingPartial { .. } => ("Querying partial...", Color32::YELLOW),
        ContestState::WaitingForPartialResponse { .. } => {
            ("Waiting for response...", Color32::LIGHT_BLUE)
        }
        ContestState::SendingExchange { .. } => ("Sending exchange...", Color32::YELLOW),
        ContestState::WaitingToSendExchange { .. } => ("Sending exchange...", Color32::YELLOW),
        ContestState::ReceivingExchange { .. } => {
            ("Receiving exchange - type what you copy", Color32::GREEN)
        }
        ContestState::SendingAgn { .. } => ("Requesting repeat...", Color32::YELLOW),
        ContestState::WaitingForAgn { .. } => ("Waiting for repeat...", Color32::LIGHT_BLUE),
        ContestState::SendingCallsignAgn { .. } => ("Requesting repeat...", Color32::YELLOW),
        ContestState::WaitingForCallsignAgn { .. } => {
            ("Waiting for repeat...", Color32::LIGHT_BLUE)
        }
        ContestState::CallerRequestingAgn { .. } => {
            ("Station requesting repeat...", Color32::YELLOW)
        }
        ContestState::WaitingForUserExchangeRepeat { .. } => {
            ("Press F2 to resend exchange", Color32::GREEN)
        }
        ContestState::QsoComplete { .. } => (
            "QSO logged! Press F1 for next CQ",
            Color32::from_rgb(100, 200, 100),
        ),
        ContestState::WaitingForTailEnder { .. } => (
            "QSO logged! Press F1 for next CQ",
            Color32::from_rgb(100, 200, 100),
        ),
        ContestState::SendingCallCorrection { .. } => {
            ("Station correcting callsign...", Color32::YELLOW)
        }
        ContestState::WaitingToSendCallCorrection { .. } => {
            ("Station correcting callsign...", Color32::YELLOW)
        }
        ContestState::WaitingForCallCorrection { .. } => {
            ("Correct callsign and resend", Color32::GREEN)
        }
        ContestState::SendingExchangeWillCorrect { .. } => ("Sending exchange...", Color32::YELLOW),
        ContestState::SendingCallsignAgnFromCorrection { .. } => {
            ("Requesting callsign repeat...", Color32::YELLOW)
        }
        ContestState::WaitingForCallsignAgnFromCorrection { .. } => {
            ("Requesting callsign repeat...", Color32::YELLOW)
        }
        ContestState::SendingCorrectionRepeat { .. } => {
            ("Station repeating callsign...", Color32::YELLOW)
        }
        ContestState::QueryingPartialFromCorrection { .. } => {
            ("Querying partial...", Color32::YELLOW)
        }
        ContestState::WaitingForPartialResponseFromCorrection { .. } => {
            ("Querying partial...", Color32::YELLOW)
        }
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Status:").strong());
        ui.label(RichText::new(status_text).color(status_color));
    });
}

fn render_input_fields(ui: &mut egui::Ui, app: &mut ContestApp) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Call:").strong());
        let call_response = ui.add_sized(
            Vec2::new(120.0, 24.0),
            egui::TextEdit::singleline(&mut app.callsign_input)
                .font(egui::TextStyle::Monospace)
                .hint_text("Callsign"),
        );

        if call_response.changed() {
            app.callsign_input = app.callsign_input.to_uppercase();
        }

        if app.current_field == InputField::Callsign && !app.show_settings {
            call_response.request_focus();
        }

        ui.add_space(20.0);

        ui.label(RichText::new("Exch:").strong());
        let exch_response = ui.add_sized(
            Vec2::new(150.0, 24.0),
            egui::TextEdit::singleline(&mut app.exchange_input)
                .font(egui::TextStyle::Monospace)
                .hint_text("Exchange"),
        );

        if exch_response.changed() {
            app.exchange_input = app.exchange_input.to_uppercase();
        }

        if app.current_field == InputField::Exchange && !app.show_settings {
            exch_response.request_focus();
        }
    });

    // Show expected exchange format hint
    ui.horizontal(|ui| {
        ui.add_space(50.0);
        let hint = match app.settings.contest.contest_type {
            crate::contest::ContestType::CqWw => "Format: RST ZONE (e.g., 599 05)",
            crate::contest::ContestType::Sweepstakes => "Format: NR PREC CK SEC (e.g., 42 A 99 CT)",
            crate::contest::ContestType::Cwt => "Format: NAME NUMBER (e.g., BOB 123 or JOE TX)",
        };
        ui.label(RichText::new(hint).small().weak());
    });
}

fn render_key_hints(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("F1").strong().monospace());
        ui.label("CQ");
        ui.add_space(10.0);

        ui.label(RichText::new("F2").strong().monospace());
        ui.label("Exchange");
        ui.add_space(10.0);

        ui.label(RichText::new("F3").strong().monospace());
        ui.label("TU");
        ui.add_space(10.0);

        ui.label(RichText::new("F5").strong().monospace());
        ui.label("His Call");
        ui.add_space(10.0);

        ui.label(RichText::new("F8").strong().monospace());
        ui.label("?");
        ui.add_space(10.0);

        ui.label(RichText::new("F12").strong().monospace());
        ui.label("Wipe");
        ui.add_space(10.0);

        ui.label(RichText::new("Enter").strong().monospace());
        ui.label("Submit");
        ui.add_space(10.0);

        ui.label(RichText::new("Esc").strong().monospace());
        ui.label("Clear");
    });
}

fn render_last_qso(ui: &mut egui::Ui, result: &crate::app::QsoResult) {
    ui.add_space(4.0);

    let call_indicator = if result.callsign_correct { "OK" } else { "X" };
    let exch_indicator = if result.exchange_correct { "OK" } else { "X" };

    let call_color = if result.callsign_correct {
        Color32::GREEN
    } else {
        Color32::RED
    };
    let exch_color = if result.exchange_correct {
        Color32::GREEN
    } else {
        Color32::RED
    };

    ui.horizontal(|ui| {
        ui.label("Last QSO:");
        ui.label(&result.callsign);
        ui.label(RichText::new(format!("Call: {}", call_indicator)).color(call_color));
        ui.label(RichText::new(format!("Exch: {}", exch_indicator)).color(exch_color));
        if result.points > 0 {
            ui.label(RichText::new(format!("+{} pts", result.points)).color(Color32::GREEN));
        }
    });

    // Show correct values if wrong
    if !result.callsign_correct || !result.exchange_correct {
        ui.horizontal(|ui| {
            ui.add_space(60.0);
            ui.label(
                RichText::new(format!(
                    "Expected: {} {}",
                    result.expected_call, result.expected_exchange
                ))
                .weak(),
            );
        });
    }
}
