use crate::config::AppSettings;
use crate::contest::{Contest, ContestDescriptor, SettingFieldGroup, SettingFieldKind};
use egui::{RichText, Vec2};
use egui_file_dialog::FileDialog;

/// Tracks which file field triggered the file dialog
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FileDialogTarget {
    ContestSetting { contest_id: String, key: String },
    ExportDirectory,
}

pub fn render_settings_panel(
    ui: &mut egui::Ui,
    settings: &mut AppSettings,
    settings_changed: &mut bool,
    contest_registry: &[ContestDescriptor],
    active_contest: &dyn Contest,
    file_dialog: &mut FileDialog,
    file_dialog_target: &mut Option<FileDialogTarget>,
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

                if ui
                    .checkbox(&mut settings.user.show_main_hints, "Show Main Field Hints")
                    .changed()
                {
                    *settings_changed = true;
                }

                ui.add_space(4.0);
                ui.label("Stats Export Directory:");
                ui.horizontal(|ui| {
                    let display = if settings.user.export_directory.is_empty() {
                        "(current directory)".to_string()
                    } else {
                        settings.user.export_directory.clone()
                    };
                    ui.add(egui::TextEdit::singleline(&mut display.as_str()).desired_width(250.0));
                    if ui.button("Browse...").clicked() {
                        *file_dialog_target = Some(FileDialogTarget::ExportDirectory);
                        file_dialog.pick_directory();
                    }
                    if !settings.user.export_directory.is_empty() && ui.button("Clear").clicked() {
                        settings.user.export_directory.clear();
                        *settings_changed = true;
                    }
                });
            });

        ui.add_space(8.0);

        // Contest Settings
        egui::CollapsingHeader::new(RichText::new("Contest Settings").strong())
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Contest Type:");
                    egui::ComboBox::from_id_salt("contest_type")
                        .selected_text(active_contest.display_name())
                        .show_ui(ui, |ui| {
                            for contest in contest_registry {
                                if ui
                                    .selectable_value(
                                        &mut settings.contest.active_contest_id,
                                        contest.id.to_string(),
                                        contest.display_name,
                                    )
                                    .changed()
                                {
                                    *settings_changed = true;
                                }
                            }
                        });
                });
            });

        ui.add_space(8.0);

        // Contest-specific settings
        egui::CollapsingHeader::new(RichText::new("Active Contest").strong())
            .default_open(true)
            .show(ui, |ui| {
                let contest_id = settings.contest.active_contest_id.clone();
                let contest_settings = settings.contest.settings_for_mut(active_contest);
                render_contest_settings(
                    ui,
                    active_contest,
                    contest_settings,
                    settings_changed,
                    file_dialog,
                    file_dialog_target,
                    &contest_id,
                );
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
                    ui.label("Filter Width (Hz):");
                    if ui
                        .add(
                            egui::Slider::new(
                                &mut settings.simulation.frequency_spread_hz,
                                100.0..=500.0,
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

                if ui
                    .checkbox(
                        &mut settings.simulation.same_country_filter_enabled,
                        "Filter Callers by Country",
                    )
                    .on_hover_text("When enabled, controls how often callers are from your country")
                    .changed()
                {
                    *settings_changed = true;
                }

                if settings.simulation.same_country_filter_enabled {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0); // indent
                        ui.label("Same Country Probability:");
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut settings.simulation.same_country_probability,
                                    0.0..=1.0,
                                )
                                .fixed_decimals(2),
                            )
                            .on_hover_text(
                                "Probability that a caller will be from the same country as you",
                            )
                            .changed()
                        {
                            *settings_changed = true;
                        }
                    });
                }
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
                    ui.label("Noise Bandwidth (Hz):");
                    if ui
                        .add(
                            egui::Slider::new(&mut settings.audio.noise_bandwidth, 100.0..=1000.0)
                                .fixed_decimals(0),
                        )
                        .on_hover_text("Simulates receiver CW filter bandwidth")
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
                        &mut settings.audio.mute_rx_during_tx,
                        "Mute RX during TX (callers + noise)",
                    )
                    .changed()
                {
                    *settings_changed = true;
                }
                if ui
                    .checkbox(
                        &mut settings.audio.mute_sidetone_during_tx,
                        "Mute sidetone during TX",
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

                ui.add_space(10.0);
                ui.label(RichText::new("QSB (Fading) Settings").strong());
                ui.separator();

                if ui
                    .checkbox(&mut settings.audio.qsb.enabled, "Enable QSB")
                    .on_hover_text("Simulate signal fading on caller signals")
                    .changed()
                {
                    *settings_changed = true;
                }

                if settings.audio.qsb.enabled {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0); // indent
                        ui.label("Fade Depth:");
                        if ui
                            .add(
                                egui::Slider::new(&mut settings.audio.qsb.depth, 0.0..=1.0)
                                    .fixed_decimals(2),
                            )
                            .on_hover_text(
                                "How much the signal fades (0 = none, 1 = full fade to silence)",
                            )
                            .changed()
                        {
                            *settings_changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.add_space(20.0); // indent
                        ui.label("Fade Rate:");
                        if ui
                            .add(
                                egui::Slider::new(&mut settings.audio.qsb.rate, 1.0..=20.0)
                                    .fixed_decimals(1)
                                    .suffix(" cpm"),
                            )
                            .on_hover_text("Fading cycles per minute (higher = faster fading)")
                            .changed()
                        {
                            *settings_changed = true;
                        }
                    });
                }
            });
    });
}

fn render_contest_settings(
    ui: &mut egui::Ui,
    contest: &dyn Contest,
    contest_settings: &mut toml::Value,
    settings_changed: &mut bool,
    file_dialog: &mut FileDialog,
    file_dialog_target: &mut Option<FileDialogTarget>,
    contest_id: &str,
) {
    let mut contest_fields = Vec::new();
    let mut user_fields = Vec::new();

    for field in contest.settings_fields() {
        match field.group {
            SettingFieldGroup::Contest => contest_fields.push(field),
            SettingFieldGroup::UserExchange => user_fields.push(field),
        }
    }

    if !contest_fields.is_empty() {
        ui.label(RichText::new("Contest").strong());
        render_setting_group(
            ui,
            &contest_fields,
            contest_settings,
            settings_changed,
            file_dialog,
            file_dialog_target,
            contest_id,
        );
        ui.add_space(6.0);
    }

    if !user_fields.is_empty() {
        ui.label(RichText::new("Your Exchange").strong());
        render_setting_group(
            ui,
            &user_fields,
            contest_settings,
            settings_changed,
            file_dialog,
            file_dialog_target,
            contest_id,
        );
    }
}

fn render_setting_group(
    ui: &mut egui::Ui,
    fields: &[crate::contest::SettingField],
    contest_settings: &mut toml::Value,
    settings_changed: &mut bool,
    file_dialog: &mut FileDialog,
    file_dialog_target: &mut Option<FileDialogTarget>,
    contest_id: &str,
) {
    let table = contest_settings_table(contest_settings);

    for field in fields {
        ui.horizontal(|ui| {
            ui.label(field.label);
            match field.kind {
                SettingFieldKind::FilePath => {
                    let value = table
                        .get(field.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mut display_value = value.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut display_value)
                            .interactive(false)
                            .desired_width(250.0),
                    );
                    if ui.button("Browse...").clicked() {
                        *file_dialog_target = Some(FileDialogTarget::ContestSetting {
                            contest_id: contest_id.to_string(),
                            key: field.key.to_string(),
                        });
                        file_dialog.pick_file();
                    }
                }
                SettingFieldKind::Text => {
                    let width_px = setting_field_width(ui, field.width_chars);
                    let mut value = table
                        .get(field.key)
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let response = ui.add_sized(
                        Vec2::new(width_px, 24.0),
                        egui::TextEdit::singleline(&mut value).hint_text(field.placeholder),
                    );
                    if response.changed() {
                        value = value.to_uppercase();
                        table.insert(field.key.to_string(), toml::Value::String(value));
                        *settings_changed = true;
                    }
                }
                SettingFieldKind::Integer { min, max } => {
                    let width_px = setting_field_width(ui, field.width_chars);
                    let mut value = table
                        .get(field.key)
                        .and_then(|v| v.as_integer())
                        .or_else(|| {
                            table
                                .get(field.key)
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.trim().parse::<i64>().ok())
                        })
                        .unwrap_or(min);
                    let response = ui.add_sized(
                        Vec2::new(width_px, 24.0),
                        egui::DragValue::new(&mut value).range(min..=max),
                    );
                    if response.changed() {
                        let clamped = value.clamp(min, max);
                        table.insert(field.key.to_string(), toml::Value::Integer(clamped));
                        *settings_changed = true;
                    }
                }
            }
        });
    }
}

fn contest_settings_table(settings: &mut toml::Value) -> &mut toml::value::Table {
    if !settings.is_table() {
        *settings = toml::Value::Table(toml::value::Table::new());
    }
    settings
        .as_table_mut()
        .expect("contest settings must be a table")
}

fn setting_field_width(ui: &egui::Ui, width_chars: u8) -> f32 {
    let font_size = ui.text_style_height(&egui::TextStyle::Body);
    let char_width = (font_size * 0.6).max(6.0);
    char_width * width_chars as f32 + 8.0
}
