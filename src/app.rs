use crossbeam_channel::{bounded, Receiver, Sender};
use egui::Key;
use egui_file_dialog::FileDialog;
use std::time::Instant;

use crate::audio::AudioEngine;
use crate::config::AppSettings;
use crate::contest::{self, Contest, ContestDescriptor, FieldKind};
use crate::cty::CtyDat;
use crate::messages::{
    AudioCommand, AudioEvent, MessageId, MessageSegment, MessageSegmentType, StationParams,
};
use crate::state::{
    CallsignRepeat, ContestState, QsoContext, StationTxType, StatusColor, UserTxType,
};
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
    /// Why audio could not be started, if it could not
    pub audio_error: Option<String>,
    /// The user message currently on the air. Completion events for any other
    /// message (one we stopped or replaced) are stale and ignored.
    user_tx: Option<MessageId>,
    next_message_id: u32,

    // Contest and station management
    pub contest: Box<dyn Contest>,
    contest_registry: Vec<ContestDescriptor>,
    caller_manager: CallerManager,
    /// Identifies the callsign source currently loaded (contest id, callsign file)
    callsign_source_key: (String, Option<String>),
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

    // Export result for modal dialog
    pub export_result: Option<String>,
}

impl ContestApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let load_result = AppSettings::load_with_notice();
        let settings = load_result.settings;

        // Create channels for audio communication
        let (cmd_tx, cmd_rx) = bounded::<AudioCommand>(64);
        let (event_tx, event_rx) = bounded::<AudioEvent>(64);

        // Create audio engine
        let (audio_engine, audio_error) =
            match AudioEngine::new(cmd_rx, event_tx, settings.audio.clone()) {
                Ok(engine) => (Some(engine), None),
                Err(e) => {
                    #[cfg(debug_assertions)]
                    eprintln!("Failed to initialize audio: {}", e);
                    (None, Some(e.to_string()))
                }
            };

        let mut app = Self::with_parts(settings, load_result.notice, cmd_tx, event_rx);
        app.audio_engine = audio_engine;
        app.audio_error = audio_error;
        app
    }

    /// Build the app around existing audio channels (no audio device required)
    fn with_parts(
        mut settings: AppSettings,
        settings_notice: Option<String>,
        cmd_tx: Sender<AudioCommand>,
        event_rx: Receiver<AudioEvent>,
    ) -> Self {
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
        let callsign_source_key = Self::callsign_source_key(contest.as_ref(), contest_settings);

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
            audio_engine: None,
            audio_error: None,
            user_tx: None,
            next_message_id: 0,
            contest,
            contest_registry,
            caller_manager,
            callsign_source_key,
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
            export_result: None,
        }
    }

    /// What determines the callsign source: the contest and its callsign file
    fn callsign_source_key(
        contest: &dyn Contest,
        contest_settings: &toml::Value,
    ) -> (String, Option<String>) {
        (
            contest.id().to_string(),
            contest_settings
                .get("callsign_file")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        )
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

    /// Put a message from the user on the air, replacing any still being sent
    fn play_user_message(&mut self, segments: Vec<MessageSegment>) {
        self.next_message_id = self.next_message_id.wrapping_add(1);
        let id = MessageId(self.next_message_id);
        self.user_tx = Some(id);
        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessageSegmented {
            id,
            segments,
            wpm: self.settings.user.wpm,
        });
    }

    /// Start a transmission from `caller`, on their frequency and at their speed
    fn station_sends(&self, caller: &ActiveCaller, message: String) {
        let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
            callsign: message,
            reaction_delay_ms: 0,
            ..caller.params.clone()
        }));
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
        let segments = vec![MessageSegment {
            content: message,
            segment_type: MessageSegmentType::Cq,
        }];

        self.play_user_message(segments);

        self.state = ContestState::CallingCq;

        // Reset AGN tracking for new QSO
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;
        self.used_f5_callsign = false;

        // Reset context for new QSO
        self.context.reset();
    }

    fn send_exchange(&mut self, their_call: &str) {
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

        self.play_user_message(segments);
    }

    fn send_exchange_only(&mut self) {
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

        // Use segmented message for element-level tracking
        let segments = vec![MessageSegment {
            content: exchange,
            segment_type: MessageSegmentType::OurExchange,
        }];

        self.play_user_message(segments);
    }

    fn send_tu(&mut self) {
        let message = format!("TU {}", self.settings.user.callsign);
        let segments = vec![MessageSegment {
            content: message,
            segment_type: MessageSegmentType::Tu,
        }];

        self.play_user_message(segments);
    }

    fn send_his_call(&mut self) {
        let their_call = self.callsign_input.trim().to_uppercase();
        if their_call.is_empty() {
            return;
        }

        // Use segmented message for element-level tracking
        let segments = vec![MessageSegment {
            content: their_call,
            segment_type: MessageSegmentType::TheirCallsign,
        }];

        self.play_user_message(segments);
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

        // Sending the right call settles any correction the caller was making.
        if exact_match {
            self.context.end_correction();
        }

        // A caller who now has both their call and our exchange has nothing to
        // repeat - they go straight to sending their exchange. Otherwise they
        // repeat their call, or may just acknowledge it when we had it right.
        let caller_has_everything = exact_match && self.context.progress.sent_our_exchange;
        self.context.callsign_repeat_request = if caller_has_everything {
            None
        } else if exact_match {
            Some(CallsignRepeat::MayAcknowledge)
        } else {
            Some(CallsignRepeat::Required)
        };

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

        // Their exchange comes next, so be ready to copy it
        if self.current_field == InputField::Callsign {
            self.set_exchange_field(self.exchange_focus_index());
        }

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

        // Only works when callers are waiting on us
        if !self.state.accepts_callsign_entry() {
            return;
        }

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
                let mut rng = rand::rng();
                let settings = &self.settings.simulation.call_correction;

                let should_correct = rng.random::<f32>() < settings.correction_probability
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

        // Clear inputs; the QSO is over, so the worked station is no longer in play
        self.callsign_input.clear();
        self.clear_exchange_inputs();
        self.current_field = InputField::Callsign;
        self.context.reset();
    }

    /// F8 - ask for a repeat. The station repeats what they are on: their
    /// exchange once they have sent it, their callsign before that.
    fn handle_f8_agn(&mut self) {
        if matches!(
            self.state,
            ContestState::StationTransmitting {
                tx_type: StationTxType::SendingExchange
            }
        ) {
            self.handle_agn_request();
        } else {
            self.handle_callsign_agn_request();
        }
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
        self.play_user_message(segments);

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::Agn,
        };
        self.used_agn_exchange = true;
    }

    fn handle_callsign_agn_request(&mut self) {
        // Works when callers are waiting on us
        if !self.state.accepts_callsign_entry() {
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
        self.play_user_message(segments);

        // The caller is to repeat their callsign
        self.context.callsign_repeat_request = Some(CallsignRepeat::Required);

        self.state = ContestState::UserTransmitting {
            tx_type: UserTxType::Agn,
        };
        self.used_agn_callsign = true;
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::UserMessageComplete(id) => {
                    if self.user_tx == Some(id) {
                        self.user_tx = None;
                        self.on_user_message_complete();
                    }
                }
                AudioEvent::UserSegmentComplete(id, segment_type) => {
                    if self.user_tx != Some(id) {
                        continue;
                    }
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

    /// Esc: stop our transmission and put the state where the user can carry on
    fn stop_user_transmission(&mut self) {
        let _ = self.cmd_tx.send(AudioCommand::StopUser);
        if self.user_tx.take().is_none() {
            return;
        }

        match self.state {
            ContestState::CallingCq => {
                self.state = ContestState::Idle;
            }
            ContestState::QsoComplete
            | ContestState::UserTransmitting {
                tx_type: UserTxType::Tu,
            } => {
                // The QSO is over either way; a cut-off TU draws no tail-enders
                self.context.reset();
                self.state = ContestState::Idle;
            }
            ContestState::UserTransmitting { .. } => {
                // Callers are still there, waiting for us to try again
                self.state = if self.context.active_callers.is_empty() {
                    ContestState::Idle
                } else {
                    ContestState::StationsCalling
                };
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

    /// TU finished: pileup leftovers and tail-enders may call straight away
    fn try_spawn_tail_ender(&mut self) {
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let responding = self.caller_manager.on_tu_complete(
            self.contest.as_ref(),
            contest_settings,
            Some(&self.settings.user.callsign),
            Some(&self.cty),
        );

        if responding.is_empty() {
            // Nobody on frequency: don't leave the previous QSO's callers in play
            self.context.reset();
            self.state = ContestState::Idle;
            return;
        }

        // Reset for new QSO
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;
        self.used_f5_callsign = false;
        self.context.reset();

        // Start audio for each tail-ender and build caller list
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

    /// Check and handle waiting states
    fn check_waiting_states(&mut self) {
        if !self.context.wait_elapsed() {
            return;
        }

        match self.state {
            ContestState::WaitingForStation => {
                self.handle_station_response();
            }
            _ => {}
        }
    }

    /// The station's turn: decide what they send in reply to our last transmission
    fn handle_station_response(&mut self) {
        use rand::Rng;
        let mut rng = rand::rng();

        // A repeat request is answered by whoever replies now, exactly once.
        let repeat_request = self.context.callsign_repeat_request.take();

        let caller = match self.context.get_current_caller() {
            Some(c) => c.clone(),
            None => {
                // Nobody singled out - all callers send their callsign again
                for caller in &self.context.active_callers {
                    let _ = self
                        .cmd_tx
                        .send(AudioCommand::StartStation(caller.params.clone()));
                }
                self.state = ContestState::StationsCalling;
                return;
            }
        };
        let callsign = caller.params.callsign.clone();

        if let Some(request) = repeat_request {
            let message = if request == CallsignRepeat::MayAcknowledge && rng.random::<bool>() {
                "R R".to_string()
            } else {
                callsign
            };
            self.station_sends(&caller, message);
            self.state = ContestState::StationsCalling;
            return;
        }

        if self.context.correction_in_progress {
            // Send callsign once (75%) or twice (25%) for emphasis
            let message = if rng.random::<f32>() < 0.75 {
                callsign
            } else {
                format!("{} {}", callsign, callsign)
            };
            self.station_sends(&caller, message);
            self.state = ContestState::StationTransmitting {
                tx_type: StationTxType::Correction,
            };
            // The fix has to be made in the callsign field
            self.current_field = InputField::Callsign;
            return;
        }

        // Otherwise the caller responds to what they have heard so far
        match CallerResponse::from_progress(&self.context.progress) {
            CallerResponse::Confused => {
                self.context.increment_confused_attempt();

                // max_correction_attempts is used for both the correction loop
                // and the confused loop escape threshold
                let max_attempts = self
                    .settings
                    .simulation
                    .call_correction
                    .max_correction_attempts;

                if self.context.confused_attempts > max_attempts {
                    // Caller gives up waiting and sends callsign + exchange together,
                    // like an impatient operator in a real contest
                    let exchange_str = self.contest.format_exchange(&caller.params.exchange);
                    self.station_sends(&caller, format!("{} {}", callsign, exchange_str));

                    // Advance progress so the QSO can complete
                    self.context.progress.sent_their_call = true;
                    self.context.progress.sent_our_exchange = true;
                    self.context.caller_exchange_sent_once = true;

                    self.state = ContestState::StationTransmitting {
                        tx_type: StationTxType::SendingExchange,
                    };
                } else {
                    // Caller didn't hear their callsign - resend it or send "?"
                    let message = if rng.random::<bool>() {
                        callsign
                    } else {
                        "?".to_string()
                    };
                    self.station_sends(&caller, message);
                    self.state = ContestState::StationsCalling;
                }
            }
            CallerResponse::RequestAgn => {
                // Caller heard their call but not our exchange
                self.station_requests_agn(&caller);
            }
            CallerResponse::SendExchange => {
                // Caller heard everything. Before sending their exchange for the
                // first time they may still ask us to repeat ours.
                let wants_repeat = !self.context.caller_exchange_sent_once
                    && rng.random::<f32>() < self.settings.simulation.agn_request_probability;
                if wants_repeat {
                    self.station_requests_agn(&caller);
                } else {
                    let exchange_str = self.contest.format_exchange(&caller.params.exchange);
                    self.station_sends(&caller, exchange_str);
                    self.context.caller_exchange_sent_once = true;
                    self.state = ContestState::StationTransmitting {
                        tx_type: StationTxType::SendingExchange,
                    };
                }
            }
        }
    }

    /// The caller asks us to send our exchange again
    fn station_requests_agn(&mut self, caller: &ActiveCaller) {
        use rand::Rng;
        let message = if rand::rng().random::<bool>() {
            "AGN"
        } else {
            "?"
        };
        self.station_sends(caller, message.to_string());
        self.state = ContestState::StationTransmitting {
            tx_type: StationTxType::RequestingAgn,
        };
    }

    fn maybe_spawn_callers(&mut self) {
        if self.state != ContestState::WaitingForCallers {
            return;
        }
        if self.settings_error.is_some() {
            return;
        }

        // Wait a bit after CQ before callers respond
        // (callers also have individual reaction_delay_ms applied in audio)
        match self.last_cq_finished {
            Some(finished) if finished.elapsed().as_millis() >= 200 => {}
            _ => return,
        }
        // Everyone listening decides once per CQ
        self.last_cq_finished = None;

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

        if responding.is_empty() {
            // Nobody came back: it is up to the user to call again
            self.state = ContestState::Idle;
            return;
        }

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
                self.handle_f8_agn();
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
                self.stop_user_transmission();
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

    /// Rebuild the callsign source if the contest or its callsign file changed.
    /// Reloading is slow and discards the caller queue, so unrelated settings
    /// changes (speed, audio, ...) must not trigger it. Returns whether it reloaded.
    fn reload_callsign_source_if_changed(&mut self) -> bool {
        let contest_settings = self
            .settings
            .contest
            .settings_for_mut(self.contest.as_ref());
        let source_key = Self::callsign_source_key(self.contest.as_ref(), contest_settings);
        if source_key == self.callsign_source_key {
            return false;
        }

        let callsign_source = self
            .contest
            .callsign_source(contest_settings)
            .unwrap_or_else(|_| {
                self.contest
                    .callsign_source(&self.contest.default_settings())
                    .expect("Failed to build callsign source")
            });
        self.caller_manager.update_callsigns(callsign_source);
        self.callsign_source_key = source_key;
        true
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

            self.reload_callsign_source_if_changed();

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
                                    FileDialogTarget::ExportDirectory => {
                                        settings.user.export_directory = path_str.to_string();
                                        *settings_changed = true;
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
            render_stats_window(
                ctx,
                &self.settings,
                &self.session_stats,
                &mut self.show_stats,
                &mut self.export_result,
            );
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

        if let Some(error) = self.audio_error.clone() {
            egui::Window::new("Audio Unavailable")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("No audio could be started, so nothing will be heard.");
                    ui.add_space(8.0);
                    ui.label(error);
                    ui.add_space(8.0);
                    ui.label("Check your audio output device and restart.");
                });
        }

        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contest::Exchange;
    use crate::messages::StationId;
    use crossbeam_channel::unbounded;

    /// App with no audio device and no settings file. The test plays the
    /// audio thread: it reads the commands and reports completions.
    struct Harness {
        app: ContestApp,
        cmd_rx: Receiver<AudioCommand>,
        event_tx: Sender<AudioEvent>,
    }

    fn harness() -> Harness {
        let (cmd_tx, cmd_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let mut settings = AppSettings::default();
        // Deterministic flow: no random repeat requests, callers always correct
        // a busted call, and nobody new shows up on frequency.
        settings.simulation.agn_request_probability = 0.0;
        settings.simulation.call_correction.correction_probability = 1.0;
        settings.simulation.station_probability = 0.0;
        Harness {
            app: ContestApp::with_parts(settings, None, cmd_tx, event_rx),
            cmd_rx,
            event_tx,
        }
    }

    fn caller(id: u32, callsign: &str) -> ActiveCaller {
        ActiveCaller {
            params: StationParams {
                id: StationId(id),
                callsign: callsign.to_string(),
                exchange: Exchange::new(vec!["BOB".to_string(), "123".to_string()]),
                frequency_offset_hz: 0.0,
                wpm: 30,
                amplitude: 1.0,
                reaction_delay_ms: 0,
            },
        }
    }

    impl Harness {
        fn callers_arrive(&mut self, callers: Vec<ActiveCaller>) {
            self.app.context.set_callers(callers);
            self.app.state = ContestState::StationsCalling;
        }

        /// The most recent message the app put on the air for the user
        fn last_user_message(&mut self) -> (MessageId, Vec<MessageSegmentType>) {
            self.cmd_rx
                .try_iter()
                .filter_map(|cmd| match cmd {
                    AudioCommand::PlayUserMessageSegmented { id, segments, .. } => {
                        Some((id, segments.iter().map(|s| s.segment_type).collect()))
                    }
                    _ => None,
                })
                .last()
                .expect("the user should be transmitting")
        }

        /// The user's transmission plays out in full, then the station answers
        fn finish_user_tx(&mut self) {
            let (id, segments) = self.last_user_message();
            for segment in segments {
                self.event_tx
                    .send(AudioEvent::UserSegmentComplete(id, segment))
                    .unwrap();
            }
            self.event_tx
                .send(AudioEvent::UserMessageComplete(id))
                .unwrap();
            self.app.process_audio_events();
            if self.app.state == ContestState::WaitingForStation {
                self.app.handle_station_response();
            }
        }

        /// What stations have been told to send since the last check
        fn station_messages(&mut self) -> Vec<String> {
            self.cmd_rx
                .try_iter()
                .filter_map(|cmd| match cmd {
                    AudioCommand::StartStation(params) => Some(params.callsign),
                    _ => None,
                })
                .collect()
        }
    }

    const SENDING_EXCHANGE: ContestState = ContestState::StationTransmitting {
        tx_type: StationTxType::SendingExchange,
    };
    const CORRECTION: ContestState = ContestState::StationTransmitting {
        tx_type: StationTxType::Correction,
    };

    #[test]
    fn repeat_request_to_pileup_does_not_leak_into_next_transmission() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW"), caller(2, "K3LR")]);

        // "?" to the whole pileup: everyone sends their call again
        h.app.handle_f8_agn();
        h.finish_user_tx();
        assert_eq!(h.app.state, ContestState::StationsCalling);
        assert_eq!(h.station_messages(), ["W1AW", "K3LR"]);

        // Now we copy one and send call + exchange: they must answer with
        // their exchange, not repeat their call again.
        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);
        assert_eq!(h.station_messages(), ["BOB 123"]);
    }

    #[test]
    fn correcting_the_call_with_f5_gets_the_exchange() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        // Busted call: the station corrects us and focus returns to the call
        h.app.callsign_input = "W1AX".to_string();
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, CORRECTION);
        assert_eq!(h.app.current_field, InputField::Callsign);

        // Fix it and send just the call: correction is over, exchange follows
        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_f5_his_call();
        h.finish_user_tx();
        assert!(!h.app.context.correction_in_progress);
        assert_eq!(h.app.state, SENDING_EXCHANGE);
    }

    #[test]
    fn correcting_the_call_with_enter_gets_the_exchange() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        h.app.callsign_input = "W1AX".to_string();
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, CORRECTION);

        // No need to wait for the correction to finish playing
        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);
    }

    #[test]
    fn worked_station_is_gone_after_logging() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);

        h.app.exchange_inputs = vec!["BOB".to_string(), "123".to_string()];
        h.app.handle_exchange_submit();
        assert_eq!(h.app.score.qso_count, 1);
        assert_eq!(h.app.state, ContestState::QsoComplete);

        // TU finishes with nobody tail-ending
        h.finish_user_tx();
        assert_eq!(h.app.state, ContestState::Idle);

        // F2 / Enter must not raise the worked station again or log twice
        h.app.handle_f2_exchange();
        assert_eq!(h.app.state, ContestState::Idle);
        h.app.handle_exchange_submit();
        assert_eq!(h.app.score.qso_count, 1);
    }

    #[test]
    fn f8_repeats_the_exchange_after_f5_then_f2() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        // Work them with F5 (call) then F2 (exchange), never pressing Enter
        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_f5_his_call();
        h.finish_user_tx();
        h.app.handle_f2_exchange();
        assert_ne!(h.app.current_field, InputField::Callsign);
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);
        h.station_messages();

        // Missed it: F8 gets the exchange again
        h.app.handle_f8_agn();
        assert_eq!(
            h.app.state,
            ContestState::UserTransmitting {
                tx_type: UserTxType::Agn
            }
        );
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);
        assert_eq!(h.station_messages(), ["BOB 123"]);
    }

    #[test]
    fn escape_during_exchange_lets_enter_resend() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_callsign_submit();
        let (stopped, _) = h.last_user_message();
        h.app.stop_user_transmission();
        assert_eq!(h.app.state, ContestState::StationsCalling);

        // A completion for the stopped message must not move things along
        h.event_tx
            .send(AudioEvent::UserMessageComplete(stopped))
            .unwrap();
        h.app.process_audio_events();
        assert_eq!(h.app.state, ContestState::StationsCalling);

        // Enter works again
        h.app.handle_callsign_submit();
        h.finish_user_tx();
        assert_eq!(h.app.state, SENDING_EXCHANGE);
    }

    #[test]
    fn escape_during_cq_returns_to_idle() {
        let mut h = harness();
        h.app.send_cq();
        assert_eq!(h.app.state, ContestState::CallingCq);

        h.app.stop_user_transmission();
        assert_eq!(h.app.state, ContestState::Idle);

        // Escape with nothing on the air changes nothing
        h.callers_arrive(vec![caller(1, "W1AW")]);
        h.app.stop_user_transmission();
        assert_eq!(h.app.state, ContestState::StationsCalling);
    }

    #[test]
    fn events_from_a_replaced_message_are_ignored() {
        let mut h = harness();
        h.callers_arrive(vec![caller(1, "W1AW")]);

        // Call + exchange goes out, then we restart with a new CQ before the
        // audio thread's reports for the old message have been read.
        h.app.callsign_input = "W1AW".to_string();
        h.app.handle_callsign_submit();
        let (old, _) = h.last_user_message();
        h.app.send_cq();

        h.event_tx
            .send(AudioEvent::UserSegmentComplete(
                old,
                MessageSegmentType::TheirCallsign,
            ))
            .unwrap();
        h.event_tx
            .send(AudioEvent::UserMessageComplete(old))
            .unwrap();
        h.app.process_audio_events();

        // Still calling CQ, and the new QSO starts with nothing sent
        assert_eq!(h.app.state, ContestState::CallingCq);
        assert!(!h.app.context.progress.sent_their_call);

        h.finish_user_tx();
        assert_eq!(h.app.state, ContestState::WaitingForCallers);
    }

    #[test]
    fn unanswered_cq_returns_to_idle() {
        let mut h = harness();
        h.app.send_cq();
        h.finish_user_tx();
        assert_eq!(h.app.state, ContestState::WaitingForCallers);

        // Too soon for anyone to have come back
        h.app.maybe_spawn_callers();
        assert_eq!(h.app.state, ContestState::WaitingForCallers);

        // Nobody is on frequency: the CQ goes unanswered
        h.app.last_cq_finished = Some(Instant::now() - std::time::Duration::from_millis(250));
        h.app.maybe_spawn_callers();
        assert_eq!(h.app.state, ContestState::Idle);
        assert!(h.station_messages().is_empty());
    }

    #[test]
    fn callsign_source_reloads_only_when_its_file_changes() {
        let mut h = harness();
        let app = &mut h.app;

        // Unrelated settings changes leave the source (and caller queue) alone
        app.settings.user.wpm += 1;
        app.settings.audio.noise_level = 0.1;
        assert!(!app.reload_callsign_source_if_changed());

        let contest_settings = app.settings.contest.settings_for_mut(app.contest.as_ref());
        contest_settings
            .as_table_mut()
            .expect("contest settings are a table")
            .insert(
                "callsign_file".to_string(),
                toml::Value::String("some_other_file.txt".to_string()),
            );
        assert!(app.reload_callsign_source_if_changed());
        assert!(!app.reload_callsign_source_if_changed());
    }
}
