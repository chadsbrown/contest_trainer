use crossbeam_channel::{bounded, Receiver, Sender};
use egui::Key;
use egui_file_dialog::FileDialog;
use std::time::Instant;

use crate::audio::AudioEngine;
use crate::config::AppSettings;
use crate::contest::ContestType;
use crate::contest::{self, Contest};
use crate::cty::CtyDat;
use crate::messages::{AudioCommand, AudioEvent, StationParams};
use crate::station::{CallerManager, CallsignPool, CwtCallsignPool};
use crate::stats::{QsoRecord, SessionStats};
use crate::ui::{render_main_panel, render_settings_panel, render_stats_window, FileDialogTarget};

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
    QueryingPartial { callers: Vec<ActiveCaller> },
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
    /// User requested AGN for callsign (cursor in callsign field)
    SendingCallsignAgn { callers: Vec<ActiveCaller> },
    /// Waiting for station(s) to resend callsign after AGN
    WaitingForCallsignAgn {
        callers: Vec<ActiveCaller>,
        wait_until: Instant,
    },
    /// Caller is requesting AGN (sending "AGN" or "?")
    CallerRequestingAgn { caller: ActiveCaller },
    /// Waiting for user to resend exchange after caller requested AGN
    WaitingForUserExchangeRepeat { caller: ActiveCaller },
    /// QSO complete, showing result
    QsoComplete,
    /// Brief pause before tail-ender starts calling
    WaitingForTailEnder {
        callers: Vec<ActiveCaller>,
        wait_until: Instant,
    },
    /// Station is sending callsign correction (user had wrong call)
    SendingCallCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// Brief pause before station sends call correction
    WaitingToSendCallCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
        wait_until: Instant,
    },
    /// Waiting for user to correct callsign and resend exchange
    WaitingForCallCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// User is sending exchange but callsign was wrong - will trigger correction
    SendingExchangeWillCorrect {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// User requested callsign repeat (F8) while in call correction flow
    SendingCallsignAgnFromCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// Waiting for station to repeat callsign during call correction flow
    WaitingForCallsignAgnFromCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
        wait_until: Instant,
    },
    /// Station is repeating callsign after user requested repeat during correction
    SendingCorrectionRepeat {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// User sent partial query (F5) while in call correction flow
    QueryingPartialFromCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
    },
    /// Waiting for station to respond to partial query during call correction flow
    WaitingForPartialResponseFromCorrection {
        caller: ActiveCaller,
        correction_attempts: u8,
        wait_until: Instant,
    },
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
    caller_manager: CallerManager,
    user_serial: u32,
    cty: CtyDat,

    // UI state
    pub show_settings: bool,
    settings_changed: bool,

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

    // File dialog for settings
    file_dialog: FileDialog,
    file_dialog_target: Option<FileDialogTarget>,
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
                #[cfg(debug_assertions)]
                eprintln!("Failed to initialize audio: {}", e);
                let _ = e;
                None
            }
        };

        // Create contest
        let contest = contest::create_contest(settings.contest.contest_type);

        // Load CTY database for country lookups
        let cty_data = include_str!("../data/cty.dat");
        let cty = CtyDat::parse(cty_data);

        // Load callsigns and create caller manager based on contest type
        let caller_manager = if settings.contest.contest_type == ContestType::Cwt {
            let cwt_callsigns = CwtCallsignPool::load(&settings.contest.cwt_callsign_file)
                .unwrap_or_else(|_| CwtCallsignPool::default_pool());
            CallerManager::new_cwt(cwt_callsigns, settings.simulation.clone())
        } else {
            let callsigns = CallsignPool::load(&settings.contest.callsign_file)
                .unwrap_or_else(|_| CallsignPool::default_pool());
            CallerManager::new(callsigns, settings.simulation.clone())
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
            caller_manager,
            user_serial: 1,
            cty,
            show_settings: false,
            settings_changed: false,
            last_cq_finished: None,
            noise_enabled,
            saved_noise_level,
            session_stats: SessionStats::new(),
            show_stats: false,
            used_agn_callsign: false,
            used_agn_exchange: false,
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

    fn send_cq(&mut self) {
        let cq_prefix = self.settings.contest.cq_message.trim();
        let callsign = self.settings.user.callsign.trim();
        let message = format!("{} {}", cq_prefix, callsign);
        let wpm = self.settings.user.wpm;

        let _ = self
            .cmd_tx
            .send(AudioCommand::PlayUserMessage { message, wpm });

        self.state = ContestState::CallingCq;

        // Reset AGN tracking for new QSO
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;
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

    fn send_exchange_only(&mut self) {
        let exchange = self.contest.user_exchange(
            &self.settings.user.callsign,
            self.user_serial,
            self.settings.user.zone,
            &self.settings.user.section,
            &self.settings.user.name,
        );

        let wpm = self.settings.user.wpm;

        let _ = self.cmd_tx.send(AudioCommand::PlayUserMessage {
            message: exchange,
            wpm,
        });
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
            .max_by(|(_, sim_a), (_, sim_b)| sim_a.total_cmp(sim_b))
            .map(|(caller, _)| caller)
    }

    fn handle_partial_query(&mut self) {
        let partial = self.callsign_input.trim().to_uppercase();
        if partial.is_empty() {
            return;
        }

        // Works when stations are calling OR when waiting for call correction
        let (callers, correction_context) = match &self.state {
            ContestState::StationsCalling { callers } => (callers.clone(), None),
            ContestState::WaitingForCallCorrection {
                caller,
                correction_attempts,
            } => (vec![caller.clone()], Some(*correction_attempts)),
            _ => return,
        };

        // Send the partial query
        self.send_partial_query(&partial);

        // Transition to appropriate state based on context
        if let Some(correction_attempts) = correction_context {
            // In correction context, always use the single caller (no matching needed)
            let caller = callers.into_iter().next().unwrap();
            self.state = ContestState::QueryingPartialFromCorrection {
                caller,
                correction_attempts,
            };
        } else {
            // Normal flow - find the most similar caller
            let matching_caller = Self::find_similar_caller(&partial, &callers);
            let matching: Vec<ActiveCaller> = matching_caller.into_iter().cloned().collect();
            self.state = ContestState::QueryingPartial { callers: matching };
        }
    }

    fn handle_callsign_submit(&mut self) {
        self.handle_callsign_submit_internal(0);
    }

    fn handle_callsign_submit_internal(&mut self, correction_attempts: u8) {
        use rand::Rng;

        let entered_call = self.callsign_input.trim().to_uppercase();
        if entered_call.is_empty() {
            return;
        }

        if let ContestState::StationsCalling { ref callers } = self.state {
            // Find the most similar caller, or fall back to first caller if none match
            let caller = Self::find_similar_caller(&entered_call, callers)
                .or_else(|| callers.first())
                .cloned();

            if let Some(caller) = caller {
                // Check if the entered callsign is correct
                let is_exact_match = entered_call == caller.params.callsign;

                if is_exact_match {
                    // Correct callsign - proceed normally
                    self.send_exchange(&entered_call);
                    self.state = ContestState::SendingExchange { caller };
                    self.current_field = InputField::Exchange;
                } else {
                    // Incorrect callsign - check if caller will correct
                    let mut rng = rand::thread_rng();
                    let settings = &self.settings.simulation.call_correction;

                    let should_correct = rng.gen::<f32>() < settings.correction_probability
                        && correction_attempts < settings.max_correction_attempts;

                    if should_correct {
                        // Caller will correct - send our exchange first, then they'll correct
                        self.send_exchange(&entered_call);

                        self.state = ContestState::SendingExchangeWillCorrect {
                            caller,
                            correction_attempts: correction_attempts + 1,
                        };
                        self.current_field = InputField::Exchange;
                    } else {
                        // Caller won't correct - proceed normally (user will get penalty)
                        self.send_exchange(&entered_call);
                        self.state = ContestState::SendingExchange { caller };
                        self.current_field = InputField::Exchange;
                    }
                }
            }
            // If no similar caller found, do nothing - user should press F1 to CQ again
        } else if let ContestState::WaitingForCallCorrection {
            ref caller,
            correction_attempts: attempts,
        } = self.state
        {
            // User is retrying after a correction
            let caller = caller.clone();
            let is_exact_match = entered_call == caller.params.callsign;

            if is_exact_match {
                // Now correct - proceed normally
                self.send_exchange(&entered_call);
                self.state = ContestState::SendingExchange { caller };
                self.current_field = InputField::Exchange;
            } else {
                // Still wrong - check if caller will try again
                let mut rng = rand::thread_rng();
                let settings = &self.settings.simulation.call_correction;

                let should_correct_again = rng.gen::<f32>() < settings.correction_probability
                    && attempts < settings.max_correction_attempts;

                if should_correct_again {
                    self.send_exchange(&entered_call);

                    self.state = ContestState::SendingExchangeWillCorrect {
                        caller,
                        correction_attempts: attempts + 1,
                    };
                    self.current_field = InputField::Exchange;
                } else {
                    // Caller gives up correcting - proceed with wrong call
                    self.send_exchange(&entered_call);
                    self.state = ContestState::SendingExchange { caller };
                    self.current_field = InputField::Exchange;
                }
            }
        }
    }

    fn handle_exchange_submit(&mut self) {
        let entered_exchange = self.exchange_input.trim().to_uppercase();
        let entered_callsign = self.callsign_input.trim().to_uppercase();

        // Get the expected caller info
        let (expected_call, expected_exchange_obj, station_wpm, station_id) = match &self.state {
            ContestState::ReceivingExchange { caller } => (
                caller.params.callsign.clone(),
                caller.params.exchange.clone(),
                caller.params.wpm,
                caller.params.id,
            ),
            _ => return,
        };

        // Validate the entry
        let expected_exchange_str = self.contest.format_sent_exchange(&expected_exchange_obj);
        let validation = self.contest.validate(
            &expected_call,
            &expected_exchange_obj,
            &entered_callsign,
            &entered_exchange,
        );

        let result = QsoResult {
            callsign: entered_callsign.clone(),
            expected_call: expected_call.clone(),
            expected_exchange: expected_exchange_str.clone(),
            callsign_correct: validation.callsign_correct,
            exchange_correct: validation.exchange_correct,
            points: validation.points,
        };

        // Log QSO to session stats
        self.session_stats.log_qso(QsoRecord {
            expected_callsign: expected_call.clone(),
            entered_callsign,
            callsign_correct: validation.callsign_correct,
            expected_exchange: expected_exchange_str,
            entered_exchange,
            exchange_correct: validation.exchange_correct,
            station_wpm,
            points: validation.points,
            used_agn_callsign: self.used_agn_callsign,
            used_agn_exchange: self.used_agn_exchange,
        });

        // Update score
        self.score.add_qso(validation.points);
        self.user_serial += 1;

        // Mark caller as worked in the caller manager
        self.caller_manager.on_qso_complete(station_id);

        // Send TU
        self.send_tu();

        self.last_qso_result = Some(result);
        self.state = ContestState::QsoComplete;

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
        self.used_agn_exchange = true;
    }

    fn handle_callsign_agn_request(&mut self) {
        // Works when stations are calling OR when waiting for call correction
        let (callers, correction_context) = match &self.state {
            ContestState::StationsCalling { callers } => (callers.clone(), None),
            ContestState::WaitingForCallCorrection {
                caller,
                correction_attempts,
            } => (vec![caller.clone()], Some(*correction_attempts)),
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

        // Transition to appropriate state based on context
        if let Some(correction_attempts) = correction_context {
            let caller = callers.into_iter().next().unwrap();
            self.state = ContestState::SendingCallsignAgnFromCorrection {
                caller,
                correction_attempts,
            };
        } else {
            self.state = ContestState::SendingCallsignAgn { callers };
        }
        self.used_agn_callsign = true;
    }

    fn process_audio_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::StationComplete(id) => {
                    self.caller_manager.station_audio_complete(id);

                    // If we were waiting for their exchange, move to logging
                    if let ContestState::ReceivingExchange { ref caller } = self.state {
                        if caller.params.id == id {
                            // Exchange received, user can now log
                        }
                    }

                    // If caller was requesting AGN, transition to waiting for user to resend
                    if let ContestState::CallerRequestingAgn { ref caller } = self.state {
                        if caller.params.id == id {
                            let caller = caller.clone();
                            self.state = ContestState::WaitingForUserExchangeRepeat { caller };
                        }
                    }

                    // If station finished sending call correction
                    if let ContestState::SendingCallCorrection {
                        ref caller,
                        correction_attempts,
                    } = self.state
                    {
                        if caller.params.id == id {
                            let caller = caller.clone();
                            // Wait for user to correct callsign and resend
                            self.state = ContestState::WaitingForCallCorrection {
                                caller,
                                correction_attempts,
                            };
                        }
                    }

                    // If station finished repeating callsign (after F8/F5 during correction flow)
                    if let ContestState::SendingCorrectionRepeat {
                        ref caller,
                        correction_attempts,
                    } = self.state
                    {
                        if caller.params.id == id {
                            let caller = caller.clone();
                            // Return to waiting for user to correct callsign
                            self.state = ContestState::WaitingForCallCorrection {
                                caller,
                                correction_attempts,
                            };
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
                        ContestState::QsoComplete => {
                            // TU finished - maybe a tail-ender jumps in
                            self.try_spawn_tail_ender();
                        }
                        ContestState::QueryingPartial { callers } => {
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
                        ContestState::SendingCallsignAgn { callers } => {
                            // AGN request sent, wait briefly before station(s) resend callsign
                            let callers = callers.clone();
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingForCallsignAgn {
                                callers,
                                wait_until,
                            };
                        }
                        ContestState::SendingExchangeWillCorrect {
                            caller,
                            correction_attempts,
                        } => {
                            // User's exchange finished, now wait briefly before caller corrects
                            let caller = caller.clone();
                            let correction_attempts = *correction_attempts;
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingToSendCallCorrection {
                                caller,
                                correction_attempts,
                                wait_until,
                            };
                        }
                        ContestState::SendingCallsignAgnFromCorrection {
                            caller,
                            correction_attempts,
                        } => {
                            // AGN request sent during correction flow, wait briefly before station repeats
                            let caller = caller.clone();
                            let correction_attempts = *correction_attempts;
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingForCallsignAgnFromCorrection {
                                caller,
                                correction_attempts,
                                wait_until,
                            };
                        }
                        ContestState::QueryingPartialFromCorrection {
                            caller,
                            correction_attempts,
                        } => {
                            // Partial query sent during correction flow, wait briefly before station responds
                            let caller = caller.clone();
                            let correction_attempts = *correction_attempts;
                            let wait_until = Instant::now() + std::time::Duration::from_millis(250);
                            self.state = ContestState::WaitingForPartialResponseFromCorrection {
                                caller,
                                correction_attempts,
                                wait_until,
                            };
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
        // Try to get a tail-ender from the caller manager
        let tail_ender = self.caller_manager.try_spawn_tail_ender(
            self.contest.as_ref(),
            Some(&self.settings.user.callsign),
            Some(&self.cty),
        );

        let Some(params) = tail_ender else {
            // No tail-ender, go to idle
            self.state = ContestState::Idle;
            return;
        };

        // Prepare the tail-ender
        let callers = vec![ActiveCaller { params }];

        // Reset AGN tracking for new QSO (this is a new QSO without F1/CQ)
        self.used_agn_callsign = false;
        self.used_agn_exchange = false;

        // Wait 100ms before the tail-ender starts calling
        let wait_until = Instant::now() + std::time::Duration::from_millis(100);
        self.state = ContestState::WaitingForTailEnder {
            callers,
            wait_until,
        };
    }

    fn check_waiting_for_tail_ender(&mut self) {
        if let ContestState::WaitingForTailEnder {
            callers,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let callers = callers.clone();

                // Now start the audio for the tail-ender(s)
                for caller in &callers {
                    let _ = self
                        .cmd_tx
                        .send(AudioCommand::StartStation(caller.params.clone()));
                }

                self.state = ContestState::StationsCalling { callers };
            }
        }
    }

    fn check_waiting_to_send_exchange(&mut self) {
        use rand::Rng;

        if let ContestState::WaitingToSendExchange { caller, wait_until } = &self.state {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();

                // Randomly decide if the caller will request AGN
                let mut rng = rand::thread_rng();
                if rng.gen::<f32>() < self.settings.simulation.agn_request_probability {
                    // Caller requests AGN - send "AGN" or "?"
                    let agn_message = if rng.gen::<bool>() { "AGN" } else { "?" };

                    let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                        id: caller.params.id,
                        callsign: agn_message.to_string(),
                        exchange: caller.params.exchange.clone(),
                        frequency_offset_hz: caller.params.frequency_offset_hz,
                        wpm: caller.params.wpm,
                        amplitude: caller.params.amplitude,
                    }));

                    self.state = ContestState::CallerRequestingAgn { caller };
                } else {
                    // Normal flow - have the station send only their exchange (not callsign again)
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

    fn check_waiting_for_callsign_agn(&mut self) {
        if let ContestState::WaitingForCallsignAgn {
            callers,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let callers = callers.clone();

                // Have station(s) resend their callsign
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

                // Go back to StationsCalling
                self.state = ContestState::StationsCalling { callers };
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

                if callers.is_empty() {
                    // No matching callers - no response, go back to waiting for callers
                    self.state = ContestState::WaitingForCallers;
                    self.last_cq_finished = Some(Instant::now());
                } else {
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
    }

    fn check_waiting_to_send_call_correction(&mut self) {
        use rand::Rng;

        if let ContestState::WaitingToSendCallCorrection {
            caller,
            correction_attempts,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();
                let correction_attempts = *correction_attempts;
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

                self.state = ContestState::SendingCallCorrection {
                    caller,
                    correction_attempts,
                };
            }
        }
    }

    fn check_waiting_for_callsign_agn_from_correction(&mut self) {
        if let ContestState::WaitingForCallsignAgnFromCorrection {
            caller,
            correction_attempts,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();
                let correction_attempts = *correction_attempts;

                // Have station resend their callsign
                let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                    id: caller.params.id,
                    callsign: caller.params.callsign.clone(),
                    exchange: caller.params.exchange.clone(),
                    frequency_offset_hz: caller.params.frequency_offset_hz,
                    wpm: caller.params.wpm,
                    amplitude: caller.params.amplitude,
                }));

                // Track that station is sending callsign repeat, will return to WaitingForCallCorrection when done
                self.state = ContestState::SendingCorrectionRepeat {
                    caller,
                    correction_attempts,
                };
            }
        }
    }

    fn check_waiting_for_partial_response_from_correction(&mut self) {
        if let ContestState::WaitingForPartialResponseFromCorrection {
            caller,
            correction_attempts,
            wait_until,
        } = &self.state
        {
            if Instant::now() >= *wait_until {
                let caller = caller.clone();
                let correction_attempts = *correction_attempts;

                // Have station send their callsign
                let _ = self.cmd_tx.send(AudioCommand::StartStation(StationParams {
                    id: caller.params.id,
                    callsign: caller.params.callsign.clone(),
                    exchange: caller.params.exchange.clone(),
                    frequency_offset_hz: caller.params.frequency_offset_hz,
                    wpm: caller.params.wpm,
                    amplitude: caller.params.amplitude,
                }));

                // Track that station is sending callsign, will return to WaitingForCallCorrection when done
                self.state = ContestState::SendingCorrectionRepeat {
                    caller,
                    correction_attempts,
                };
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

        // Get callers from the persistent queue
        let responding = self.caller_manager.on_cq_complete(
            self.contest.as_ref(),
            Some(&self.settings.user.callsign),
            Some(&self.cty),
        );

        if !responding.is_empty() {
            let mut callers = Vec::new();

            for params in responding {
                // Station sends their callsign
                let _ = self.cmd_tx.send(AudioCommand::StartStation(params.clone()));
                callers.push(ActiveCaller { params });
            }

            self.state = ContestState::StationsCalling { callers };
        }
    }

    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // F1 - Send CQ (always available - persistent callers may retry)
            if i.key_pressed(Key::F1) {
                // Stop any playing audio
                let _ = self.cmd_tx.send(AudioCommand::StopAll);
                // Notify caller manager that CQ is restarting (callers get another chance)
                self.caller_manager.on_cq_restart();
                self.callsign_input.clear();
                self.exchange_input.clear();
                self.current_field = InputField::Callsign;
                // Send CQ
                self.send_cq();
            }

            // F2 - Send Exchange (uses callsign from input field if available)
            if i.key_pressed(Key::F2) {
                // Handle resending exchange when caller requested AGN
                if let ContestState::WaitingForUserExchangeRepeat { ref caller } = self.state {
                    let caller = caller.clone();
                    self.send_exchange_only();
                    self.state = ContestState::SendingExchange { caller };
                } else if let ContestState::StationsCalling { ref callers } = self.state {
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

            // F8 - Request AGN (ask station to repeat)
            // In callsign field: ask station(s) to repeat callsign
            // In exchange field: ask station to repeat exchange
            if i.key_pressed(Key::F8) {
                match self.current_field {
                    InputField::Callsign => self.handle_callsign_agn_request(),
                    InputField::Exchange => self.handle_agn_request(),
                }
            }

            // F12 - Wipe (clear callsign and exchange fields)
            if i.key_pressed(Key::F12) {
                self.callsign_input.clear();
                self.exchange_input.clear();
                self.current_field = InputField::Callsign;
            }

            // Up arrow - Increase WPM
            if i.key_pressed(Key::ArrowUp) {
                if self.settings.user.wpm < 50 {
                    self.settings.user.wpm += 1;
                    self.settings_changed = true;
                }
            }

            // Down arrow - Decrease WPM
            if i.key_pressed(Key::ArrowDown) {
                if self.settings.user.wpm > 15 {
                    self.settings.user.wpm -= 1;
                    self.settings_changed = true;
                }
            }

            // Enter - Submit current field (or send CQ if callsign field is empty)
            if i.key_pressed(Key::Enter) {
                match self.current_field {
                    InputField::Callsign => {
                        if self.callsign_input.trim().is_empty() {
                            // Empty callsign field - act like F1 (send CQ)
                            let _ = self.cmd_tx.send(AudioCommand::StopAll);
                            self.caller_manager.on_cq_restart();
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
                self.caller_manager.update_cwt_callsigns(cwt_callsigns);
            } else {
                let callsigns = CallsignPool::load(&self.settings.contest.callsign_file)
                    .unwrap_or_else(|_| CallsignPool::default_pool());
                self.caller_manager.update_callsigns(callsigns);
            }

            // Update caller manager settings
            self.caller_manager
                .update_settings(self.settings.simulation.clone());

            // Update audio settings
            let _ = self
                .cmd_tx
                .send(AudioCommand::UpdateSettings(self.settings.audio.clone()));

            // Save settings to file
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

        // Check if waiting to send exchange
        self.check_waiting_to_send_exchange();

        // Check if waiting for AGN response
        self.check_waiting_for_agn();

        // Check if waiting for callsign AGN response
        self.check_waiting_for_callsign_agn();

        // Check if waiting for partial response
        self.check_waiting_for_partial_response();

        // Check if waiting to send call correction
        self.check_waiting_to_send_call_correction();

        // Check if waiting for callsign AGN response during correction flow
        self.check_waiting_for_callsign_agn_from_correction();

        // Check if waiting for partial response during correction flow
        self.check_waiting_for_partial_response_from_correction();

        // Check if waiting for tail-ender
        self.check_waiting_for_tail_ender();

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

        // Settings window (separate OS window)
        if self.show_settings {
            let settings = &mut self.settings;
            let settings_changed = &mut self.settings_changed;
            let show_settings = &mut self.show_settings;
            let file_dialog = &mut self.file_dialog;
            let file_dialog_target = &mut self.file_dialog_target;

            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("Settings")
                    .with_inner_size([475.0, 600.0]),
                |ctx, _class| {
                    // Update file dialog
                    file_dialog.update(ctx);

                    // Check if a file was picked
                    if let Some(path) = file_dialog.take_picked() {
                        if let Some(path_str) = path.to_str() {
                            match file_dialog_target {
                                Some(FileDialogTarget::CallsignFile) => {
                                    settings.contest.callsign_file = path_str.to_string();
                                    *settings_changed = true;
                                }
                                Some(FileDialogTarget::CwtCallsignFile) => {
                                    settings.contest.cwt_callsign_file = path_str.to_string();
                                    *settings_changed = true;
                                }
                                None => {}
                            }
                            *file_dialog_target = None;
                        }
                    }

                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_settings_panel(
                            ui,
                            settings,
                            settings_changed,
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

        // Stats window (separate OS window)
        if self.show_stats {
            render_stats_window(ctx, &self.session_stats, &mut self.show_stats);
        }

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            render_main_panel(ui, self);
        });

        // Request continuous repaints for audio processing
        ctx.request_repaint();
    }
}
