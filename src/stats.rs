use std::collections::HashMap;

/// Record of a single QSO for analysis
#[derive(Clone, Debug)]
pub struct QsoRecord {
    pub expected_callsign: String,
    pub entered_callsign: String,
    pub callsign_correct: bool,
    pub expected_exchange: String,
    pub entered_exchange: String,
    pub exchange_correct: bool,
    pub station_wpm: u8,
    pub points: u32,
    pub used_agn_callsign: bool,
    pub used_agn_exchange: bool,
}

/// Session statistics collector and analyzer
#[derive(Clone, Debug, Default)]
pub struct SessionStats {
    pub qsos: Vec<QsoRecord>,
}

/// Analysis results for display
#[derive(Clone, Debug, Default)]
pub struct StatsAnalysis {
    pub total_qsos: usize,
    pub correct_callsigns: usize,
    pub correct_exchanges: usize,
    pub correct_qsos: usize, // Both callsign and exchange correct (may have used AGN)
    pub perfect_qsos: usize, // Both correct AND no AGN used
    pub total_points: u32,
    pub callsign_accuracy: f32,
    pub exchange_accuracy: f32,
    pub correct_rate: f32, // Percentage of correct QSOs
    pub perfect_rate: f32, // Percentage of perfect QSOs (no AGN)
    pub avg_station_wpm: f32,
    pub min_station_wpm: u8,
    pub max_station_wpm: u8,
    pub char_error_rates: Vec<(char, f32, usize)>, // (char, error_rate, total_count)
    pub agn_callsign_count: usize,                 // QSOs where AGN was used for callsign
    pub agn_exchange_count: usize,                 // QSOs where AGN was used for exchange
    pub agn_any_count: usize,                      // QSOs where any AGN was used
}

impl SessionStats {
    pub fn new() -> Self {
        Self { qsos: Vec::new() }
    }

    pub fn log_qso(&mut self, record: QsoRecord) {
        self.qsos.push(record);
    }

    pub fn clear(&mut self) {
        self.qsos.clear();
    }

    pub fn analyze(&self) -> StatsAnalysis {
        if self.qsos.is_empty() {
            return StatsAnalysis::default();
        }

        let total_qsos = self.qsos.len();
        let correct_callsigns = self.qsos.iter().filter(|q| q.callsign_correct).count();
        let correct_exchanges = self.qsos.iter().filter(|q| q.exchange_correct).count();

        // Correct QSOs: both callsign and exchange correct (may have used AGN)
        let correct_qsos = self
            .qsos
            .iter()
            .filter(|q| q.callsign_correct && q.exchange_correct)
            .count();

        // Perfect QSOs: both correct AND no AGN used at all
        let perfect_qsos = self
            .qsos
            .iter()
            .filter(|q| {
                q.callsign_correct
                    && q.exchange_correct
                    && !q.used_agn_callsign
                    && !q.used_agn_exchange
            })
            .count();

        let total_points: u32 = self.qsos.iter().map(|q| q.points).sum();

        let callsign_accuracy = (correct_callsigns as f32 / total_qsos as f32) * 100.0;
        let exchange_accuracy = (correct_exchanges as f32 / total_qsos as f32) * 100.0;
        let correct_rate = (correct_qsos as f32 / total_qsos as f32) * 100.0;
        let perfect_rate = (perfect_qsos as f32 / total_qsos as f32) * 100.0;

        // AGN usage stats
        let agn_callsign_count = self.qsos.iter().filter(|q| q.used_agn_callsign).count();
        let agn_exchange_count = self.qsos.iter().filter(|q| q.used_agn_exchange).count();
        let agn_any_count = self
            .qsos
            .iter()
            .filter(|q| q.used_agn_callsign || q.used_agn_exchange)
            .count();

        // WPM stats
        let wpms: Vec<u8> = self.qsos.iter().map(|q| q.station_wpm).collect();
        let avg_station_wpm = wpms.iter().map(|&w| w as f32).sum::<f32>() / wpms.len() as f32;
        let min_station_wpm = *wpms.iter().min().unwrap_or(&0);
        let max_station_wpm = *wpms.iter().max().unwrap_or(&0);

        // Character error analysis
        let char_error_rates = self.analyze_character_errors();

        StatsAnalysis {
            total_qsos,
            correct_callsigns,
            correct_exchanges,
            correct_qsos,
            perfect_qsos,
            total_points,
            callsign_accuracy,
            exchange_accuracy,
            correct_rate,
            perfect_rate,
            avg_station_wpm,
            min_station_wpm,
            max_station_wpm,
            char_error_rates,
            agn_callsign_count,
            agn_exchange_count,
            agn_any_count,
        }
    }

    fn analyze_character_errors(&self) -> Vec<(char, f32, usize)> {
        let mut char_totals: HashMap<char, usize> = HashMap::new();
        let mut char_errors: HashMap<char, usize> = HashMap::new();

        for qso in &self.qsos {
            // Always count totals for all characters encountered
            Self::count_chars(&qso.expected_callsign, &mut char_totals);
            Self::count_chars(&qso.expected_exchange, &mut char_totals);

            // Only count errors when there was an actual mistake
            if !qso.callsign_correct {
                Self::count_errors(
                    &qso.expected_callsign,
                    &qso.entered_callsign,
                    &mut char_errors,
                );
            }
            if !qso.exchange_correct {
                Self::count_errors(
                    &qso.expected_exchange,
                    &qso.entered_exchange,
                    &mut char_errors,
                );
            }
        }

        // Calculate error rates and sort by error rate descending
        let mut results: Vec<(char, f32, usize)> = char_totals
            .iter()
            .map(|(&ch, &total)| {
                let errors = *char_errors.get(&ch).unwrap_or(&0);
                let error_rate = if total > 0 {
                    (errors as f32 / total as f32) * 100.0
                } else {
                    0.0
                };
                (ch, error_rate, total)
            })
            .filter(|(_, _, total)| *total >= 3) // Only show chars with enough samples
            .collect();

        // Sort by error rate descending, then by character ascending for stable ordering
        results.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        results
    }

    /// Count occurrences of each alphanumeric character in a string
    fn count_chars(s: &str, totals: &mut HashMap<char, usize>) {
        for ch in s.to_uppercase().chars() {
            if ch.is_alphanumeric() {
                *totals.entry(ch).or_insert(0) += 1;
            }
        }
    }

    /// Count character errors by comparing expected vs entered strings
    fn count_errors(expected: &str, entered: &str, errors: &mut HashMap<char, usize>) {
        let expected_chars: Vec<char> = expected.to_uppercase().chars().collect();
        let entered_chars: Vec<char> = entered.to_uppercase().chars().collect();

        for (i, &exp_char) in expected_chars.iter().enumerate() {
            if !exp_char.is_alphanumeric() {
                continue;
            }

            let matches = entered_chars
                .get(i)
                .map(|&c| c == exp_char)
                .unwrap_or(false);

            if !matches {
                *errors.entry(exp_char).or_insert(0) += 1;
            }
        }
    }
}
