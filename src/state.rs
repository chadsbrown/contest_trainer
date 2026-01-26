//! State machine types for the contest trainer
//!
//! This module defines the information-driven state machine that tracks
//! QSO progress and allows flexible user actions.

use std::time::Instant;

use crate::app::ActiveCaller;

/// Tracks what information has been successfully communicated during a QSO
#[derive(Clone, Debug, Default)]
pub struct QsoProgress {
    /// We have completed sending the caller's callsign
    pub sent_their_call: bool,
    /// We have completed sending our exchange
    pub sent_our_exchange: bool,
    /// We have received the caller's callsign (user entered something)
    pub received_their_call: bool,
    /// We have received the caller's exchange (user entered something)
    pub received_their_exchange: bool,
}

impl QsoProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset progress for a new QSO
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Check if we've sent enough info for the caller to send their exchange
    pub fn caller_can_send_exchange(&self) -> bool {
        self.sent_their_call && self.sent_our_exchange
    }

    /// Check if the QSO is complete (all info exchanged)
    pub fn is_complete(&self) -> bool {
        self.sent_their_call
            && self.sent_our_exchange
            && self.received_their_call
            && self.received_their_exchange
    }
}

/// Context data for the current QSO, separate from the state enum
#[derive(Clone, Debug)]
pub struct QsoContext {
    /// Progress tracking for information exchange
    pub progress: QsoProgress,
    /// The current caller we're working (single QSO)
    pub current_caller: Option<ActiveCaller>,
    /// All active callers (for pileup situations)
    pub active_callers: Vec<ActiveCaller>,
    /// Whether we're in a call correction flow
    pub correction_in_progress: bool,
    /// Number of correction attempts
    pub correction_attempts: u8,
    /// Timer for waiting states
    pub wait_until: Option<Instant>,
    /// Whether we're expecting caller to repeat their callsign (after partial query or F8)
    pub expecting_callsign_repeat: bool,
}

impl Default for QsoContext {
    fn default() -> Self {
        Self::new()
    }
}

impl QsoContext {
    pub fn new() -> Self {
        Self {
            progress: QsoProgress::new(),
            current_caller: None,
            active_callers: Vec::new(),
            correction_in_progress: false,
            correction_attempts: 0,
            wait_until: None,
            expecting_callsign_repeat: false,
        }
    }

    /// Reset context for a new QSO
    pub fn reset(&mut self) {
        self.progress.reset();
        self.current_caller = None;
        self.active_callers.clear();
        self.correction_in_progress = false;
        self.correction_attempts = 0;
        self.wait_until = None;
        self.expecting_callsign_repeat = false;
    }

    /// Set up context for a new set of callers
    pub fn set_callers(&mut self, callers: Vec<ActiveCaller>) {
        self.active_callers = callers;
        if self.active_callers.len() == 1 {
            self.current_caller = Some(self.active_callers[0].clone());
        } else {
            self.current_caller = None;
        }
    }

    /// Select a specific caller from the pileup
    pub fn select_caller(&mut self, caller: ActiveCaller) {
        self.current_caller = Some(caller);
    }

    /// Get the current caller (single caller or selected from pileup)
    pub fn get_current_caller(&self) -> Option<&ActiveCaller> {
        self.current_caller.as_ref()
    }

    /// Start correction flow
    pub fn start_correction(&mut self) {
        self.correction_in_progress = true;
        self.correction_attempts = 0;
    }

    /// Increment correction attempt
    pub fn increment_correction_attempt(&mut self) {
        self.correction_attempts += 1;
    }

    /// End correction flow
    pub fn end_correction(&mut self) {
        self.correction_in_progress = false;
    }

    /// Set the wait timer
    pub fn set_wait(&mut self, duration_ms: u64) {
        self.wait_until = Some(Instant::now() + std::time::Duration::from_millis(duration_ms));
    }

    /// Check if wait timer has elapsed
    pub fn wait_elapsed(&self) -> bool {
        match self.wait_until {
            Some(until) => Instant::now() >= until,
            None => true,
        }
    }

    /// Clear the wait timer
    pub fn clear_wait(&mut self) {
        self.wait_until = None;
    }
}

/// What type of message the user is transmitting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserTxType {
    /// CQ call
    Cq,
    /// Sending their call + our exchange (Enter in callsign field)
    Exchange,
    /// Sending just their callsign (F5)
    CallsignOnly,
    /// Sending just our exchange (F2)
    ExchangeOnly,
    /// Sending AGN/? request (F8)
    Agn,
    /// Sending TU (F3 or after logging)
    Tu,
}

/// What type of message a station is transmitting
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationTxType {
    /// Station(s) sending their callsign (responding to CQ or repeat)
    CallingUs,
    /// Station sending their exchange
    SendingExchange,
    /// Station requesting AGN (sending "AGN" or "?")
    RequestingAgn,
    /// Station sending callsign correction
    Correction,
}

/// Simplified contest state machine - describes who is transmitting/waiting
///
/// Context like current_caller, correction_in_progress, etc. is stored
/// in QsoContext rather than duplicated across state variants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContestState {
    /// Idle - waiting for user to start
    #[default]
    Idle,

    /// User is sending CQ
    CallingCq,

    /// CQ finished, waiting for stations to call
    WaitingForCallers,

    /// Station(s) are calling us (callers stored in QsoContext)
    StationsCalling,

    /// User is transmitting (type in tx_type)
    UserTransmitting { tx_type: UserTxType },

    /// Brief pause before station responds (wait_until in QsoContext)
    WaitingForStation,

    /// Station is transmitting (type in tx_type)
    StationTransmitting { tx_type: StationTxType },

    /// QSO complete, TU being sent or just sent
    QsoComplete,

    /// Brief pause before tail-ender starts calling
    WaitingForTailEnder,
}

impl ContestState {
    /// Check if user is currently transmitting
    pub fn is_user_transmitting(&self) -> bool {
        matches!(
            self,
            ContestState::CallingCq | ContestState::UserTransmitting { .. }
        )
    }

    /// Check if a station is currently transmitting
    pub fn is_station_transmitting(&self) -> bool {
        matches!(
            self,
            ContestState::StationsCalling | ContestState::StationTransmitting { .. }
        )
    }

    /// Check if we're in any waiting state
    pub fn is_waiting(&self) -> bool {
        matches!(
            self,
            ContestState::WaitingForCallers
                | ContestState::WaitingForStation
                | ContestState::WaitingForTailEnder
        )
    }

    /// Get status text and color for UI display
    pub fn status_text(&self, context: &QsoContext) -> (&'static str, StatusColor) {
        match self {
            ContestState::Idle => ("Press F1/Enter to call CQ", StatusColor::Gray),
            ContestState::CallingCq => ("Calling CQ...", StatusColor::Yellow),
            ContestState::WaitingForCallers => ("Waiting for callers...", StatusColor::LightBlue),
            ContestState::StationsCalling => {
                if context.correction_in_progress {
                    ("Fix callsign and press Enter", StatusColor::Orange)
                } else {
                    ("Station calling - enter callsign", StatusColor::Green)
                }
            }
            ContestState::UserTransmitting { tx_type } => match tx_type {
                UserTxType::Cq => ("Calling CQ...", StatusColor::Yellow),
                UserTxType::Exchange => ("Sending exchange...", StatusColor::Yellow),
                UserTxType::CallsignOnly => {
                    if context.active_callers.len() > 1 {
                        ("Querying partial...", StatusColor::Yellow)
                    } else {
                        ("Sending callsign...", StatusColor::Yellow)
                    }
                }
                UserTxType::ExchangeOnly => ("Sending exchange...", StatusColor::Yellow),
                UserTxType::Agn => ("Requesting repeat...", StatusColor::Yellow),
                UserTxType::Tu => ("Sending TU...", StatusColor::Yellow),
            },
            ContestState::WaitingForStation => {
                if context.correction_in_progress {
                    ("Waiting for correction...", StatusColor::LightBlue)
                } else {
                    ("Waiting for response...", StatusColor::LightBlue)
                }
            }
            ContestState::StationTransmitting { tx_type } => match tx_type {
                StationTxType::CallingUs => {
                    ("Station calling - enter callsign", StatusColor::Green)
                }
                StationTxType::SendingExchange => (
                    "Receiving exchange - press Enter to log",
                    StatusColor::Green,
                ),
                StationTxType::RequestingAgn => {
                    ("Station requests repeat - press F2", StatusColor::Orange)
                }
                StationTxType::Correction => {
                    ("Station correcting callsign...", StatusColor::Orange)
                }
            },
            ContestState::QsoComplete => ("QSO logged! Press F1 for next", StatusColor::Green),
            ContestState::WaitingForTailEnder => ("QSO logged! Waiting...", StatusColor::Green),
        }
    }
}

/// Status colors for UI display
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusColor {
    Gray,
    Yellow,
    LightBlue,
    Green,
    Orange,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qso_progress_default() {
        let progress = QsoProgress::new();
        assert!(!progress.sent_their_call);
        assert!(!progress.sent_our_exchange);
        assert!(!progress.received_their_call);
        assert!(!progress.received_their_exchange);
        assert!(!progress.caller_can_send_exchange());
        assert!(!progress.is_complete());
    }

    #[test]
    fn test_qso_progress_caller_can_send_exchange() {
        let mut progress = QsoProgress::new();

        // Can't send exchange if we haven't sent their call
        progress.sent_our_exchange = true;
        assert!(!progress.caller_can_send_exchange());

        // Can't send exchange if we haven't sent our exchange
        progress.sent_their_call = true;
        progress.sent_our_exchange = false;
        assert!(!progress.caller_can_send_exchange());

        // Can send exchange if we've sent both
        progress.sent_our_exchange = true;
        assert!(progress.caller_can_send_exchange());
    }

    #[test]
    fn test_qso_progress_is_complete() {
        let mut progress = QsoProgress::new();

        progress.sent_their_call = true;
        assert!(!progress.is_complete());

        progress.sent_our_exchange = true;
        assert!(!progress.is_complete());

        progress.received_their_call = true;
        assert!(!progress.is_complete());

        progress.received_their_exchange = true;
        assert!(progress.is_complete());
    }

    #[test]
    fn test_qso_progress_reset() {
        let mut progress = QsoProgress::new();
        progress.sent_their_call = true;
        progress.sent_our_exchange = true;
        progress.received_their_call = true;
        progress.received_their_exchange = true;

        progress.reset();

        assert!(!progress.sent_their_call);
        assert!(!progress.sent_our_exchange);
        assert!(!progress.received_their_call);
        assert!(!progress.received_their_exchange);
    }

    #[test]
    fn test_contest_state_helpers() {
        let state = ContestState::Idle;
        assert!(!state.is_user_transmitting());
        assert!(!state.is_station_transmitting());
        assert!(!state.is_waiting());

        let state = ContestState::CallingCq;
        assert!(state.is_user_transmitting());
        assert!(!state.is_station_transmitting());

        let state = ContestState::UserTransmitting {
            tx_type: UserTxType::Exchange,
        };
        assert!(state.is_user_transmitting());
        assert!(!state.is_station_transmitting());

        let state = ContestState::StationTransmitting {
            tx_type: StationTxType::CallingUs,
        };
        assert!(!state.is_user_transmitting());
        assert!(state.is_station_transmitting());

        let state = ContestState::StationsCalling;
        assert!(state.is_station_transmitting());

        let state = ContestState::WaitingForStation;
        assert!(state.is_waiting());
    }

    #[test]
    fn test_qso_context_callers() {
        use crate::contest::Exchange;
        use crate::messages::{StationId, StationParams};

        let mut context = QsoContext::new();

        let caller1 = ActiveCaller {
            params: StationParams {
                id: StationId(1),
                callsign: "W1AW".to_string(),
                exchange: Exchange::CqWw {
                    rst: "599".to_string(),
                    zone: 5,
                },
                frequency_offset_hz: 0.0,
                wpm: 25,
                amplitude: 1.0,
            },
        };

        let caller2 = ActiveCaller {
            params: StationParams {
                id: StationId(2),
                callsign: "K3LR".to_string(),
                exchange: Exchange::CqWw {
                    rst: "599".to_string(),
                    zone: 5,
                },
                frequency_offset_hz: 100.0,
                wpm: 30,
                amplitude: 0.8,
            },
        };

        // Single caller - should auto-select
        context.set_callers(vec![caller1.clone()]);
        assert_eq!(context.active_callers.len(), 1);
        assert!(context.current_caller.is_some());
        assert_eq!(
            context.current_caller.as_ref().unwrap().params.callsign,
            "W1AW"
        );

        // Multiple callers - should not auto-select
        context.set_callers(vec![caller1.clone(), caller2.clone()]);
        assert_eq!(context.active_callers.len(), 2);
        assert!(context.current_caller.is_none());

        // Manual selection
        context.select_caller(caller2.clone());
        assert!(context.current_caller.is_some());
        assert_eq!(
            context.current_caller.as_ref().unwrap().params.callsign,
            "K3LR"
        );

        // Reset clears everything
        context.reset();
        assert!(context.active_callers.is_empty());
        assert!(context.current_caller.is_none());
    }
}
