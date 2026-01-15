use crossbeam_channel::{bounded, Receiver, Sender};
use egui::Key;
use std::time::Instant;

use crate::audio::AudioEngine;
use crate::config::AppSettings;
use crate::contest::{self, Contest};
use crate::messages::{AudioCommand, AudioEvent, StationParams};
use crate::station::{CallsignPool, StationSpawner};
use crate::ui::{render_main_panel, render_settings_panel};

/// Which input field is active
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputField {
    Callsign,
    Exchange,
}

/// Current state of the contest session
#[derive(Clone, Debug)]
pub enum ContestState {
    /// Idle - waiting for user to start
    Idle,
    /// User is sending CQ
    CallingCq,
    /// CQ finished, waiting for stations to call
    WaitingForCallers,
    /// Station(s) are calling
    StationsCalling { callers: Vec<ActiveCaller> },
    /// User sent partial callsign query, waiting for matching station to repeat
    QueryingPartial {
        callers: Vec<ActiveCaller>,
        partial: String,
    },
    /// User entered callsign, we're sending their exchange
    SendingExchange { caller: ActiveCaller },
    /// Station is sending their exchange
    ReceivingExchange { caller: ActiveCaller },
    /// QSO complete, showing result
    QsoComplete { result: QsoResult },
}

#[derive(Clone, Debug)]
pub struct ActiveCaller {
    pub params: StationParams,
}

#[derive(Clone, Debug)]
pub struct QsoResult {
    pub callsign: String,
    pub expected_call: String,
    pub expected_exchange: String,
    pub callsign_correct: bool,
    pub exchange_correct: bool,
    pub points: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Score {
    pub qso_count: u32,
    pub total_points: u32,
    pub start_time: Option<Instant>,
}

impl Score {
    pub fn hourly_rate(&self) -> u32 {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_secs_f64() / 3600.0;
            if elapsed > 0.01 {
                return (self.qso_count as f64 / elapsed) as u32;
            }
        }
        0
    }

    pub fn add_qso(&mut self, points: u32) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self.qso_count += 1;
        self.total_points += points;
    }
}

pub struct ContestApp {
    pub settings: AppSettings,
    pub state: ContestState,
    pub score: Score,
    pub callsign_input: String,
    pub exchange_input: String,
    pub current_field: InputField,
    pub last_qso_result: Option<QsoResult>,

    // Audio system
    cmd_tx: Sender<AudioCommand>,
    event_rx: Receiver<AudioEvent>,
    audio_engine: Option<AudioEngine>,

    // Contest and station management
    contest: Box<dyn Contest>,
    spawner: StationSpawner,
    user_serial: u32,

    // UI state
    pub show_settings: bool,
    settings_changed: bool,

    // Timing for caller spawning
    last_cq_finished: Option<Instant>,
}

impl ContestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings::load_or_default();

        // Create channels for audio communication
        let (cmd_tx, cmd_rx) = bounded::<AudioCommand>(64);
        let (event_tx, event_rx) = bounded::<AudioEvent>(64);

        // Create audio engine
        let audio_engine = match AudioEngine::new(cmd_rx, event_tx, settings.audio.clone()) {
            Ok(engine) => Some(engine),
            Err(e) => {
                eprintln!("Failed to initialize audio: {}", e);
                None
            }
        };

        // Create contest
        let contest = contest::create_contest(settings.contest.contest_type);

        // Load callsigns
        let callsigns = CallsignPool::load(&settings.contest.callsign_file)
            .unwrap_or_else(|_| CallsignPool::default_pool());

        // Create station spawner
        let spawner = StationSpawner::new(callsigns, settings.simulation.clone());

        Self {
            settings,
            state: ContestState::Idle,
            score: Score::default(),
            callsign_input: String::new(),
            exchange_input: String::new(),
            current_field: InputField::Callsign,
            last_qso_result: None,
            cmd_tx,
            event_rx,
            audio_engine,
            contest,
            spawner,
            user_serial: 1,
            show_settings: false,
            settings_changed: false,
            last_cq_finished: None,
        }
    }

    fn send_cq(&mut self) {
        let message = format!("CQ TEST {} TEST", self.settings.user.callsign);
        let wpm = self.settings.user.wpm;

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessage { message, wpm });

        self.state = ContestState::CallingCq;
    }

    fn send_exchange(&mut self, their_call: &str) {
        let exchange = self.contest.user_exchange(
            &self.settings.user.callsign,
            self.user_serial,
            self.settings.user.zone,
            &self.settings.user.section,
            &self.settings.user.name,
        );

        let message = format!("{} {}", their_call, exchange);
        let wpm = self.settings.user.wpm;

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessage { message, wpm });
    }

    fn send_tu(&mut self) {
        let message = format!("TU {}", self.settings.user.callsign);
        let wpm = self.settings.user.wpm;

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessage { message, wpm });
    }

    fn send_partial_query(&mut self, partial: &str) {
        // Send partial callsign with "AGN" to request repeat
        let message = format!("{} AGN", partial);
        let wpm = self.settings.user.wpm;

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessage { message, wpm });
    }

    fn handle_partial_query(&mut self) {
        let partial = self.callsign_input.trim().to_uppercase();
        if partial.is_empty() {
            return;
        }

        // Only works when stations are calling
        let callers = match &self.state {
            ContestState::StationsCalling { callers } => callers.clone(),
            _ => return,
        };

        // Find callers that match the partial (case-insensitive substring match)
        let matching: Vec<ActiveCaller> = callers
            .into_iter()
            .filter(|c| c.params.callsign.contains(&partial))
            .collect();

        if matching.is_empty() {
            // No match - could send "AGN" anyway, but for now just ignore
            return;
        }

        // Send the partial query
        self.send_partial_query(&partial);

        // Transition to QueryingPartial state
        self.state = ContestState::QueryingPartial {
            callers: matching,
            partial,
        };
    }

    fn handle_callsign_submit(&mut self) {
        let entered_call = self.callsign_input.trim().to_uppercase();
        if entered_call.is_empty() {
            return;
        }

        if let ContestState::StationsCalling { ref callers } = self.state {
            // Find the caller (take first one for simplicity)
            if let Some(caller) = callers.first().cloned() {
                // Send our exchange to them
                self.send_exchange(&entered_call);

                self.state = ContestState::SendingExchange { caller };
                self.current_field = InputField::Exchange;
            }
        }
    }

    fn handle_exchange_submit(&mut self) {
        let entered_exchange = self.exchange_input.trim().to_uppercase();

        // Get the expected caller info
        let (expected_call, expected_exchange_obj) = match &self.state {
            ContestState::ReceivingExchange { caller } => (
                caller.params.callsign.clone(),
                caller.params.exchange.clone(),
            ),
            _ => return,
        };

        // Validate the entry
        let expected_exchange_str = self.contest.format_sent_exchange(&expected_exchange_obj);
        let validation = self.contest.validate(
            &expected_call,
            &expected_exchange_obj,
            &self.callsign_input.trim().to_uppercase(),
            &entered_exchange,
        );

        let result = QsoResult {
            callsign: self.callsign_input.trim().to_uppercase(),
            expected_call: expected_call.clone(),
            expected_exchange: expected_exchange_str,
            callsign_correct: validation.callsign_correct,
            exchange_correct: validation.exchange_correct,
            points: validation.points,
        };

        // Update score
        self.score.add_qso(validation.points);
        self.user_serial += 1;

        // Send TU
        self.send_tu();

        self.last_qso_result = Some(result.clone());
        self.state = ContestState::QsoComplete { result };

        // Clear inputs
        self.callsign_input.clear();
        self.exchange_input.clear();
        self.current_field = InputField::Callsign;
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::StationComplete(id) => {
                    self.spawner.station_completed();

                    // If we were waiting for their exchange, move to logging
                    if let ContestState::ReceivingExchange { ref caller } = self.state {
                        if caller.params.id == id {
                            // Exchange received, user can now log
                        }
                    }
                }
                AudioEvent::UserMessageComplete => {
                    match &self.state {
                        ContestState::CallingCq => {
                            // CQ finished, wait for callers
                            self.state = ContestState::WaitingForCallers;
                            self.last_cq_finished = Some(Instant::now());
                        }
                        ContestState::SendingExchange { caller } => {
                            // Our exchange sent, now station sends just their exchange
                            let caller = caller.clone();

                            // Have the station send only their exchange (not callsign again)
                            let exchange_str =
                                self.contest.format_sent_exchange(&caller.params.exchange);

                            let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                                id: caller.params.id,
                                callsign: exchange_str,
                                exchange: caller.params.exchange.clone(),
                                frequency_offset_hz: caller.params.frequency_offset_hz,
                                wpm: caller.params.wpm,
                                amplitude: caller.params.amplitude,
                            }));

                            self.state = ContestState::ReceivingExchange { caller };
                        }
                        ContestState::QsoComplete { .. } => {
                            // TU finished - maybe a tail-ender jumps in
                            self.try_spawn_tail_ender();
                        }
                        ContestState::QueryingPartial {
                            callers,
                            partial: _,
                        } => {
                            // Partial query sent, matching station(s) repeat their callsign
                            let callers = callers.clone();

                            for caller in &callers {
                                // Station repeats just their callsign
                                let _ =
                                    self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                                        id: caller.params.id,
                                        callsign: caller.params.callsign.clone(),
                                        exchange: caller.params.exchange.clone(),
                                        frequency_offset_hz: caller.params.frequency_offset_hz,
                                        wpm: caller.params.wpm,
                                        amplitude: caller.params.amplitude,
                                    }));
                            }

                            // Go back to StationsCalling with only the matching callers
                            self.state = ContestState::StationsCalling { callers };
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Try to spawn a tail-ender after TU - a station that calls immediately
    /// without waiting for another CQ
    fn try_spawn_tail_ender(&mut self) {
        use rand::Rng;

        let mut rng = rand::thread_rng();

        // Use the same probability as normal station spawning
        if rng.gen::<f32>() > self.settings.simulation.station_probability {
            // No tail-ender, go to idle
            self.state = ContestState::Idle;
            return;
        }

        // Try to spawn a station
        let new_stations = self.spawner.tick(self.contest.as_ref());

        if new_stations.is_empty() {
            // Couldn't spawn, go to idle
            self.state = ContestState::Idle;
            return;
        }

        // Spawn the tail-ender(s)
        let mut callers = Vec::new();
        for params in new_stations {
            let _ = self.cmd_tx.send(AudioCommand::StartStation(params.clone()));
            callers.push(ActiveCaller { params });
        }

        self.state = ContestState::StationsCalling { callers };
    }

    fn maybe_spawn_callers(&mut self) {
        if !matches!(self.state, ContestState::WaitingForCallers) {
            return;
        }

        // Wait a bit after CQ before callers respond
        if let Some(finished) = self.last_cq_finished {
            if finished.elapsed().as_millis() < 300 {
                return;
            }
        }

        // Try to spawn stations
        let new_stations = self.spawner.tick(self.contest.as_ref());

        if !new_stations.is_empty() {
            let mut callers = Vec::new();

            for params in new_stations {
                // Station sends their callsign
                let _ = self.cmd_tx.send(AudioCommand::StartStation(params.clone()));
                callers.push(ActiveCaller { params });
            }

            self.state = ContestState::StationsCalling { callers };
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // F1 - Send CQ
            if i.key_pressed(Key::F1) {
                match self.state {
                    ContestState::Idle | ContestState::QsoComplete { .. } => {
                        self.spawner.reset();
                        self.send_cq();
                    }
                    _ => {}
                }
            }

            // F2 - Send Exchange
            if i.key_pressed(Key::F2) {
                let callsign = if let ContestState::StationsCalling { ref callers } = self.state {
                    callers.first().map(|c| c.params.callsign.clone())
                } else {
                    None
                };
                if let Some(call) = callsign {
                    self.send_exchange(&call);
                }
            }

            // F3 - Send TU
            if i.key_pressed(Key::F3) {
                self.send_tu();
            }

            // F5 - Query partial callsign (send what user typed + "AGN")
            if i.key_pressed(Key::F5) {
                self.handle_partial_query();
            }

            // Enter - Submit current field
            if i.key_pressed(Key::Enter) {
                match self.current_field {
                    InputField::Callsign => {
                        self.handle_callsign_submit();
                    }
                    InputField::Exchange => {
                        self.handle_exchange_submit();
                    }
                }
            }

            // Escape - Clear
            if i.key_pressed(Key::Escape) {
                self.callsign_input.clear();
                self.exchange_input.clear();
                self.current_field = InputField::Callsign;
                let _ = self.cmd_tx.send(AudioCommand::StopAll);
            }

            // Tab - Switch fields
            if i.key_pressed(Key::Tab) {
                self.current_field = match self.current_field {
                    InputField::Callsign => InputField::Exchange,
                    InputField::Exchange => InputField::Callsign,
                };
            }
        });
    }

    fn apply_settings_changes(&mut self) {
        if self.settings_changed {
            // Update contest type
            self.contest = contest::create_contest(self.settings.contest.contest_type);

            // Update callsigns
            let callsigns = CallsignPool::load(&self.settings.contest.callsign_file)
                .unwrap_or_else(|_| CallsignPool::default_pool());
            self.spawner.update_callsigns(callsigns);

            // Update spawner settings
            self.spawner
                .update_settings(self.settings.simulation.clone());

            // Update audio settings
            let _ = self
                .cmd_tx
                .send(AudioCommand::UpdateSettings(self.settings.audio.clone()));

            // Save settings to file
            if let Err(e) = self.settings.save() {
                eprintln!("Failed to save settings: {}", e);
            }

            self.settings_changed = false;
        }
    }
}

impl eframe::App for ContestApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process audio engine commands
        if let Some(ref engine) = self.audio_engine {
            engine.process_commands();
        }

        // Process audio events
        self.process_audio_events();

        // Maybe spawn callers
        self.maybe_spawn_callers();

        // Handle keyboard input
        self.handle_keyboard(ctx);

        // Apply any settings changes
        self.apply_settings_changes();

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Settings").clicked() {
                        self.show_settings = !self.show_settings;
                        ui.close();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        // Settings panel (side panel)
        if self.show_settings {
            egui::SidePanel::right("settings_panel")
                .min_width(300.0)
                .show(ctx, |ui| {
                    render_settings_panel(
                        ui,
                        &mut self.settings,
                        &mut self.settings_changed,
                        &mut self.show_settings,
                    );
                });
        }

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("CW Contest Trainer");
            ui.separator();
            render_main_panel(ui, self);
        });

        // Request continuous repaints for audio processing
        ctx.request_repaint();
    }
}
