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

/// Return the pair in ascending order
fn ordered<T: PartialOrd>(a: T, b: T) -> (T, T) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
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
        let mut rng = rand::rng();
        let delay = rng.random_range(min_ms..=max_ms);
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
        let mut rng = rand::rng();

        // Don't replenish too often
        if self.last_replenish.elapsed().as_millis() < 500 {
            return;
        }
        self.last_replenish = Instant::now();

        // Target queue size based on the maximum simultaneous stations
        let target_queue_size =
            (self.settings.max_simultaneous_stations as f32 * 2.5).ceil() as usize;

        // Count active callers (not given up, not worked)
        let mut active_in_queue = self
            .queue
            .iter()
            .filter(|c| c.state != CallerState::GaveUp && c.state != CallerState::Worked)
            .count();

        // Add callers if below target
        while active_in_queue < target_queue_size {
            // Probability check for adding each caller
            if rng.random::<f32>() > self.settings.station_probability {
                break;
            }

            if let Some(caller) = self.create_caller(contest, contest_settings, user_callsign, cty)
            {
                self.queue.push(caller);
                active_in_queue += 1;
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
        let mut rng = rand::rng();

        // Same-country filtering: `same_country_probability` is the fraction of
        // callers that should be from the user's country. Decide up front whether
        // this caller should be domestic or DX, then draw callsigns until one
        // matches. Filtering is skipped if the user's country can't be determined.
        let user_country = if self.settings.same_country_filter_enabled {
            match (user_callsign, cty) {
                (Some(user_call), Some(cty_db)) => {
                    cty_db.lookup_prefix(user_call).map(|p| (p, cty_db))
                }
                _ => None,
            }
        } else {
            None
        };
        let want_domestic = rng.random::<f32>() < self.settings.same_country_probability;

        // If the callsign pool can't supply a match (e.g. a domestic-only pool
        // while DX is wanted), fall back to the last drawn callsign.
        let max_draws = if user_country.is_some() { 200 } else { 1 };
        let mut callsign_and_exchange = None;

        for _ in 0..max_draws {
            let Some((callsign, exchange)) =
                self.callsigns
                    .random(contest, self.serial_counter, contest_settings)
            else {
                break;
            };

            let matches = match &user_country {
                Some((user_prefix, cty_db)) => {
                    let is_domestic = cty_db.lookup_prefix(&callsign).as_ref() == Some(user_prefix);
                    is_domestic == want_domestic
                }
                None => true,
            };

            callsign_and_exchange = Some((callsign, exchange));
            if matches {
                break;
            }
        }

        let (callsign, exchange) = callsign_and_exchange?;
        self.serial_counter += 1;

        // Random parameters
        // Ranges come from user-editable settings, so tolerate min == max and
        // min > max instead of panicking on an empty range.
        let (wpm_lo, wpm_hi) = ordered(self.settings.wpm_min, self.settings.wpm_max);
        let wpm = rng.random_range(wpm_lo..=wpm_hi);
        let half_width = (self.settings.frequency_spread_hz / 2.0).max(0.0);
        let freq_offset = rng.random_range(-half_width..=half_width);
        let (amp_lo, amp_hi) = ordered(self.settings.amplitude_min, self.settings.amplitude_max);
        let amplitude = rng.random_range(amp_lo..=amp_hi);

        // Random patience (1-7 attempts)
        let (patience_lo, patience_hi) = ordered(
            self.pileup_settings.min_patience,
            self.pileup_settings.max_patience,
        );
        let patience = rng.random_range(patience_lo..=patience_hi);

        // Random reaction time (faster operators call sooner)
        let reaction_delay_ms = rng.random_range(100..800);

        self.next_id += 1;

        Some(PersistentCaller {
            params: StationParams {
                id: StationId(self.next_id),
                callsign,
                exchange,
                frequency_offset_hz: freq_offset,
                wpm,
                amplitude,
                reaction_delay_ms,
            },
            patience,
            attempts: 0,
            state: CallerState::Waiting,
            ready_at: Instant::now(),
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
        let mut rng = rand::rng();

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
            jitter.insert(caller.params.id, rng.random_range(0..100));
        }
        self.queue.sort_by_key(|c| {
            c.params.reaction_delay_ms + jitter.get(&c.params.id).copied().unwrap_or(0)
        });

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
            if rng.random::<f32>() > call_probability {
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

    /// Try to spawn tail-enders after QSO completion
    /// Returns list of tail-enders that will call (uses geometric decay for multiple callers)
    pub fn try_spawn_tail_ender(
        &mut self,
        contest: &dyn Contest,
        contest_settings: &toml::Value,
        user_callsign: Option<&str>,
        cty: Option<&CtyDat>,
    ) -> Vec<StationParams> {
        let mut rng = rand::rng();

        // Replenish queue first
        self.replenish_queue(contest, contest_settings, user_callsign, cty);

        // Clean up worked/given-up callers
        self.queue
            .retain(|c| c.state != CallerState::Worked && c.state != CallerState::GaveUp);

        // Clear active list for new potential callers
        self.active_ids.clear();

        // Select tail-enders (up to max_simultaneous) with geometric decay
        let mut responding: Vec<StationParams> = Vec::new();
        let max_callers = self.settings.max_simultaneous_stations as usize;
        let decay_factor: f32 = 0.5;

        // Sort by reaction time with a stable random jitter
        let mut jitter: HashMap<StationId, u32> = HashMap::new();
        for caller in &self.queue {
            jitter.insert(caller.params.id, rng.random_range(0..100));
        }
        self.queue.sort_by_key(|c| {
            c.params.reaction_delay_ms + jitter.get(&c.params.id).copied().unwrap_or(0)
        });

        for caller in &mut self.queue {
            if responding.len() >= max_callers {
                break;
            }

            // Only consider waiting callers that are ready
            if caller.state != CallerState::Waiting || !caller.is_ready_to_call() {
                continue;
            }

            // Geometric decay: base_probability * 0.5^slot
            let base_probability = 0.5 + (caller.patience as f32 - 1.0) * 0.1;
            let slot_probability = base_probability * decay_factor.powi(responding.len() as i32);
            if rng.random::<f32>() > slot_probability {
                continue;
            }

            // This caller will tail-end
            caller.mark_calling();
            caller.record_attempt();
            self.active_ids.push(caller.params.id);
            responding.push(caller.params.clone());
        }

        responding
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Keep regression tests bounded even if replenishment stops honoring its target.
    struct FiniteCallsignSource {
        remaining: usize,
    }

    impl CallsignSource for FiniteCallsignSource {
        fn random(
            &mut self,
            _contest: &dyn Contest,
            _serial: u32,
            _settings: &toml::Value,
        ) -> Option<(String, crate::contest::Exchange)> {
            if self.remaining == 0 {
                return None;
            }
            self.remaining -= 1;
            Some((
                format!("W{}AW", self.remaining),
                crate::contest::Exchange::new(vec![]),
            ))
        }
    }

    #[test]
    fn replenish_queue_stops_at_target_at_full_probability() {
        let contest = crate::contest::cqww::make_contest();
        let contest_settings = contest.default_settings();

        for (max_stations, target) in [(1, 3), (2, 5), (5, 13)] {
            let mut manager = CallerManager::new(
                Box::new(FiniteCallsignSource {
                    remaining: target + 1,
                }),
                SimulationSettings {
                    max_simultaneous_stations: max_stations,
                    station_probability: 1.0,
                    ..SimulationSettings::default()
                },
            );
            manager.last_replenish = Instant::now() - Duration::from_millis(500);

            manager.replenish_queue(contest.as_ref(), &contest_settings, None, None);

            assert_eq!(manager.queue.len(), target, "max_stations={max_stations}");
        }
    }

    #[test]
    fn create_caller_tolerates_degenerate_ranges() {
        let contest = crate::contest::cqww::make_contest();
        let contest_settings = contest.default_settings();
        let mut manager = CallerManager::new(
            Box::new(FiniteCallsignSource { remaining: 10 }),
            SimulationSettings {
                // Equal bounds (reachable from the settings sliders) and reversed bounds
                amplitude_min: 0.8,
                amplitude_max: 0.8,
                wpm_min: 36,
                wpm_max: 28,
                frequency_spread_hz: 0.0,
                ..SimulationSettings::default()
            },
        );

        let caller = manager
            .create_caller(contest.as_ref(), &contest_settings, None, None)
            .expect("caller should be created");

        assert_eq!(caller.params.amplitude, 0.8);
        assert_eq!(caller.params.frequency_offset_hz, 0.0);
        assert!((28..=36).contains(&caller.params.wpm));
    }

    #[test]
    fn replenish_queue_counts_existing_callers_and_replaces_finished_callers() {
        let contest = crate::contest::cqww::make_contest();
        let contest_settings = contest.default_settings();
        let mut manager = CallerManager::new(
            Box::new(FiniteCallsignSource { remaining: 10 }),
            SimulationSettings {
                max_simultaneous_stations: 2,
                station_probability: 1.0,
                ..SimulationSettings::default()
            },
        );
        for state in [
            CallerState::Waiting,
            CallerState::Calling,
            CallerState::Worked,
            CallerState::GaveUp,
        ] {
            let mut caller = manager
                .create_caller(contest.as_ref(), &contest_settings, None, None)
                .unwrap();
            caller.state = state;
            manager.queue.push(caller);
        }

        manager.last_replenish = Instant::now() - Duration::from_millis(500);
        manager.replenish_queue(contest.as_ref(), &contest_settings, None, None);

        // Two existing viable callers plus three new ones, and two finished callers.
        assert_eq!(manager.queue.len(), 7);
        assert_eq!(
            manager
                .queue
                .iter()
                .filter(|c| c.state == CallerState::Waiting || c.state == CallerState::Calling)
                .count(),
            5
        );

        // A full queue must stay unchanged on the next eligible refill.
        manager.last_replenish = Instant::now() - Duration::from_millis(500);
        manager.replenish_queue(contest.as_ref(), &contest_settings, None, None);
        assert_eq!(manager.queue.len(), 7);
    }

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

    mod same_country_filter {
        use super::super::*;
        use crate::contest::{cqww, FileCallsignSource};

        const SAMPLES: usize = 400;

        fn cty() -> CtyDat {
            CtyDat::parse(include_str!("../../data/cty.dat"))
        }

        fn manager(enabled: bool, probability: f32) -> CallerManager {
            let path = concat!(env!("CARGO_MANIFEST_DIR"), "/callsigns.txt");
            let source = FileCallsignSource::load(path).expect("callsigns.txt (SCP) should load");
            let settings = SimulationSettings {
                same_country_filter_enabled: enabled,
                same_country_probability: probability,
                ..SimulationSettings::default()
            };
            CallerManager::new(Box::new(source), settings)
        }

        /// Fraction of generated callers that share a country with `user_call`
        fn domestic_fraction(mgr: &mut CallerManager, user_call: &str, cty: &CtyDat) -> f32 {
            let contest = cqww::make_contest();
            let contest_settings = contest.default_settings();
            let mut domestic = 0;
            for _ in 0..SAMPLES {
                let caller = mgr
                    .create_caller(
                        contest.as_ref(),
                        &contest_settings,
                        Some(user_call),
                        Some(cty),
                    )
                    .expect("caller should be created");
                if cty.same_country(user_call, &caller.params.callsign) {
                    domestic += 1;
                }
            }
            domestic as f32 / SAMPLES as f32
        }

        #[test]
        fn probability_one_gives_only_domestic_callers() {
            // Issue #15: NQ0K with probability 1.0 was still getting DX callers
            let cty = cty();
            let frac = domestic_fraction(&mut manager(true, 1.0), "NQ0K", &cty);
            assert_eq!(frac, 1.0);
        }

        #[test]
        fn probability_zero_gives_only_dx_callers() {
            let cty = cty();
            let frac = domestic_fraction(&mut manager(true, 0.0), "NQ0K", &cty);
            assert_eq!(frac, 0.0);
        }

        #[test]
        fn intermediate_probabilities_are_respected() {
            let cty = cty();
            for (p, lo, hi) in [(0.1, 0.03, 0.18), (0.5, 0.42, 0.58), (0.8, 0.72, 0.88)] {
                let frac = domestic_fraction(&mut manager(true, p), "NQ0K", &cty);
                assert!(
                    (lo..=hi).contains(&frac),
                    "p={p}: domestic fraction {frac} outside [{lo}, {hi}]"
                );
            }
        }

        #[test]
        fn works_for_non_us_user() {
            let cty = cty();
            let frac = domestic_fraction(&mut manager(true, 1.0), "DL1ABC", &cty);
            assert_eq!(frac, 1.0);
        }

        #[test]
        fn disabled_filter_uses_natural_pool_mix() {
            let cty = cty();
            let frac = domestic_fraction(&mut manager(false, 1.0), "NQ0K", &cty);
            assert!(frac > 0.0 && frac < 1.0, "unfiltered fraction {frac}");
        }

        /// A callsign source that only ever produces US callsigns
        struct UsOnlySource;
        impl CallsignSource for UsOnlySource {
            fn random(
                &mut self,
                _contest: &dyn Contest,
                _serial: u32,
                _settings: &toml::Value,
            ) -> Option<(String, crate::contest::Exchange)> {
                Some(("W1AW".to_string(), crate::contest::Exchange::new(vec![])))
            }
        }

        #[test]
        fn falls_back_when_pool_cannot_match() {
            // e.g. a Sweepstakes pool (US/VE only) with DX requested: still get callers
            let cty = cty();
            let mut mgr = CallerManager::new(
                Box::new(UsOnlySource),
                SimulationSettings {
                    same_country_filter_enabled: true,
                    same_country_probability: 0.0,
                    ..SimulationSettings::default()
                },
            );
            let contest = cqww::make_contest();
            let caller = mgr.create_caller(
                contest.as_ref(),
                &contest.default_settings(),
                Some("NQ0K"),
                Some(&cty),
            );
            assert_eq!(caller.unwrap().params.callsign, "W1AW");
        }

        #[test]
        fn serial_advances_once_per_caller() {
            let cty = cty();
            let mut mgr = manager(true, 1.0);
            let contest = cqww::make_contest();
            let contest_settings = contest.default_settings();
            for i in 0..20 {
                assert_eq!(mgr.serial_counter, 1 + i);
                mgr.create_caller(
                    contest.as_ref(),
                    &contest_settings,
                    Some("NQ0K"),
                    Some(&cty),
                );
            }
        }
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
