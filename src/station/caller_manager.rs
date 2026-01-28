use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::config::{PileupSettings, SimulationSettings};
use crate::contest::{CallsignSource, Contest};
use crate::cty::CtyDat;
use crate::messages::{StationId, StationParams};
use crate::state::{QsoContext, QsoProgress};

/// How a caller should respond based on what they've heard
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerResponse {
    /// Caller is confused - resends their callsign or sends "?"
    Confused,
    /// Caller heard their call but not exchange - sends "AGN" or "?"
    RequestAgn,
    /// Caller heard both call and exchange - sends their exchange
    SendExchange,
    /// Caller waits silently for the user's exchange
    Wait,
}

impl CallerResponse {
    /// Determine the appropriate caller response based on QSO progress
    ///
    /// This implements the caller response table:
    /// | sent_their_call | sent_our_exchange | Response |
    /// |-----------------|-------------------|----------|
    /// | false           | false             | Confused |
    /// | true            | false             | RequestAgn |
    /// | false           | true              | Confused (unusual case) |
    /// | true            | true              | SendExchange |
    pub fn from_progress(progress: &QsoProgress) -> Self {
        match (progress.sent_their_call, progress.sent_our_exchange) {
            (true, true) => CallerResponse::SendExchange,
            (true, false) => CallerResponse::RequestAgn,
            // If we haven't sent their call, they're confused regardless
            (false, _) => CallerResponse::Confused,
        }
    }

    /// Determine caller response using both QSO progress and context.
    pub fn from_progress_and_context(progress: &QsoProgress, context: &QsoContext) -> Self {
        if context.awaiting_user_exchange && progress.sent_their_call && !progress.sent_our_exchange
        {
            return CallerResponse::Wait;
        }

        Self::from_progress(progress)
    }
}

/// State of a caller in the persistent queue
#[derive(Clone, Debug, PartialEq)]
pub enum CallerState {
    /// In queue, waiting for next opportunity to call
    Waiting,
    /// Currently transmitting callsign
    Calling,
    /// Given up and left the frequency
    GaveUp,
    /// Successfully completed QSO
    Worked,
}

/// A caller that persists across CQ cycles
#[derive(Clone, Debug)]
pub struct PersistentCaller {
    pub params: StationParams,
    /// How many attempts this caller is willing to make (1-7)
    pub patience: u8,
    /// How many attempts they've made so far
    pub attempts: u8,
    /// Current state
    pub state: CallerState,
    /// When the caller will be ready to try again
    pub ready_at: Instant,
    /// Delay before responding to CQ (reaction time)
    pub reaction_delay_ms: u32,
}

impl PersistentCaller {
    /// Check if this caller is ready to call (waiting and delay elapsed)
    pub fn is_ready_to_call(&self) -> bool {
        self.state == CallerState::Waiting && Instant::now() >= self.ready_at
    }

    /// Check if caller has given up (exceeded patience)
    pub fn has_given_up(&self) -> bool {
        self.state == CallerState::GaveUp || self.attempts >= self.patience
    }

    /// Record an attempt (increment counter, but don't mark as given up yet)
    pub fn record_attempt(&mut self) {
        self.attempts += 1;
    }

    /// Set delay before next call attempt
    pub fn set_retry_delay(&mut self, min_ms: u32, max_ms: u32) {
        let mut rng = rand::thread_rng();
        let delay = rng.gen_range(min_ms..=max_ms);
        self.ready_at = Instant::now() + Duration::from_millis(delay as u64);
        self.state = CallerState::Waiting;
    }

    /// Mark as currently calling
    pub fn mark_calling(&mut self) {
        self.state = CallerState::Calling;
    }

    /// Mark as successfully worked
    pub fn mark_worked(&mut self) {
        self.state = CallerState::Worked;
    }
}

/// Manages a persistent queue of callers
pub struct CallerManager {
    callsigns: Box<dyn CallsignSource>,
    settings: SimulationSettings,
    pileup_settings: PileupSettings,
    next_id: u32,
    serial_counter: u32,

    /// The persistent queue of callers
    queue: Vec<PersistentCaller>,

    /// Callers currently active (subset of queue that's calling)
    active_ids: Vec<StationId>,

    /// Last time we tried to add callers to the queue
    last_replenish: Instant,
}

impl CallerManager {
    pub fn new(callsigns: Box<dyn CallsignSource>, settings: SimulationSettings) -> Self {
        let pileup_settings = settings.pileup.clone();
        Self {
            callsigns,
            settings,
            pileup_settings,
            next_id: 0,
            serial_counter: 1,
            queue: Vec::new(),
            active_ids: Vec::new(),
            last_replenish: Instant::now(),
        }
    }

    /// Update settings
    pub fn update_settings(&mut self, settings: SimulationSettings) {
        self.pileup_settings = settings.pileup.clone();
        self.settings = settings;
    }

    /// Update callsign pool (regular)
    pub fn update_callsigns(&mut self, callsigns: Box<dyn CallsignSource>) {
        self.callsigns = callsigns;
        // Clear queue when callsigns change
        self.queue.clear();
        self.active_ids.clear();
    }

    /// Add new callers to the queue (call periodically to simulate stations finding frequency)
    fn replenish_queue(
        &mut self,
        contest: &dyn Contest,
        contest_settings: &toml::Value,
        user_callsign: Option<&str>,
        cty: Option<&CtyDat>,
    ) {
        let mut rng = rand::thread_rng();

        // Don't replenish too often
        if self.last_replenish.elapsed().as_millis() < 500 {
            return;
        }
        self.last_replenish = Instant::now();

        // Target queue size based on station probability (more likely = bigger pileup)
        let target_queue_size =
            (self.settings.max_simultaneous_stations as f32 * 2.5).ceil() as usize;

        // Count active callers (not given up, not worked)
        let active_in_queue = self
            .queue
            .iter()
            .filter(|c| c.state != CallerState::GaveUp && c.state != CallerState::Worked)
            .count();

        // Add callers if below target
        while active_in_queue < target_queue_size {
            // Probability check for adding each caller
            if rng.gen::<f32>() > self.settings.station_probability {
                break;
            }

            if let Some(caller) = self.create_caller(contest, contest_settings, user_callsign, cty)
            {
                self.queue.push(caller);
            } else {
                break;
            }
        }
    }

    /// Create a new persistent caller
    fn create_caller(
        &mut self,
        contest: &dyn Contest,
        contest_settings: &toml::Value,
        user_callsign: Option<&str>,
        cty: Option<&CtyDat>,
    ) -> Option<PersistentCaller> {
        let mut rng = rand::thread_rng();

        // Pick a random callsign with same-country filtering
        let max_retries = 10;
        let mut callsign_and_exchange = None;

        for _ in 0..max_retries {
            let Some((callsign, exchange)) =
                self.callsigns
                    .random(contest, self.serial_counter, contest_settings)
            else {
                break;
            };
            self.serial_counter += 1;

            // Check if we should reject this callsign due to same-country
            let should_reject = if self.settings.same_country_filter_enabled {
                match (user_callsign, cty) {
                    (Some(user_call), Some(cty_db)) => {
                        if cty_db.same_country(user_call, &callsign) {
                            rng.gen::<f32>() > self.settings.same_country_probability
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            } else {
                false
            };

            if !should_reject {
                callsign_and_exchange = Some((callsign, exchange));
                break;
            }
        }

        let (callsign, exchange) = callsign_and_exchange?;

        // Random parameters
        let wpm = rng.gen_range(self.settings.wpm_min..=self.settings.wpm_max);
        let freq_offset =
            rng.gen_range(-self.settings.frequency_spread_hz..self.settings.frequency_spread_hz);
        let amplitude = rng.gen_range(self.settings.amplitude_min..self.settings.amplitude_max);

        // Random patience (1-7 attempts)
        let patience =
            rng.gen_range(self.pileup_settings.min_patience..=self.pileup_settings.max_patience);

        // Random reaction time (faster operators call sooner)
        let reaction_delay_ms = rng.gen_range(100..800);

        self.next_id += 1;

        Some(PersistentCaller {
            params: StationParams {
                id: StationId(self.next_id),
                callsign,
                exchange,
                frequency_offset_hz: freq_offset,
                wpm,
                amplitude,
            },
            patience,
            attempts: 0,
            state: CallerState::Waiting,
            ready_at: Instant::now(),
            reaction_delay_ms,
        })
    }

    /// Called when CQ completes - select callers to respond
    /// Returns list of callers that will call (as StationParams for audio)
    pub fn on_cq_complete(
        &mut self,
        contest: &dyn Contest,
        contest_settings: &toml::Value,
        user_callsign: Option<&str>,
        cty: Option<&CtyDat>,
    ) -> Vec<StationParams> {
        let mut rng = rand::thread_rng();

        // First, replenish the queue
        self.replenish_queue(contest, contest_settings, user_callsign, cty);

        // Clean up worked/given-up callers
        self.queue
            .retain(|c| c.state != CallerState::Worked && c.state != CallerState::GaveUp);

        // Reset active list
        self.active_ids.clear();

        // Select callers to respond (up to max_simultaneous)
        let mut responding: Vec<StationParams> = Vec::new();
        let max_callers = self.settings.max_simultaneous_stations as usize;

        // Sort by reaction time with a stable random jitter (precomputed)
        let mut jitter: HashMap<StationId, u32> = HashMap::new();
        for caller in &self.queue {
            jitter.insert(caller.params.id, rng.gen_range(0..100));
        }
        self.queue
            .sort_by_key(|c| c.reaction_delay_ms + jitter.get(&c.params.id).copied().unwrap_or(0));

        for caller in &mut self.queue {
            if responding.len() >= max_callers {
                break;
            }

            // Only consider waiting callers
            if caller.state != CallerState::Waiting {
                continue;
            }

            // Probability check - more persistent callers are more likely to call
            let call_probability = 0.5 + (caller.patience as f32 - 1.0) * 0.1;
            if rng.gen::<f32>() > call_probability {
                continue;
            }

            // This caller will respond
            caller.mark_calling();
            caller.record_attempt();
            self.active_ids.push(caller.params.id);
            responding.push(caller.params.clone());
        }

        responding
    }

    /// Called when user presses F1 again without completing QSO
    /// Callers that were calling get another chance (patience permitting)
    pub fn on_cq_restart(&mut self) {
        for caller in &mut self.queue {
            if caller.state == CallerState::Calling {
                if caller.has_given_up() {
                    caller.state = CallerState::GaveUp;
                } else {
                    // Set retry delay
                    caller.set_retry_delay(
                        self.pileup_settings.retry_delay_min_ms,
                        self.pileup_settings.retry_delay_max_ms,
                    );
                }
            }
        }
        self.active_ids.clear();
    }

    /// Called when a QSO is completed with a specific station
    pub fn on_qso_complete(&mut self, station_id: StationId) {
        if let Some(caller) = self.queue.iter_mut().find(|c| c.params.id == station_id) {
            caller.mark_worked();
        }
        self.active_ids.retain(|id| *id != station_id);
    }

    /// Called when audio for a station completes
    pub fn station_audio_complete(&mut self, _id: StationId) {
        // Currently just for tracking - caller remains in active state
        // until either worked or CQ restart
    }

    /// Try to spawn a tail-ender after QSO completion
    /// Returns Some if a tail-ender will call
    pub fn try_spawn_tail_ender(
        &mut self,
        contest: &dyn Contest,
        contest_settings: &toml::Value,
        user_callsign: Option<&str>,
        cty: Option<&CtyDat>,
    ) -> Option<StationParams> {
        let mut rng = rand::thread_rng();

        // Probability check
        if rng.gen::<f32>() > self.settings.station_probability {
            return None;
        }

        // Replenish queue first
        self.replenish_queue(contest, contest_settings, user_callsign, cty);

        // Clean up worked/given-up callers
        self.queue
            .retain(|c| c.state != CallerState::Worked && c.state != CallerState::GaveUp);

        // Clear active list for new potential caller
        self.active_ids.clear();

        // Find a waiting caller to be the tail-ender
        for caller in &mut self.queue {
            if caller.state == CallerState::Waiting && caller.is_ready_to_call() {
                caller.mark_calling();
                caller.record_attempt();
                self.active_ids.push(caller.params.id);
                return Some(caller.params.clone());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caller_response_from_progress() {
        // Nothing sent - caller is confused
        let progress = QsoProgress {
            sent_their_call: false,
            sent_our_exchange: false,
            received_their_call: false,
            received_their_exchange: false,
        };
        assert_eq!(
            CallerResponse::from_progress(&progress),
            CallerResponse::Confused
        );

        // Sent their call but not our exchange - caller requests AGN
        let progress = QsoProgress {
            sent_their_call: true,
            sent_our_exchange: false,
            received_their_call: false,
            received_their_exchange: false,
        };
        assert_eq!(
            CallerResponse::from_progress(&progress),
            CallerResponse::RequestAgn
        );

        // Sent both - caller sends their exchange
        let progress = QsoProgress {
            sent_their_call: true,
            sent_our_exchange: true,
            received_their_call: false,
            received_their_exchange: false,
        };
        assert_eq!(
            CallerResponse::from_progress(&progress),
            CallerResponse::SendExchange
        );

        // Edge case: sent exchange but not their call - still confused
        let progress = QsoProgress {
            sent_their_call: false,
            sent_our_exchange: true,
            received_their_call: false,
            received_their_exchange: false,
        };
        assert_eq!(
            CallerResponse::from_progress(&progress),
            CallerResponse::Confused
        );
    }

    #[test]
    fn test_caller_response_waits_when_awaiting_exchange() {
        let progress = QsoProgress {
            sent_their_call: true,
            sent_our_exchange: false,
            received_their_call: false,
            received_their_exchange: false,
        };
        let mut context = QsoContext::new();
        context.awaiting_user_exchange = true;

        assert_eq!(
            CallerResponse::from_progress_and_context(&progress, &context),
            CallerResponse::Wait
        );
    }
}
