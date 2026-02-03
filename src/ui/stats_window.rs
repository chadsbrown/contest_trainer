use crate::config::AppSettings;
use crate::export::export_session_stats;
use crate::stats::SessionStats;
use crate::ui::render_export_dialog;
use egui::RichText;

pub fn render_stats_window(
    ctx: &egui::Context,
    settings: &AppSettings,
    stats: &SessionStats,
    show_stats: &mut bool,
    export_result: &mut Option<String>,
) {
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of("stats_viewport"),
        egui::ViewportBuilder::default()
            .with_title("Session Statistics")
            .with_inner_size([450.0, 550.0]),
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| {
                // Centered Export Stats button at the top
                ui.vertical_centered(|ui| {
                    if ui.button("Export Stats").clicked() {
                        match export_session_stats(settings, stats) {
                            Ok(filename) => *export_result = Some(filename),
                            Err(e) => *export_result = Some(format!("Error: {}", e)),
                        }
                    }
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                render_stats_content(ui, stats);
            });

            // Render export dialog within this viewport
            render_export_dialog(ctx, export_result);

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

                ui.label("Total Points:");
                ui.label(format!("{}", analysis.total_points));
                ui.end_row();
            });

        ui.add_space(4.0);

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

        // Streaks section
        ui.heading("Streaks");
        ui.add_space(8.0);

        egui::Grid::new("streaks_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .show(ui, |ui| {
                ui.label("Current Clean:");
                ui.label(format!("{}", analysis.streaks.current_clean));
                ui.end_row();

                ui.label("Max Clean:");
                ui.label(format!("{}", analysis.streaks.max_clean));
                ui.end_row();

                ui.label("Current Error:");
                ui.label(format!("{}", analysis.streaks.current_error));
                ui.end_row();

                ui.label("Max Error:");
                ui.label(format!("{}", analysis.streaks.max_error));
                ui.end_row();
            });

        ui.add_space(4.0);
        ui.label(
            RichText::new("Clean = callsign and exchange both correct")
                .small()
                .italics(),
        );

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);

        // F5/F8 Usage section
        ui.heading("F5/F8 Usage");
        ui.add_space(8.0);

        egui::Grid::new("agn_grid")
            .num_columns(2)
            .spacing([40.0, 4.0])
            .show(ui, |ui| {
                ui.label("F5 (His Call):");
                ui.label(format!("{}", analysis.f5_callsign_count));
                ui.end_row();

                ui.label("F8 Callsign:");
                ui.label(format!("{}", analysis.agn_callsign_count));
                ui.end_row();

                ui.label("F8 Exchange:");
                ui.label(format!("{}", analysis.agn_exchange_count));
                ui.end_row();

                ui.label("Total with F8:");
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

        // WPM bucket accuracy
        ui.heading("WPM Accuracy (2-WPM buckets)");
        ui.add_space(8.0);

        if analysis.wpm_buckets.is_empty() {
            ui.label("No QSOs logged yet");
        } else {
            egui::Grid::new("wpm_bucket_grid")
                .num_columns(4)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Bucket").strong());
                    ui.label(RichText::new("Total").strong());
                    ui.label(RichText::new("Correct").strong());
                    ui.label(RichText::new("Accuracy").strong());
                    ui.end_row();

                    for bucket in &analysis.wpm_buckets {
                        ui.label(bucket.label.clone());
                        ui.label(format!("{}", bucket.total));
                        ui.label(format!("{}", bucket.correct));
                        ui.label(format!("{:.1}%", bucket.accuracy_pct));
                        ui.end_row();
                    }
                });
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
                        let is_perfect = is_correct && !agn_used && !qso.used_f5_callsign;
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
                RichText::new("AGN: C=callsign, X=exchange | ok=correct with AGN")
                    .small()
                    .italics(),
            );
        }
    });
}
