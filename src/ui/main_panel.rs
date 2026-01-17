use crate::app::{ContestApp, ContestState, InputField, Score};
use egui::{Color32, RichText, Vec2};

pub fn render_main_panel(ui: &mut egui::Ui, app: &mut ContestApp) {
    // Contest type display
    ui.horizontal(|ui| {
        ui.label(RichText::new("Contest:").strong());
        ui.label(app.settings.contest.contest_type.display_name());
    });

    ui.add_space(4.0);

    // Top bar: Score display
    render_score_bar(ui, &app.score, app.settings.user.wpm);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Status indicator
    render_status(ui, &app.state);

    ui.add_space(12.0);

    // Input fields
    render_input_fields(ui, app);

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Function key hints
    render_key_hints(ui);

    ui.add_space(8.0);

    // Last QSO info
    if let Some(ref last) = app.last_qso_result {
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
        ContestState::QsoComplete { .. } => (
            "QSO logged! Press F1 for next CQ",
            Color32::from_rgb(100, 200, 100),
        ),
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
                .hint_text("callsign"),
        );

        if app.current_field == InputField::Callsign && !app.show_settings {
            call_response.request_focus();
        }

        ui.add_space(20.0);

        ui.label(RichText::new("Exch:").strong());
        let exch_response = ui.add_sized(
            Vec2::new(150.0, 24.0),
            egui::TextEdit::singleline(&mut app.exchange_input)
                .font(egui::TextStyle::Monospace)
                .hint_text("exchange"),
        );

        if app.current_field == InputField::Exchange && !app.show_settings {
            exch_response.request_focus();
        }
    });

    // Show expected exchange format hint
    ui.horizontal(|ui| {
        ui.add_space(50.0);
        let hint = match app.settings.contest.contest_type {
            crate::contest::ContestType::CqWw => "Format: RST ZONE (e.g., 599 05)",
            crate::contest::ContestType::NaSprint => "Format: NR NAME QTH (e.g., 123 BOB TX)",
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
        ui.label("AGN?");
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
