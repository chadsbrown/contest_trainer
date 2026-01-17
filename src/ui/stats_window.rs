use crate::stats::SessionStats;
use egui::RichText;

pub fn render_stats_window(ctx: &egui::Context, stats: &SessionStats, show_stats: &mut bool) {
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("stats_viewport"),
        egui::ViewportBuilder::default()
            .with_title("Session Statistics")
            .with_inner_size([450.0, 500.0]),
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_stats_content(ui, stats);
            });

            if ctx.input(|i| i.viewport().close_requested()) {
                *show_stats = false;
            }
        },
    );
}

fn render_stats_content(ui: &mut egui::Ui, stats: &SessionStats) {
    let analysis = stats.analyze();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Summary section
        ui.heading("Session Summary");
        ui.add_space(8.0);

        egui::Grid::new("summary_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .show(ui, |ui| {
                ui.label("Total QSOs:");
                ui.label(format!("{}", analysis.total_qsos));
                ui.end_row();

                ui.label("Perfect QSOs:");
                ui.label(format!(
                    "{} ({:.1}%)",
                    analysis.perfect_qsos, analysis.overall_accuracy
                ));
                ui.end_row();

                ui.label("Total Points:");
                ui.label(format!("{}", analysis.total_points));
                ui.end_row();
            });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Accuracy section
        ui.heading("Accuracy");
        ui.add_space(8.0);

        egui::Grid::new("accuracy_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .show(ui, |ui| {
                ui.label("Callsign Accuracy:");
                ui.label(format!(
                    "{}/{} ({:.1}%)",
                    analysis.correct_callsigns, analysis.total_qsos, analysis.callsign_accuracy
                ));
                ui.end_row();

                ui.label("Exchange Accuracy:");
                ui.label(format!(
                    "{}/{} ({:.1}%)",
                    analysis.correct_exchanges, analysis.total_qsos, analysis.exchange_accuracy
                ));
                ui.end_row();
            });

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // WPM section
        ui.heading("Calling Station Speed");
        ui.add_space(8.0);

        if analysis.total_qsos > 0 {
            egui::Grid::new("wpm_grid")
                .num_columns(2)
                .spacing([40.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Average WPM:");
                    ui.label(format!("{:.1}", analysis.avg_station_wpm));
                    ui.end_row();

                    ui.label("WPM Range:");
                    ui.label(format!(
                        "{} - {}",
                        analysis.min_station_wpm, analysis.max_station_wpm
                    ));
                    ui.end_row();
                });
        } else {
            ui.label("No QSOs logged yet");
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Character error analysis
        ui.heading("Character Error Analysis");
        ui.add_space(8.0);

        if analysis.char_error_rates.is_empty() {
            ui.label("Not enough data for character analysis");
        } else {
            ui.label(RichText::new("Characters with highest error rates:").small());
            ui.add_space(4.0);

            egui::Grid::new("char_error_grid")
                .num_columns(3)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Char").strong());
                    ui.label(RichText::new("Error Rate").strong());
                    ui.label(RichText::new("Samples").strong());
                    ui.end_row();

                    for (ch, error_rate, count) in analysis
                        .char_error_rates
                        .iter()
                        .filter(|(_, rate, _)| *rate > 0.0)
                        .take(10)
                    {
                        let char_display = if *ch == ' ' {
                            "[space]".to_string()
                        } else {
                            ch.to_string()
                        };
                        ui.label(RichText::new(char_display).monospace());
                        ui.label(format!("{:.1}%", error_rate));
                        ui.label(format!("{}", count));
                        ui.end_row();
                    }
                });
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // Recent QSOs
        ui.heading("Recent QSOs");
        ui.add_space(8.0);

        if stats.qsos.is_empty() {
            ui.label("No QSOs logged yet");
        } else {
            egui::Grid::new("qso_grid")
                .num_columns(4)
                .spacing([15.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Callsign").strong());
                    ui.label(RichText::new("Exchange").strong());
                    ui.label(RichText::new("WPM").strong());
                    ui.label(RichText::new("Result").strong());
                    ui.end_row();

                    // Show last 15 QSOs in reverse order
                    for qso in stats.qsos.iter().rev().take(15) {
                        // Callsign column
                        let call_color = if qso.callsign_correct {
                            egui::Color32::GREEN
                        } else {
                            egui::Color32::RED
                        };
                        ui.label(
                            RichText::new(&qso.entered_callsign)
                                .monospace()
                                .color(call_color),
                        );

                        // Exchange column
                        let exch_color = if qso.exchange_correct {
                            egui::Color32::GREEN
                        } else {
                            egui::Color32::RED
                        };
                        ui.label(
                            RichText::new(&qso.entered_exchange)
                                .monospace()
                                .color(exch_color),
                        );

                        // WPM column
                        ui.label(format!("{}", qso.station_wpm));

                        // Result column
                        let result_text = if qso.callsign_correct && qso.exchange_correct {
                            "OK"
                        } else {
                            "ERR"
                        };
                        let result_color = if qso.callsign_correct && qso.exchange_correct {
                            egui::Color32::GREEN
                        } else {
                            egui::Color32::RED
                        };
                        ui.label(RichText::new(result_text).color(result_color));
                        ui.end_row();
                    }
                });
        }
    });
}
