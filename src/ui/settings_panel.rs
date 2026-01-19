use crate::config::AppSettings;
use crate::contest::ContestType;
use egui::RichText;

pub fn render_settings_panel(
    ui: &mut egui::Ui,
    settings: &mut AppSettings,
    settings_changed: &mut bool,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // User Settings
        egui::CollapsingHeader::new(RichText::new("User Settings").strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Your Callsign:");
                    if ui
                        .text_edit_singleline(&mut settings.user.callsign)
                        .changed()
                    {
                        settings.user.callsign = settings.user.callsign.to_uppercase();
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Your Name:");
                    if ui.text_edit_singleline(&mut settings.user.name).changed() {
                        settings.user.name = settings.user.name.to_uppercase();
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("CQ Zone:");
                    if ui
                        .add(egui::DragValue::new(&mut settings.user.zone).range(1..=40))
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Section/Exchange:");
                    if ui
                        .text_edit_singleline(&mut settings.user.section)
                        .changed()
                    {
                        settings.user.section = settings.user.section.to_uppercase();
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Your WPM:");
                    if ui
                        .add(egui::Slider::new(&mut settings.user.wpm, 15..=50))
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Font Size:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.user.font_size, 10.0..=24.0)
                                .fixed_decimals(0),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("AGN Message:");
                    if ui
                        .text_edit_singleline(&mut settings.user.agn_message)
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                if ui
                    .checkbox(&mut settings.user.show_status_line, "Show Status Line")
                    .changed()
                {
                    *settings_changed = true;
                }
            });

        ui.add_space(8.0);

        // Contest Settings
        egui::CollapsingHeader::new(RichText::new("Contest Settings").strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Contest Type:");
                    egui::ComboBox::from_id_salt("contest_type")
                        .selected_text(format!("{:?}", settings.contest.contest_type))
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut settings.contest.contest_type,
                                    ContestType::CqWw,
                                    "CQ World Wide",
                                )
                                .changed()
                            {
                                *settings_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut settings.contest.contest_type,
                                    ContestType::NaSprint,
                                    "NA Sprint",
                                )
                                .changed()
                            {
                                *settings_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut settings.contest.contest_type,
                                    ContestType::Sweepstakes,
                                    "ARRL Sweepstakes",
                                )
                                .changed()
                            {
                                *settings_changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut settings.contest.contest_type,
                                    ContestType::Cwt,
                                    "CWT",
                                )
                                .changed()
                            {
                                *settings_changed = true;
                            }
                        });
                });

                ui.horizontal(|ui| {
                    ui.label("Callsign File:");
                    if ui
                        .text_edit_singleline(&mut settings.contest.callsign_file)
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                if settings.contest.contest_type == ContestType::Cwt {
                    ui.horizontal(|ui| {
                        ui.label("CWT Callsign File:");
                        if ui
                            .text_edit_singleline(&mut settings.contest.cwt_callsign_file)
                            .changed()
                        {
                            *settings_changed = true;
                        }
                    });
                }

                ui.horizontal(|ui| {
                    ui.label("CQ Message:");
                    if ui
                        .text_edit_singleline(&mut settings.contest.cq_message)
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });
            });

        ui.add_space(8.0);

        // Simulation Settings
        egui::CollapsingHeader::new(RichText::new("Simulation Settings").strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Max Simultaneous Stations:");
                    if ui
                        .add(egui::Slider::new(
                            &mut settings.simulation.max_simultaneous_stations,
                            1..=5,
                        ))
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Station Probability:");
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut settings.simulation.station_probability,
                                0.1..=1.0,
                            )
                            .fixed_decimals(2),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("WPM Range:");
                    let mut changed = false;
                    changed |= ui
                        .add(egui::DragValue::new(&mut settings.simulation.wpm_min).range(10..=50))
                        .changed();
                    ui.label("-");
                    changed |= ui
                        .add(egui::DragValue::new(&mut settings.simulation.wpm_max).range(10..=50))
                        .changed();
                    if changed {
                        // Ensure min <= max
                        if settings.simulation.wpm_min > settings.simulation.wpm_max {
                            settings.simulation.wpm_max = settings.simulation.wpm_min;
                        }
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Frequency Spread (Hz):");
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut settings.simulation.frequency_spread_hz,
                                0.0..=1000.0,
                            )
                            .fixed_decimals(0),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Signal Strength Range:");
                    let mut changed = false;
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut settings.simulation.amplitude_min, 0.1..=1.0)
                                .fixed_decimals(2)
                                .text("min"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut settings.simulation.amplitude_max, 0.1..=1.0)
                                .fixed_decimals(2)
                                .text("max"),
                        )
                        .changed();
                    if changed {
                        if settings.simulation.amplitude_min > settings.simulation.amplitude_max {
                            settings.simulation.amplitude_max = settings.simulation.amplitude_min;
                        }
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Caller Needs Repeat Probability:");
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut settings.simulation.agn_request_probability,
                                0.0..=1.0,
                            )
                            .fixed_decimals(2),
                        )
                        .on_hover_text(
                            "Probability that a caller will request you repeat your exchange",
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });
            });

        ui.add_space(8.0);

        // Audio Settings
        egui::CollapsingHeader::new(RichText::new("Audio Settings").strong())
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Tone Frequency (Hz):");
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut settings.audio.tone_frequency_hz,
                                400.0..=1000.0,
                            )
                            .fixed_decimals(0),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Noise Level:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise_level, 0.0..=0.5)
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Master Volume:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.master_volume, 0.0..=1.0)
                                .fixed_decimals(2),
                        )
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                if ui
                    .checkbox(
                        &mut settings.audio.mute_noise_during_tx,
                        "Mute background noise during TX",
                    )
                    .changed()
                {
                    *settings_changed = true;
                }

                ui.add_space(10.0);
                ui.label(RichText::new("Static/QRN Settings").strong());
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Crash Rate:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise.crash_rate, 0.0..=2.0)
                                .fixed_decimals(1)
                                .suffix("/sec"),
                        )
                        .on_hover_text("Static crashes per second")
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Crash Intensity:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise.crash_intensity, 0.0..=1.0)
                                .fixed_decimals(2),
                        )
                        .on_hover_text("Volume of static crashes")
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Pop Rate:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise.pop_rate, 0.0..=10.0)
                                .fixed_decimals(1)
                                .suffix("/sec"),
                        )
                        .on_hover_text("Clicks/pops per second")
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Pop Intensity:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise.pop_intensity, 0.0..=1.0)
                                .fixed_decimals(2),
                        )
                        .on_hover_text("Volume of pops/clicks")
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("QRN Intensity:");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise.qrn_intensity, 0.0..=1.0)
                                .fixed_decimals(2),
                        )
                        .on_hover_text("Atmospheric noise rumble")
                        .changed()
                    {
                        *settings_changed = true;
                    }
                });
            });
    });
}
