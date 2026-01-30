use crossbeam_channel::{bounded, Receiver, Sender};
use egui::Key;
use egui_file_dialog::FileDialog;
use std::time::Instant;

use crate::audio::AudioEngine;
use crate::config::AppSettings;
use crate::contest::{self, Contest, ContestDescriptor, FieldKind};
use crate::cty::CtyDat;
use crate::messages::{
    AudioCommand, AudioEvent, MessageSegment, MessageSegmentType, StationParams,
};
use crate::state::{ContestState, QsoContext, StationTxType, StatusColor, UserTxType};
use crate::station::{CallerManager, CallerResponse};
use crate::stats::{QsoRecord, SessionStats};
use crate::ui::{render_main_panel, render_settings_panel, render_stats_window, FileDialogTarget};

/// Which input field is active
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputField {
    Callsign,
    Exchange(usize),
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
    pub context: QsoContext,
    pub score: Score,
    pub callsign_input: String,
    pub exchange_inputs: Vec<String>,
    pub current_field: InputField,
    pub last_qso_result: Option<QsoResult>,

    // Audio system
    cmd_tx: Sender<AudioCommand>,
    event_rx: Receiver<AudioEvent>,
    audio_engine: Option<AudioEngine>,

    // Contest and station management
    pub contest: Box<dyn Contest>,
    contest_registry: Vec<ContestDescriptor>,
    caller_manager: CallerManager,
    user_serial: u32,
    cty: CtyDat,

    // UI state
    pub show_settings: bool,
    settings_changed: bool,
    pub settings_notice: Option<String>,
    settings_error: Option<String>,
    pub last_exchange_field_index: usize,

    // Timing for caller spawning
    last_cq_finished: Option<Instant>,

    // Noise toggle state
    pub noise_enabled: bool,
    saved_noise_level: f32,

    // Session statistics
    pub session_stats: SessionStats,
    pub show_stats: bool,

    // AGN usage tracking for current QSO
    used_agn_callsign: bool,
    used_agn_exchange: bool,
    used_f5_callsign: bool,

    // File dialog for settings
    file_dialog: FileDialog,
    file_dialog_target: Option<FileDialogTarget>,
}

impl ContestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let load_result = AppSettings::load_with_notice();
        let mut settings = load_result.settings;
        let settings_notice = load_result.notice;
        let mut settings_changed = false;

        let contest_registry = contest::registry();
        let default_descriptor = contest_registry
            .first()
            .expect("No contests registered. Add at least one contest file.");
        let active_descriptor = contest_registry
            .iter()
            .find(|entry| entry.id == settings.contest.active_contest_id)
            .unwrap_or_else(|| {
                settings.contest.active_contest_id = default_descriptor.id.to_string();
                settings_changed = true;
                default_descriptor
            });

        // Create channels for audio communication
        let (cmd_tx, cmd_rx) = bounded::<AudioCommand>(64);
        let (event_tx, event_rx) = bounded::<AudioEvent>(64);

        // Create audio engine
        let audio_engine = match AudioEngine::new(cmd_rx, event_tx, settings.audio.clone()) {
            Ok(engine) => Some(engine),
            Err(e) => {
                #[cfg(debug_assertions)]
                eprintln!("Failed to initialize audio: {}", e);
                let _ = e;
                None
            }
        };

        // Create contest
        let contest = (active_descriptor.factory)();
        let needs_settings = !settings.contest.contests.contains_key(active_descriptor.id);
        let contest_settings = settings.contest.settings_for_mut(contest.as_ref());
        if needs_settings {
            settings_changed = true;
        }
        let settings_error = contest.validate_settings(contest_settings).err();

        // Load CTY database for country lookups
        let cty_data = include_str!("../data/cty.dat");
        let cty = CtyDat::parse(cty_data);

        // Load callsigns and create caller manager
        let callsign_source = contest
            .callsign_source(contest_settings)
            .unwrap_or_else(|_| {
                contest
                    .callsign_source(&contest.default_settings())
                    .expect("Failed to build callsign source")
            });
        let caller_manager = CallerManager::new(callsign_source, settings.simulation.clone());

        let noise_enabled = settings.audio.noise_level > 0.0;
        let saved_noise_level = settings.audio.noise_level;

        Self {
            settings,
            state: ContestState::Idle,
            context: QsoContext::new(),
            score: Score::default(),
            callsign_input: String::new(),
            exchange_inputs: contest
                .exchange_fields()
                .iter()
                .map(|field| field.default_value.unwrap_or("").to_string())
                .collect(),
            current_field: InputField::Callsign,
            last_qso_result: None,
            cmd_tx,
            event_rx,
            audio_engine,
            contest,
            contest_registry,
            caller_manager,
            user_serial: 1,
            cty,
            show_settings: false,
            settings_changed,
            settings_notice,
            settings_error,
            last_exchange_field_index: 0,
            last_cq_finished: None,
            noise_enabled,
            saved_noise_level,
            session_stats: SessionStats::new(),
            show_stats: false,
            used_agn_callsign: false,
            used_agn_exchange: false,
            used_f5_callsign: false,
            file_dialog: FileDialog::new(),
            file_dialog_target: None,
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

    /// Get the status text and color for UI display
    pub fn get_status(&self) -> (&'static str, StatusColor) {
        self.state.status_text(&self.context)
    }

    fn send_cq(&mut self) {
        let cq_prefix = self
            .contest
            .cq_message(
                self.settings
                    .contest
                    .settings_for_mut(self.contest.as_ref()),
            )
            .trim()
            .to_string();
        let callsign = self.settings.user.callsign.trim();
        let message = format!("{} {}", cq_prefix, callsign);
        let wpm = self.settings.user.wpm;

        let segments = vec![MessageSegment {
            content: message,
            segment_type: MessageSegmentType::Cq,
        }];

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessageSegmented { segments, wpm });

        self.state = ContestState::CallingCq;

        // Reset AGN tracking for new QSO
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;
        self.used_f5_callsign = false;

        // Reset context for new QSO
        self.context.reset();
    }

    fn send_exchange(&mut self, their_call: &str) {
        self.context.awaiting_user_exchange = false;
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let exchange_fields = self.contest.user_exchange_fields(
            &self.settings.user.callsign,
            self.user_serial,
            contest_settings,
        );
        let exchange = self.contest.format_user_exchange(&exchange_fields);

        let wpm = self.settings.user.wpm;

        // Use segmented message for element-level tracking
        // Word gap is automatically added between segments by SegmentedUserStation
        let segments = vec![
            MessageSegment {
                content: their_call.to_string(),
                segment_type: MessageSegmentType::TheirCallsign,
            },
            MessageSegment {
                content: exchange,
                segment_type: MessageSegmentType::OurExchange,
            },
        ];

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessageSegmented { segments, wpm });
    }

    fn send_exchange_only(&mut self) {
        self.context.awaiting_user_exchange = false;
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let exchange_fields = self.contest.user_exchange_fields(
            &self.settings.user.callsign,
            self.user_serial,
            contest_settings,
        );
        let exchange = self.contest.format_user_exchange(&exchange_fields);

        let wpm = self.settings.user.wpm;

        // Use segmented message for element-level tracking
        let segments = vec![MessageSegment {
            content: exchange,
            segment_type: MessageSegmentType::OurExchange,
        }];

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessageSegmented { segments, wpm });
    }

    fn send_tu(&mut self) {
        let message = format!("TU {}", self.settings.user.callsign);
        let wpm = self.settings.user.wpm;

        let segments = vec![MessageSegment {
            content: message,
            segment_type: MessageSegmentType::Tu,
        }];

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessageSegmented { segments, wpm });
    }

    fn send_his_call(&mut self) {
        let their_call = self.callsign_input.trim().to_uppercase();
        if their_call.is_empty() {
            return;
        }

        let wpm = self.settings.user.wpm;

        // Use segmented message for element-level tracking
        let segments = vec![MessageSegment {
            content: their_call,
            segment_type: MessageSegmentType::TheirCallsign,
        }];

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessageSegmented { segments, wpm });
    }

    fn clear_exchange_inputs(&mut self) {
        self.exchange_inputs = self.exchange_default_values();
    }

    fn reset_exchange_inputs(&mut self) {
        self.exchange_inputs = self.exchange_default_values();
        if self.exchange_inputs.is_empty() {
            self.last_exchange_field_index = 0;
        } else if self.last_exchange_field_index >= self.exchange_inputs.len() {
            self.last_exchange_field_index = self.exchange_inputs.len() - 1;
        }
    }

    fn set_exchange_field(&mut self, index: usize) {
        if self.exchange_inputs.is_empty() {
            self.current_field = InputField::Callsign;
            return;
        }
        let index = index.min(self.exchange_inputs.len() - 1);
        self.last_exchange_field_index = index;
        self.current_field = InputField::Exchange(index);
    }

    fn advance_field_forward(&mut self) {
        match self.current_field {
            InputField::Callsign => {
                if !self.exchange_inputs.is_empty() {
                    self.set_exchange_field(0);
                }
            }
            InputField::Exchange(index) => {
                let next = index + 1;
                if next < self.exchange_inputs.len() {
                    self.set_exchange_field(next);
                } else {
                    self.current_field = InputField::Callsign;
                }
            }
        }
    }

    fn advance_field_backward(&mut self) {
        match self.current_field {
            InputField::Callsign => {
                if !self.exchange_inputs.is_empty() {
                    self.set_exchange_field(self.exchange_inputs.len() - 1);
                }
            }
            InputField::Exchange(index) => {
                if index > 0 {
                    self.set_exchange_field(index - 1);
                } else {
                    self.current_field = InputField::Callsign;
                }
            }
        }
    }

    fn normalized_exchange_inputs(&self) -> Vec<String> {
        let field_defs = self.contest.exchange_fields();
        self.exchange_inputs
            .iter()
            .enumerate()
            .map(|(idx, value)| {
                let kind = field_defs
                    .get(idx)
                    .map(|field| field.kind)
                    .unwrap_or(FieldKind::Text);
                contest::normalize_exchange_input(value, kind)
            })
            .collect()
    }

    pub fn exchange_default_values(&self) -> Vec<String> {
        self.contest
            .exchange_fields()
            .iter()
            .map(|field| field.default_value.unwrap_or("").to_string())
            .collect()
    }

    fn exchange_focus_index(&self) -> usize {
        self.contest
            .exchange_fields()
            .iter()
            .position(|field| field.focus_on_enter)
            .unwrap_or(0)
    }

    /// Calculate similarity between two strings (0.0 to 1.0)
    fn callsign_similarity(a: &str, b: &str) -> f32 {
        if a.is_empty() || b.is_empty() {
            return 0.0;
        }

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

        if a.contains(b) || b.contains(a) {
            let shorter = a.len().min(b.len()) as f32;
            let longer = a.len().max(b.len()) as f32;
            return shorter / longer;
        }

        (2.0 * matches as f32) / (a.len() + b.len()) as f32
    }

    /// Find the most similar caller to the entered text
    fn find_similar_caller<'a>(
        entered: &str,
        callers: &'a [ActiveCaller],
    ) -> Option<&'a ActiveCaller> {
        const SIMILARITY_THRESHOLD: f32 = 0.4;

        callers
            .iter()
            .map(|c| (c, Self::callsign_similarity(entered, &c.params.callsign)))
            .filter(|(_, sim)| *sim >= SIMILARITY_THRESHOLD)
            .max_by(|(_, sim_a), (_, sim_b)| sim_a.total_cmp(sim_b))
            .map(|(caller, _)| caller)
    }

    /// F5 - Send his call (callsign field contents only)
    /// Available in any state with an active caller
    fn handle_f5_his_call(&mut self) {
        let entered_call = self.callsign_input.trim().to_uppercase();
        if entered_call.is_empty() {
            return;
        }

        // Need at least one active caller
        if self.context.active_callers.is_empty() {
            return;
        }

        // Stop any current audio
        let _ = self.cmd_tx.send(AudioCommand::StopAll);

        // Find matching caller and select them (clone to avoid borrow issues)
        let matching_caller =
            Self::find_similar_caller(&entered_call, &self.context.active_callers).cloned();
        if let Some(caller) = matching_caller {
            // If multiple callers, narrow down to just this one
            let multiple_callers = self.context.active_callers.len() > 1;
            self.context.select_caller(caller.clone());
            if multiple_callers {
                self.context.set_callers(vec![caller]);
            }
        }

        let exact_match = self
            .context
            .get_current_caller()
            .map(|c| entered_call == c.params.callsign)
            .unwrap_or(false);

        // Send his call
        self.used_f5_callsign = true;
        self.send_his_call();

        // If we have an exact match and no exchange sent yet, wait for the user to send exchange.
        self.context.awaiting_user_exchange =
            exact_match && !self.context.progress.sent_our_exchange;

        // Only expect a repeat when the callsign isn't an exact match.
        self.context.expecting_callsign_repeat = true;
        self.context.allow_callsign_repeat_ack = exact_match;

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::CallsignOnly,
        };
    }

    /// F2 - Send exchange only
    /// Available in any state with an active caller
    fn handle_f2_exchange(&mut self) {
        // Need at least one active caller
        if self.context.active_callers.is_empty() {
            return;
        }

        // Stop any current audio
        let _ = self.cmd_tx.send(AudioCommand::StopAll);

        // If we have an entered callsign, try to select a matching caller (clone to avoid borrow issues)
        let entered_call = self.callsign_input.trim().to_uppercase();
        if !entered_call.is_empty() {
            let matching_caller =
                Self::find_similar_caller(&entered_call, &self.context.active_callers).cloned();
            if let Some(caller) = matching_caller {
                // If multiple callers, narrow down to just this one
                let multiple_callers = self.context.active_callers.len() > 1;
                self.context.select_caller(caller.clone());
                if multiple_callers {
                    self.context.set_callers(vec![caller]);
                }
            }
        }

        // Send exchange only
        self.send_exchange_only();

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::ExchangeOnly,
        };
    }

    fn handle_callsign_submit(&mut self) {
        use rand::Rng;

        let entered_call = self.callsign_input.trim().to_uppercase();
        if entered_call.is_empty() {
            return;
        }

        // Only works when stations are calling
        if self.state != ContestState::StationsCalling {
            return;
        }

        // User has entered a callsign, so they've "received" it
        self.context.progress.received_their_call = true;

        // Find the most similar caller, or fall back to first caller if none match
        let caller = Self::find_similar_caller(&entered_call, &self.context.active_callers)
            .or_else(|| self.context.active_callers.first())
            .cloned();

        if let Some(caller) = caller {
            // Select this caller as the current one
            self.context.select_caller(caller.clone());

            // Check if the entered callsign is correct
            let is_exact_match = entered_call == caller.params.callsign;

            if is_exact_match {
                // Correct callsign - clear any correction state
                self.context.end_correction();
            } else {
                // Incorrect callsign - check if caller will correct
                let mut rng = rand::thread_rng();
                let settings = &self.settings.simulation.call_correction;

                let should_correct = rng.gen::<f32>() < settings.correction_probability
                    && self.context.correction_attempts < settings.max_correction_attempts;

                if should_correct {
                    self.context.correction_in_progress = true;
                    self.context.increment_correction_attempt();
                } else {
                    // Caller won't correct anymore - clear correction state
                    self.context.end_correction();
                }
            }

            // Send our exchange
            self.send_exchange(&entered_call);
            self.state = ContestState::UserTransmitting {
                tx_type: UserTxType::Exchange,
            };
            self.set_exchange_field(self.exchange_focus_index());
        }
    }

    fn handle_exchange_submit(&mut self) {
        let entered_fields = self.normalized_exchange_inputs();
        let entered_callsign = self.callsign_input.trim().to_uppercase();

        // User has entered an exchange, so they've "received" it
        if entered_fields.iter().any(|field| !field.is_empty()) {
            self.context.progress.received_their_exchange = true;
        }

        // Get the expected caller info
        let caller = match self.context.get_current_caller() {
            Some(c) => c.clone(),
            None => return,
        };

        // Must be receiving exchange
        if !matches!(
            self.state,
            ContestState::StationTransmitting {
                tx_type: StationTxType::SendingExchange
            }
        ) {
            return;
        }

        // Validate the entry
        let expected_exchange_str = self.contest.format_exchange(&caller.params.exchange);
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let validation = self.contest.validate(
            &caller.params.callsign,
            &caller.params.exchange,
            &entered_callsign,
            &entered_fields,
            contest_settings,
        );
        let entered_exchange = self.contest.format_received_exchange(&entered_fields);

        let result = QsoResult {
            callsign: entered_callsign.clone(),
            expected_call: caller.params.callsign.clone(),
            expected_exchange: expected_exchange_str.clone(),
            callsign_correct: validation.callsign_correct,
            exchange_correct: validation.exchange_correct,
            points: validation.points,
        };

        // Log QSO to session stats
        self.session_stats.log_qso(QsoRecord {
            expected_callsign: caller.params.callsign.clone(),
            entered_callsign,
            callsign_correct: validation.callsign_correct,
            expected_exchange: expected_exchange_str,
            entered_exchange,
            exchange_correct: validation.exchange_correct,
            station_wpm: caller.params.wpm,
            points: validation.points,
            used_agn_callsign: self.used_agn_callsign,
            used_agn_exchange: self.used_agn_exchange,
            used_f5_callsign: self.used_f5_callsign,
        });

        // Update score
        self.score.add_qso(validation.points);
        self.user_serial += 1;

        // Mark caller as worked in the caller manager
        self.caller_manager.on_qso_complete(caller.params.id);

        // Send TU
        self.send_tu();

        self.last_qso_result = Some(result);
        self.state = ContestState::QsoComplete;

        // Clear inputs and reset correction state
        self.callsign_input.clear();
        self.clear_exchange_inputs();
        self.current_field = InputField::Callsign;
        self.context.end_correction();
    }

    fn handle_agn_request(&mut self) {
        // Only works when receiving exchange
        if !matches!(
            self.state,
            ContestState::StationTransmitting {
                tx_type: StationTxType::SendingExchange
            }
        ) {
            return;
        }

        // Stop any current station audio
        let _ = self.cmd_tx.send(AudioCommand::StopAll);

        // Send the AGN message
        let agn_message = self.settings.user.agn_message.clone();
        let segments = vec![MessageSegment {
            content: agn_message,
            segment_type: MessageSegmentType::Agn,
        }];
        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessageSegmented {
            segments,
            wpm: self.settings.user.wpm,
        });

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::Agn,
        };
        self.used_agn_exchange = true;
    }

    fn handle_callsign_agn_request(&mut self) {
        // Works when stations are calling, or during call correction
        let allowed_in_correction = self.context.correction_in_progress
            && matches!(
                self.state,
                ContestState::StationTransmitting {
                    tx_type: StationTxType::Correction
                }
            );
        if self.state != ContestState::StationsCalling && !allowed_in_correction {
            return;
        }

        // Stop any current station audio
        let _ = self.cmd_tx.send(AudioCommand::StopAll);

        // Send the AGN message
        let agn_message = self.settings.user.agn_message.clone();
        let segments = vec![MessageSegment {
            content: agn_message,
            segment_type: MessageSegmentType::Agn,
        }];
        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessageSegmented {
            segments,
            wpm: self.settings.user.wpm,
        });

        // Mark that we expect the caller to repeat their callsign
        self.context.expecting_callsign_repeat = true;
        self.context.allow_callsign_repeat_ack = false;

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::Agn,
        };
        self.used_agn_callsign = true;
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::StationComplete(id) => {
                    self.caller_manager.station_audio_complete(id);
                    self.on_station_audio_complete(id);
                }
                AudioEvent::UserMessageComplete => {
                    self.on_user_message_complete();
                }
                AudioEvent::UserSegmentComplete(segment_type) => {
                    // Update QsoProgress based on which segment completed
                    match segment_type {
                        MessageSegmentType::TheirCallsign => {
                            self.context.progress.sent_their_call = true;
                        }
                        MessageSegmentType::OurExchange => {
                            self.context.progress.sent_our_exchange = true;
                        }
                        MessageSegmentType::Cq
                        | MessageSegmentType::Tu
                        | MessageSegmentType::Agn => {}
                    }
                }
            }
        }
    }

    fn on_station_audio_complete(&mut self, _id: crate::messages::StationId) {
        match self.state {
            ContestState::StationTransmitting { tx_type } => {
                match tx_type {
                    StationTxType::RequestingAgn => {
                        // Caller finished requesting AGN, wait for user to resend
                        self.state = ContestState::StationsCalling;
                    }
                    StationTxType::Correction => {
                        // Caller finished sending correction, wait for user to fix
                        self.state = ContestState::StationsCalling;
                    }
                    StationTxType::SendingExchange => {
                        // Exchange received, stay in this state for user to log
                    }
                }
            }
            ContestState::StationsCalling => {
                // Station audio complete while in StationsCalling - stay there
            }
            _ => {}
        }
    }

    fn on_user_message_complete(&mut self) {
        match self.state {
            ContestState::CallingCq => {
                // CQ finished, wait for callers
                self.state = ContestState::WaitingForCallers;
                self.last_cq_finished = Some(Instant::now());
            }
            ContestState::UserTransmitting { tx_type } => {
                match tx_type {
                    UserTxType::Exchange | UserTxType::ExchangeOnly => {
                        // Exchange sent, wait for station response
                        self.context.set_wait(250);
                        self.state = ContestState::WaitingForStation;
                    }
                    UserTxType::CallsignOnly => {
                        // Partial query sent, wait for station response
                        self.context.set_wait(250);
                        self.state = ContestState::WaitingForStation;
                    }
                    UserTxType::Agn => {
                        // AGN request sent, wait for station response
                        self.context.set_wait(250);
                        self.state = ContestState::WaitingForStation;
                    }
                    UserTxType::Tu => {
                        // TU sent, check for tail-ender
                        self.try_spawn_tail_ender();
                    }
                }
            }
            ContestState::QsoComplete => {
                // TU finished - maybe a tail-ender jumps in
                self.try_spawn_tail_ender();
            }
            _ => {}
        }
    }

    /// Try to spawn a tail-ender after TU
    fn try_spawn_tail_ender(&mut self) {
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let tail_ender = self.caller_manager.try_spawn_tail_ender(
            self.contest.as_ref(),
            contest_settings,
            Some(&self.settings.user.callsign),
            Some(&self.cty),
        );

        let Some(params) = tail_ender else {
            self.state = ContestState::Idle;
            return;
        };

        // Prepare the tail-ender
        let callers = vec![ActiveCaller { params }];

        // Reset for new QSO
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;
        self.used_f5_callsign = false;
        self.context.reset();
        self.context.set_callers(callers);

        // Wait briefly before tail-ender starts calling
        self.context.set_wait(100);
        self.state = ContestState::WaitingForTailEnder;
    }

    /// Check and handle waiting states
    fn check_waiting_states(&mut self) {
        if !self.context.wait_elapsed() {
            return;
        }

        match self.state {
            ContestState::WaitingForStation => {
                self.handle_station_response();
            }
            ContestState::WaitingForTailEnder => {
                // Start tail-ender audio
                for caller in &self.context.active_callers {
                    let _ = self
                        .cmd_tx
                        .send(AudioCommand::StartStation(caller.params.clone()));
                }
                self.state = ContestState::StationsCalling;
            }
            _ => {}
        }
    }

    /// Handle station response based on QsoProgress
    fn handle_station_response(&mut self) {
        use rand::Rng;

        let caller = match self.context.get_current_caller() {
            Some(c) => c.clone(),
            None => {
                // No current caller - have active callers resend their callsign
                for caller in &self.context.active_callers {
                    let _ = self
                        .cmd_tx
                        .send(AudioCommand::StartStation(caller.params.clone()));
                }
                self.state = ContestState::StationsCalling;
                return;
            }
        };

        // If we're expecting a callsign repeat (after partial query or F8), send callsign or "R R"
        if self.context.expecting_callsign_repeat {
            let allow_ack = self.context.allow_callsign_repeat_ack;
            self.context.expecting_callsign_repeat = false;
            self.context.allow_callsign_repeat_ack = false;

            let message = if allow_ack && rand::thread_rng().gen::<bool>() {
                "R R".to_string()
            } else {
                caller.params.callsign.clone()
            };

            let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                id: caller.params.id,
                callsign: message,
                exchange: caller.params.exchange.clone(),
                frequency_offset_hz: caller.params.frequency_offset_hz,
                wpm: caller.params.wpm,
                amplitude: caller.params.amplitude,
            }));

            self.state = ContestState::StationsCalling;
            return;
        }

        // If we're in correction mode, send the correction
        if self.context.correction_in_progress {
            let mut rng = rand::thread_rng();
            // Send callsign once (75%) or twice (25%) for emphasis
            let message = if rng.gen::<f32>() < 0.75 {
                caller.params.callsign.clone()
            } else {
                format!("{} {}", caller.params.callsign, caller.params.callsign)
            };

            let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                id: caller.params.id,
                callsign: message,
                exchange: caller.params.exchange.clone(),
                frequency_offset_hz: caller.params.frequency_offset_hz,
                wpm: caller.params.wpm,
                amplitude: caller.params.amplitude,
            }));

            self.state = ContestState::StationTransmitting {
                tx_type: StationTxType::Correction,
            };
            return;
        }

        // Determine caller response based on what they've heard
        let response =
            CallerResponse::from_progress_and_context(&self.context.progress, &self.context);

        match response {
            CallerResponse::Confused => {
                // Caller didn't hear their callsign - resend it or send "?"
                let mut rng = rand::thread_rng();
                let message = if rng.gen::<bool>() {
                    caller.params.callsign.clone()
                } else {
                    "?".to_string()
                };

                let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                    id: caller.params.id,
                    callsign: message,
                    exchange: caller.params.exchange.clone(),
                    frequency_offset_hz: caller.params.frequency_offset_hz,
                    wpm: caller.params.wpm,
                    amplitude: caller.params.amplitude,
                }));

                self.state = ContestState::StationsCalling;
            }
            CallerResponse::RequestAgn => {
                // Caller heard their call but not our exchange - request AGN
                let mut rng = rand::thread_rng();
                let agn_message = if rng.gen::<bool>() { "AGN" } else { "?" };

                let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                    id: caller.params.id,
                    callsign: agn_message.to_string(),
                    exchange: caller.params.exchange.clone(),
                    frequency_offset_hz: caller.params.frequency_offset_hz,
                    wpm: caller.params.wpm,
                    amplitude: caller.params.amplitude,
                }));

                self.state = ContestState::StationTransmitting {
                    tx_type: StationTxType::RequestingAgn,
                };
            }
            CallerResponse::SendExchange => {
                // Caller heard everything - send their exchange
                let mut rng = rand::thread_rng();

                // Only allow random AGN before the caller has sent their exchange once
                let allow_random_agn = !self.context.caller_exchange_sent_once;
                if allow_random_agn
                    && rng.gen::<f32>() < self.settings.simulation.agn_request_probability
                {
                    let agn_message = if rng.gen::<bool>() { "AGN" } else { "?" };

                    let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                        id: caller.params.id,
                        callsign: agn_message.to_string(),
                        exchange: caller.params.exchange.clone(),
                        frequency_offset_hz: caller.params.frequency_offset_hz,
                        wpm: caller.params.wpm,
                        amplitude: caller.params.amplitude,
                    }));

                    self.state = ContestState::StationTransmitting {
                        tx_type: StationTxType::RequestingAgn,
                    };
                } else {
                    // Normal flow - send their exchange
                    let exchange_str = self.contest.format_exchange(&caller.params.exchange);

                    let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                        id: caller.params.id,
                        callsign: exchange_str,
                        exchange: caller.params.exchange.clone(),
                        frequency_offset_hz: caller.params.frequency_offset_hz,
                        wpm: caller.params.wpm,
                        amplitude: caller.params.amplitude,
                    }));

                    self.context.caller_exchange_sent_once = true;
                    self.state = ContestState::StationTransmitting {
                        tx_type: StationTxType::SendingExchange,
                    };
                }
            }
            CallerResponse::Wait => {
                // Caller waits silently for the user's exchange.
                self.context.clear_wait();
                self.state = ContestState::StationsCalling;
            }
        }
    }

    fn maybe_spawn_callers(&mut self) {
        if self.state != ContestState::WaitingForCallers {
            return;
        }
        if self.settings_error.is_some() {
            return;
        }

        // Wait a bit after CQ before callers respond
        if let Some(finished) = self.last_cq_finished {
            if finished.elapsed().as_millis() < 300 {
                return;
            }
        }

        // Get callers from the persistent queue
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let responding = self.caller_manager.on_cq_complete(
            self.contest.as_ref(),
            contest_settings,
            Some(&self.settings.user.callsign),
            Some(&self.cty),
        );

        if !responding.is_empty() {
            let callers: Vec<ActiveCaller> = responding
                .into_iter()
                .map(|params| {
                    let _ = self.cmd_tx.send(AudioCommand::StartStation(params.clone()));
                    ActiveCaller { params }
                })
                .collect();

            self.context.set_callers(callers);
            self.state = ContestState::StationsCalling;
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            let settings_valid = self.settings_error.is_none();
            // F1 - Send CQ (always available)
            if i.key_pressed(Key::F1) {
                if settings_valid {
                    let _ = self.cmd_tx.send(AudioCommand::StopAll);
                    self.caller_manager.on_cq_restart();
                    self.callsign_input.clear();
                    self.clear_exchange_inputs();
                    self.current_field = InputField::Callsign;
                    self.send_cq();
                }
            }

            // F2 - Send Exchange only (available in any state with active caller)
            if i.key_pressed(Key::F2) {
                self.handle_f2_exchange();
            }

            // F3 - Send TU
            if i.key_pressed(Key::F3) {
                self.send_tu();
                self.state = ContestState::UserTransmitting {
                    tx_type: UserTxType::Tu,
                };
            }

            // F5 - Send his call only (available in any state with active caller)
            if i.key_pressed(Key::F5) {
                self.handle_f5_his_call();
            }

            // F8 - Request AGN
            if i.key_pressed(Key::F8) {
                if self.context.wants_callsign_repeat()
                    || self.current_field == InputField::Callsign
                {
                    self.handle_callsign_agn_request();
                } else {
                    self.handle_agn_request();
                }
            }

            // F12 - Wipe
            if i.key_pressed(Key::F12) {
                self.callsign_input.clear();
                self.clear_exchange_inputs();
                self.current_field = InputField::Callsign;
            }

            // Up/Down arrows - WPM adjustment
            if i.key_pressed(Key::ArrowUp) && self.settings.user.wpm < 50 {
                self.settings.user.wpm += 1;
                self.settings_changed = true;
            }
            if i.key_pressed(Key::ArrowDown) && self.settings.user.wpm > 15 {
                self.settings.user.wpm -= 1;
                self.settings_changed = true;
            }

            // Enter - Submit current field
            if i.key_pressed(Key::Enter) {
                match self.current_field {
                    InputField::Callsign => {
                        if self.callsign_input.trim().is_empty() {
                            // Empty callsign field - act like F1
                            let _ = self.cmd_tx.send(AudioCommand::StopAll);
                            self.caller_manager.on_cq_restart();
                            self.callsign_input.clear();
                            self.clear_exchange_inputs();
                            self.current_field = InputField::Callsign;
                            self.send_cq();
                        } else {
                            self.handle_callsign_submit();
                        }
                    }
                    InputField::Exchange(_) => {
                        self.handle_exchange_submit();
                    }
                }
            }

            // Escape - Stop transmission
            if i.key_pressed(Key::Escape) {
                let _ = self.cmd_tx.send(AudioCommand::StopAll);
            }

            // Space - advance exchange field (contest logger convention)
            if i.key_pressed(Key::Space) {
                if i.modifiers.shift {
                    self.advance_field_backward();
                } else {
                    self.advance_field_forward();
                }
            }

            // Tab - Switch fields
            if i.key_pressed(Key::Tab) {
                if i.modifiers.shift {
                    self.advance_field_backward();
                } else {
                    self.advance_field_forward();
                }
            }
        });
    }

    fn apply_settings_changes(&mut self) {
        if self.settings_changed {
            let active_id = self.settings.contest.active_contest_id.clone();
            let default_descriptor = self
                .contest_registry
                .first()
                .expect("No contests registered. Add at least one contest file.");
            let active_descriptor = if let Some(entry) = self
                .contest_registry
                .iter()
                .find(|entry| entry.id == active_id)
            {
                entry
            } else {
                self.settings.contest.active_contest_id = default_descriptor.id.to_string();
                default_descriptor
            };

            if active_descriptor.id != self.contest.id() {
                self.contest = (active_descriptor.factory)();
                self.reset_exchange_inputs();
                self.callsign_input.clear();
                self.clear_exchange_inputs();
                self.current_field = InputField::Callsign;
            }

            let contest_settings = self
                .settings
                .contest
                .settings_for_mut(self.contest.as_ref());
            self.settings_error = self.contest.validate_settings(contest_settings).err();
            let callsign_source = self
                .contest
                .callsign_source(contest_settings)
                .unwrap_or_else(|_| {
                    self.contest
                        .callsign_source(&self.contest.default_settings())
                        .expect("Failed to build callsign source")
                });
            self.caller_manager.update_callsigns(callsign_source);

            self.caller_manager
                .update_settings(self.settings.simulation.clone());

            let _ = self
                .cmd_tx
                .send(AudioCommand::UpdateSettings(self.settings.audio.clone()));

            if let Err(_e) = self.settings.save() {
                #[cfg(debug_assertions)]
                eprintln!("Failed to save settings: {}", _e);
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

        // Check waiting states
        self.check_waiting_states();

        // Handle keyboard input
        self.handle_keyboard(ctx);

        // Apply any settings changes
        self.apply_settings_changes();

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
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

        // Settings window
        if self.show_settings {
            let settings = &mut self.settings;
            let settings_changed = &mut self.settings_changed;
            let show_settings = &mut self.show_settings;
            let file_dialog = &mut self.file_dialog;
            let file_dialog_target = &mut self.file_dialog_target;
            let contest_registry = &self.contest_registry;

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("Settings")
                    .with_inner_size([475.0, 600.0]),
                |ctx, _class| {
                    file_dialog.update(ctx);

                    if let Some(path) = file_dialog.take_picked() {
                        if let Some(path_str) = path.to_str() {
                            if let Some(target) = file_dialog_target.take() {
                                match target {
                                    FileDialogTarget::ContestSetting { contest_id, key } => {
                                        let entry = settings
                                            .contest
                                            .contests
                                            .entry(contest_id)
                                            .or_insert_with(|| {
                                                toml::Value::Table(toml::value::Table::new())
                                            });
                                        if let toml::Value::Table(table) = entry {
                                            table.insert(
                                                key,
                                                toml::Value::String(path_str.to_string()),
                                            );
                                            *settings_changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    egui::CentralPanel::default().show(ctx, |ui| {
                        let default_descriptor = contest_registry
                            .first()
                            .expect("No contests registered. Add at least one contest file.");
                        let active_descriptor = contest_registry
                            .iter()
                            .find(|entry| entry.id == settings.contest.active_contest_id)
                            .unwrap_or_else(|| {
                                settings.contest.active_contest_id =
                                    default_descriptor.id.to_string();
                                *settings_changed = true;
                                default_descriptor
                            });
                        let contest_for_settings = (active_descriptor.factory)();
                        render_settings_panel(
                            ui,
                            settings,
                            settings_changed,
                            contest_registry,
                            contest_for_settings.as_ref(),
                            file_dialog,
                            file_dialog_target,
                        );
                    });

                    if ctx.input(|i| i.viewport().close_requested()) {
                        *show_settings = false;
                    }
                },
            );
        }

        // Stats window
        if self.show_stats {
            render_stats_window(ctx, &self.session_stats, &mut self.show_stats);
        }

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            render_main_panel(ui, self);
        });

        if let Some(error) = self.settings_error.clone() {
            egui::Window::new("Invalid Contest Settings")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(error);
                    ui.add_space(8.0);
                    ui.label("Fix the contest settings to continue.");
                });
        }

        ctx.request_repaint();
    }
}
