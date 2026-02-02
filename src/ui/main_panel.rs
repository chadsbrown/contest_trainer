use crate::app::{ContestApp, InputField, Score};
use crate::contest::normalize_exchange_input;
use crate::state::StatusColor;
use egui::{Color32, RichText, Vec2};

pub fn render_main_panel(ui: &mut egui::Ui, app: &mut ContestApp) {
    // Contest type display
    ui.horizontal_top(|ui| {
        ui.label(RichText::new("Contest:").strong());
        ui.label(app.contest.display_name());
    });

    ui.add_space(4.0);

    if let Some(notice) = app.settings_notice.clone() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(notice).color(Color32::YELLOW));
            if ui.button("Dismiss").clicked() {
                app.settings_notice = None;
            }
        });
        ui.add_space(4.0);
    }

    // Top bar: Score display
    render_score_bar(ui, &app.score, app.settings.user.wpm);

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Status indicator
    if app.settings.user.show_status_line {
        render_status(ui, app);
        ui.add_space(12.0);
    }

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

fn render_status(ui: &mut egui::Ui, app: &ContestApp) {
    let (status_text, status_color) = app.get_status();
    let color = match status_color {
        StatusColor::Gray => Color32::GRAY,
        StatusColor::Yellow => Color32::YELLOW,
        StatusColor::LightBlue => Color32::LIGHT_BLUE,
        StatusColor::Green => Color32::from_rgb(100, 200, 100),
        StatusColor::Orange => Color32::from_rgb(255, 165, 0),
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Status:").strong());
        ui.label(RichText::new(status_text).color(color));
    });
}

fn render_input_fields(ui: &mut egui::Ui, app: &mut ContestApp) {
    let exchange_fields = app.contest.exchange_fields();
    if app.exchange_inputs.len() != exchange_fields.len() {
        let defaults = app.exchange_default_values();
        let mut next = Vec::with_capacity(exchange_fields.len());
        for idx in 0..exchange_fields.len() {
            let value = app
                .exchange_inputs
                .get(idx)
                .cloned()
                .unwrap_or_else(|| defaults.get(idx).cloned().unwrap_or_default());
            next.push(value);
        }
        app.exchange_inputs = next;
    }

    let label_size = (app.settings.user.font_size - 4.0).max(8.0);
    egui::Grid::new("input_fields_grid")
        .num_columns(exchange_fields.len() + 1)
        .spacing([6.0, 2.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Call").size(label_size));
            for field in exchange_fields.iter() {
                ui.label(RichText::new(field.label).size(label_size));
            }
            ui.end_row();

            let mut call_edit = egui::TextEdit::singleline(&mut app.callsign_input)
                .font(egui::TextStyle::Monospace);
            if app.settings.user.show_main_hints {
                call_edit = call_edit.hint_text("Callsign");
            }
            let call_response = ui.add_sized(Vec2::new(120.0, 24.0), call_edit);

            if call_response.changed() {
                app.callsign_input = app.callsign_input.to_uppercase();
            }

            if app.current_field == InputField::Callsign && !app.show_settings {
                call_response.request_focus();
            }
            if call_response.clicked() {
                app.current_field = InputField::Callsign;
            }

            for (idx, field) in exchange_fields.iter().enumerate() {
                let width_px =
                    exchange_field_width(ui, field.width_chars, app.settings.user.font_size);
                let mut exchange_edit = egui::TextEdit::singleline(&mut app.exchange_inputs[idx])
                    .font(egui::TextStyle::Monospace);
                if app.settings.user.show_main_hints {
                    exchange_edit = exchange_edit.hint_text(field.placeholder);
                }
                let response = ui.add_sized(Vec2::new(width_px, 24.0), exchange_edit);
                if response.changed() {
                    let normalized =
                        normalize_exchange_input(&app.exchange_inputs[idx], field.kind);
                    app.exchange_inputs[idx] = normalized;
                }

                if app.current_field == InputField::Exchange(idx) && !app.show_settings {
                    response.request_focus();
                }
                if response.clicked() {
                    app.current_field = InputField::Exchange(idx);
                    app.last_exchange_field_index = idx;
                }
            }
            ui.end_row();
        });
}

fn exchange_field_width(ui: &egui::Ui, width_chars: u8, font_size: f32) -> f32 {
    let _ = ui;
    let char_width = (font_size * 0.6).max(6.0);
    char_width * width_chars as f32 + 8.0
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
        ui.label("Stop");
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
