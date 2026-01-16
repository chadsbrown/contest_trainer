use crossbeam_channel::{bounded, Receiver, Sender};
use egui::Key;
use std::time::Instant;

use crate::audio::AudioEngine;
use crate::config::AppSettings;
use crate::contest::ContestType;
use crate::contest::{self, Contest};
use crate::messages::{AudioCommand, AudioEvent, StationParams};
use crate::station::{CallsignPool, CwtCallsignPool, StationSpawner};
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
    /// Brief pause before station repeats callsign after partial query
    WaitingForPartialResponse {
        callers: Vec<ActiveCaller>,
        wait_until: Instant,
    },
    /// User entered callsign, we're sending their exchange
    SendingExchange { caller: ActiveCaller },
    /// Brief pause before station sends their exchange
    WaitingToSendExchange {
        caller: ActiveCaller,
        wait_until: Instant,
    },
    /// Station is sending their exchange
    ReceivingExchange { caller: ActiveCaller },
    /// User requested AGN, sending AGN message
    SendingAgn { caller: ActiveCaller },
    /// Waiting for station to resend exchange after AGN
    WaitingForAgn {
        caller: ActiveCaller,
        wait_until: Instant,
    },
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

    // Noise toggle state
    pub noise_enabled: bool,
    saved_noise_level: f32,
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

        // Load callsigns and create spawner based on contest type
        let spawner = if settings.contest.contest_type == ContestType::Cwt {
            let cwt_callsigns = CwtCallsignPool::load(&settings.contest.cwt_callsign_file)
                .unwrap_or_else(|_| CwtCallsignPool::default_pool());
            StationSpawner::new_cwt(cwt_callsigns, settings.simulation.clone())
        } else {
            let callsigns = CallsignPool::load(&settings.contest.callsign_file)
                .unwrap_or_else(|_| CallsignPool::default_pool());
            StationSpawner::new(callsigns, settings.simulation.clone())
        };

        let noise_enabled = settings.audio.noise_level > 0.0;
        let saved_noise_level = settings.audio.noise_level;

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
            noise_enabled,
            saved_noise_level,
        }
    }

    pub fn reset_score(&mut self) {
        self.score = Score::default();
        self.last_qso_result = None;
        self.user_serial = 1;
    }

    pub fn toggle_noise(&mut self) {
        if self.noise_enabled {
            // Save current level and disable
            self.saved_noise_level = self.settings.audio.noise_level;
            self.settings.audio.noise_level = 0.0;
            self.noise_enabled = false;
        } else {
            // Restore saved level (use default if saved was 0)
            self.settings.audio.noise_level = if self.saved_noise_level > 0.0 {
                self.saved_noise_level
            } else {
                0.15
            };
            self.noise_enabled = true;
        }
        // Send updated settings to audio engine
        let _ = self
            .cmd_tx
            .send(AudioCommand::UpdateSettings(self.settings.audio.clone()));
    }

    fn send_cq(&mut self) {
        let message = format!("CQ TEST {}", self.settings.user.callsign);
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
        // Send just the partial callsign (his call)
        let wpm = self.settings.user.wpm;

        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessage {
            message: partial.to_string(),
            wpm,
        });
    }

    /// Calculate similarity between two strings (0.0 to 1.0)
    /// Uses longest common subsequence ratio
    fn callsign_similarity(a: &str, b: &str) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

        // Count matching characters in sequence
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        let mut matches = 0;
        let mut b_idx = 0;
        for a_char in &a_chars {
            for j in b_idx..b_chars.len() {
                if *a_char == b_chars[j] {
                    matches += 1;
                    b_idx = j + 1;
                    break;
                }
            }
        }

        // Also check if one contains the other as substring
        if a.contains(b) || b.contains(a) {
            let shorter = a.len().min(b.len()) as f32;
            let longer = a.len().max(b.len()) as f32;
            return shorter / longer;
        }

        (2.0 * matches as f32) / (a.len() + b.len()) as f32
    }

    /// Find the most similar caller to the entered text
    /// Returns None if no caller is similar enough (threshold: 0.4)
    fn find_similar_caller<'a>(
        entered: &str,
        callers: &'a [ActiveCaller],
    ) -> Option<&'a ActiveCaller> {
        const SIMILARITY_THRESHOLD: f32 = 0.4;

        callers
            .iter()
            .map(|c| (c, Self::callsign_similarity(entered, &c.params.callsign)))
            .filter(|(_, sim)| *sim >= SIMILARITY_THRESHOLD)
            .max_by(|(_, sim_a), (_, sim_b)| sim_a.partial_cmp(sim_b).unwrap())
            .map(|(caller, _)| caller)
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

        // Find the most similar caller
        let matching_caller = Self::find_similar_caller(&partial, &callers);

        if matching_caller.is_none() {
            // No similar match - do nothing
            return;
        }

        // Send the partial query
        self.send_partial_query(&partial);

        // Transition to QueryingPartial state with only the matching caller
        let matching = matching_caller.into_iter().cloned().collect();
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
            // Find the most similar caller
            let caller = Self::find_similar_caller(&entered_call, callers).cloned();

            if let Some(caller) = caller {
                // Send our exchange to them
                self.send_exchange(&entered_call);

                self.state = ContestState::SendingExchange { caller };
                self.current_field = InputField::Exchange;
            }
            // If no similar caller found, do nothing - user should press F1 to CQ again
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

    fn handle_agn_request(&mut self) {
        // Only works when receiving exchange
        let caller = match &self.state {
            ContestState::ReceivingExchange { caller } => caller.clone(),
            _ => return,
        };

        // Stop any current station audio
        let _ = self.cmd_tx.send(AudioCommand::StopAll);

        // Send the AGN message
        let agn_message = self.settings.user.agn_message.clone();
        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessage {
            message: agn_message,
            wpm: self.settings.user.wpm,
        });

        self.state = ContestState::SendingAgn { caller };
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
                            // Our exchange sent, wait briefly before station responds
                            let caller = caller.clone();
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingToSendExchange { caller, wait_until };
                        }
                        ContestState::QsoComplete { .. } => {
                            // TU finished - maybe a tail-ender jumps in
                            self.try_spawn_tail_ender();
                        }
                        ContestState::QueryingPartial {
                            callers,
                            partial: _,
                        } => {
                            // Partial query sent, wait briefly before station repeats
                            let callers = callers.clone();
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingForPartialResponse {
                                callers,
                                wait_until,
                            };
                        }
                        ContestState::SendingAgn { caller } => {
                            // AGN request sent, wait briefly before station resends exchange
                            let caller = caller.clone();
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingForAgn { caller, wait_until };
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

    fn check_waiting_to_send_exchange(&mut self) {
        if let ContestState::WaitingToSendExchange { caller, wait_until } = &self.state {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();

                // Have the station send only their exchange (not callsign again)
                let exchange_str = self.contest.format_sent_exchange(&caller.params.exchange);

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
        }
    }

    fn check_waiting_for_agn(&mut self) {
        if let ContestState::WaitingForAgn { caller, wait_until } = &self.state {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();

                // Have the station resend their exchange
                let exchange_str = self.contest.format_sent_exchange(&caller.params.exchange);

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
        }
    }

    fn check_waiting_for_partial_response(&mut self) {
        if let ContestState::WaitingForPartialResponse {
            callers,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let callers = callers.clone();

                // Station(s) repeat their callsign
                for caller in &callers {
                    let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
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
        }
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
            // F1 - Send CQ (always available - resets state and starts fresh)
            if i.key_pressed(Key::F1) {
                // Stop any playing audio
                let _ = self.cmd_tx.send(AudioCommand::StopAll);
                // Reset spawner and inputs
                self.spawner.reset();
                self.callsign_input.clear();
                self.exchange_input.clear();
                self.current_field = InputField::Callsign;
                // Send CQ
                self.send_cq();
            }

            // F2 - Send Exchange (uses callsign from input field if available)
            if i.key_pressed(Key::F2) {
                if let ContestState::StationsCalling { ref callers } = self.state {
                    let entered_call = self.callsign_input.trim().to_uppercase();
                    // Use entered callsign if it matches a caller, otherwise use first caller
                    let callsign = if !entered_call.is_empty() {
                        callers
                            .iter()
                            .find(|c| {
                                c.params.callsign == entered_call
                                    || c.params.callsign.contains(&entered_call)
                            })
                            .map(|c| c.params.callsign.clone())
                    } else {
                        callers.first().map(|c| c.params.callsign.clone())
                    };
                    if let Some(call) = callsign {
                        self.send_exchange(&call);
                    }
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

            // F8 - Request AGN (ask station to repeat exchange)
            if i.key_pressed(Key::F8) {
                self.handle_agn_request();
            }

            // Enter - Submit current field (or send CQ if callsign field is empty)
            if i.key_pressed(Key::Enter) {
                match self.current_field {
                    InputField::Callsign => {
                        if self.callsign_input.trim().is_empty() {
                            // Empty callsign field - act like F1 (send CQ)
                            let _ = self.cmd_tx.send(AudioCommand::StopAll);
                            self.spawner.reset();
                            self.callsign_input.clear();
                            self.exchange_input.clear();
                            self.current_field = InputField::Callsign;
                            self.send_cq();
                        } else {
                            self.handle_callsign_submit();
                        }
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

            // Update callsigns based on contest type
            if self.settings.contest.contest_type == ContestType::Cwt {
                let cwt_callsigns = CwtCallsignPool::load(&self.settings.contest.cwt_callsign_file)
                    .unwrap_or_else(|_| CwtCallsignPool::default_pool());
                self.spawner.update_cwt_callsigns(cwt_callsigns);
            } else {
                let callsigns = CallsignPool::load(&self.settings.contest.callsign_file)
                    .unwrap_or_else(|_| CallsignPool::default_pool());
                self.spawner.update_callsigns(callsigns);
            }

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
        // Apply font size
        ctx.style_mut(|style| {
            style.text_styles.iter_mut().for_each(|(_, font_id)| {
                font_id.size = self.settings.user.font_size;
            });
        });

        // Process audio engine commands
        if let Some(ref engine) = self.audio_engine {
            engine.process_commands();
        }

        // Process audio events
        self.process_audio_events();

        // Maybe spawn callers
        self.maybe_spawn_callers();

        // Check if waiting to send exchange
        self.check_waiting_to_send_exchange();

        // Check if waiting for AGN response
        self.check_waiting_for_agn();

        // Check if waiting for partial response
        self.check_waiting_for_partial_response();

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

        // Settings window (separate OS window)
        if self.show_settings {
            let settings = &mut self.settings;
            let settings_changed = &mut self.settings_changed;
            let show_settings = &mut self.show_settings;

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("Settings")
                    .with_inner_size([475.0, 600.0]),
                |ctx, _class| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_settings_panel(ui, settings, settings_changed);
                    });

                    if ctx.input(|i| i.viewport().close_requested()) {
                        *show_settings = false;
                    }
                },
            );
        }

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            render_main_panel(ui, self);
        });

        // Request continuous repaints for audio processing
        ctx.request_repaint();
    }
}
