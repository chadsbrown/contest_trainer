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
    pub perfect_qsos: usize,
    pub total_points: u32,
    pub callsign_accuracy: f32,
    pub exchange_accuracy: f32,
    pub overall_accuracy: f32,
    pub avg_station_wpm: f32,
    pub min_station_wpm: u8,
    pub max_station_wpm: u8,
    pub char_error_rates: Vec<(char, f32, usize)>, // (char, error_rate, total_count)
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
        let perfect_qsos = self
            .qsos
            .iter()
            .filter(|q| q.callsign_correct && q.exchange_correct)
            .count();
        let total_points: u32 = self.qsos.iter().map(|q| q.points).sum();

        let callsign_accuracy = (correct_callsigns as f32 / total_qsos as f32) * 100.0;
        let exchange_accuracy = (correct_exchanges as f32 / total_qsos as f32) * 100.0;
        let overall_accuracy = (perfect_qsos as f32 / total_qsos as f32) * 100.0;

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
            perfect_qsos,
            total_points,
            callsign_accuracy,
            exchange_accuracy,
            overall_accuracy,
            avg_station_wpm,
            min_station_wpm,
            max_station_wpm,
            char_error_rates,
        }
    }

    fn analyze_character_errors(&self) -> Vec<(char, f32, usize)> {
        let mut char_totals: HashMap<char, usize> = HashMap::new();
        let mut char_errors: HashMap<char, usize> = HashMap::new();

        for qso in &self.qsos {
            // Analyze callsign characters
            Self::compare_strings(
                &qso.expected_callsign,
                &qso.entered_callsign,
                &mut char_totals,
                &mut char_errors,
            );

            // Analyze exchange characters
            Self::compare_strings(
                &qso.expected_exchange,
                &qso.entered_exchange,
                &mut char_totals,
                &mut char_errors,
            );
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

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    fn compare_strings(
        expected: &str,
        entered: &str,
        totals: &mut HashMap<char, usize>,
        errors: &mut HashMap<char, usize>,
    ) {
        let expected_chars: Vec<char> = expected.to_uppercase().chars().collect();
        let entered_chars: Vec<char> = entered.to_uppercase().chars().collect();

        // Simple character-by-character comparison
        // For each expected character, count it and check if it appears correctly
        for (i, &exp_char) in expected_chars.iter().enumerate() {
            if !exp_char.is_alphanumeric() {
                continue;
            }

            *totals.entry(exp_char).or_insert(0) += 1;

            // Check if this position matches
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
