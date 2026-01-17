use crate::stats::SessionStats;
use egui::RichText;

pub fn render_stats_window(ctx: &egui::Context, stats: &SessionStats, show_stats: &mut bool) {
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("stats_viewport"),
        egui::ViewportBuilder::default()
            .with_title("Session Statistics")
            .with_inner_size([450.0, 550.0]),
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

                ui.label("Correct QSOs:");
                ui.label(format!(
                    "{} ({:.1}%)",
                    analysis.correct_qsos, analysis.correct_rate
                ));
                ui.end_row();

                ui.label("Perfect QSOs:");
                ui.label(format!(
                    "{} ({:.1}%)",
                    analysis.perfect_qsos, analysis.perfect_rate
                ));
                ui.end_row();

                ui.label("Total Points:");
                ui.label(format!("{}", analysis.total_points));
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.label(
            RichText::new("Perfect = correct without using AGN")
                .small()
                .italics(),
        );

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

        // AGN Usage section
        ui.heading("AGN Usage");
        ui.add_space(8.0);

        egui::Grid::new("agn_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .show(ui, |ui| {
                ui.label("Callsign AGN:");
                ui.label(format!("{}", analysis.agn_callsign_count));
                ui.end_row();

                ui.label("Exchange AGN:");
                ui.label(format!("{}", analysis.agn_exchange_count));
                ui.end_row();

                ui.label("Total with AGN:");
                if analysis.total_qsos > 0 {
                    let agn_pct =
                        (analysis.agn_any_count as f32 / analysis.total_qsos as f32) * 100.0;
                    ui.label(format!("{} ({:.1}%)", analysis.agn_any_count, agn_pct));
                } else {
                    ui.label("0");
                }
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
                .num_columns(5)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Callsign").strong());
                    ui.label(RichText::new("Exchange").strong());
                    ui.label(RichText::new("WPM").strong());
                    ui.label(RichText::new("AGN").strong());
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

                        // AGN column
                        let agn_used = qso.used_agn_callsign || qso.used_agn_exchange;
                        if agn_used {
                            let mut agn_parts = Vec::new();
                            if qso.used_agn_callsign {
                                agn_parts.push("C");
                            }
                            if qso.used_agn_exchange {
                                agn_parts.push("X");
                            }
                            ui.label(
                                RichText::new(agn_parts.join(",")).color(egui::Color32::YELLOW),
                            );
                        } else {
                            ui.label("-");
                        }

                        // Result column
                        let is_correct = qso.callsign_correct && qso.exchange_correct;
                        let is_perfect = is_correct && !agn_used;
                        let (result_text, result_color) = if is_perfect {
                            ("OK", egui::Color32::GREEN)
                        } else if is_correct {
                            ("ok", egui::Color32::LIGHT_GREEN)
                        } else {
                            ("ERR", egui::Color32::RED)
                        };
                        ui.label(RichText::new(result_text).color(result_color));
                        ui.end_row();
                    }
                });

            ui.add_space(4.0);
            ui.label(
                RichText::new("AGN: C=callsign, X=exchange | OK=perfect, ok=correct with AGN")
                    .small()
                    .italics(),
            );
        }
    });
}
