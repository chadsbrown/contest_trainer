/// A single Morse code element
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MorseElement {
    Dit,        // 1 unit tone
    Dah,        // 3 units tone
    ElementGap, // 1 unit silence (between dit/dah in same character)
    CharGap,    // 3 units silence (between characters)
    WordGap,    // 7 units silence (between words)
}

impl MorseElement {
    /// Returns the duration in units (1 unit = dit length)
    pub fn units(&self) -> u32 {
        match self {
            MorseElement::Dit => 1,
            MorseElement::Dah => 3,
            MorseElement::ElementGap => 1,
            MorseElement::CharGap => 3,
            MorseElement::WordGap => 7,
        }
    }

    /// Returns true if this element produces a tone
    pub fn is_tone(&self) -> bool {
        matches!(self, MorseElement::Dit | MorseElement::Dah)
    }
}

/// Calculates Morse timing based on WPM
pub struct MorseTimer {
    sample_rate: u32,
    samples_per_unit: usize,
}

impl MorseTimer {
    pub fn new(sample_rate: u32, wpm: u8) -> Self {
        // PARIS = 50 units, so at N WPM we send N*50 units per minute
        // units_per_second = (wpm * 50) / 60
        // samples_per_unit = sample_rate / units_per_second
        let units_per_second = (wpm as f64 * 50.0) / 60.0;
        let samples_per_unit = (sample_rate as f64 / units_per_second) as usize;

        Self {
            sample_rate,
            samples_per_unit,
        }
    }

    /// Get samples for a given element
    pub fn element_samples(&self, element: MorseElement) -> usize {
        self.samples_per_unit * element.units() as usize
    }

    /// Get samples per unit (dit length)
    pub fn samples_per_unit(&self) -> usize {
        self.samples_per_unit
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Generates sine wave tones with envelope shaping
pub struct ToneGenerator {
    frequency_hz: f32,
    sample_rate: f32,
    phase: f64,
    // Envelope for click-free keying (in samples)
    ramp_samples: usize,
}

impl ToneGenerator {
    pub fn new(frequency_hz: f32, sample_rate: u32) -> Self {
        // Ramp time ~5ms to avoid clicks
        let ramp_samples = (sample_rate as f32 * 0.005) as usize;

        Self {
            frequency_hz,
            sample_rate: sample_rate as f32,
            phase: 0.0,
            ramp_samples,
        }
    }

    /// Set the frequency (for frequency offset support)
    pub fn set_frequency(&mut self, frequency_hz: f32) {
        self.frequency_hz = frequency_hz;
    }

    /// Generate a sample at the current phase
    pub fn next_sample(&mut self) -> f32 {
        let sample = (self.phase * 2.0 * std::f64::consts::PI).sin() as f32;
        self.phase += self.frequency_hz as f64 / self.sample_rate as f64;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        sample
    }

    /// Apply raised cosine envelope to avoid clicks
    pub fn envelope(&self, position: usize, total: usize) -> f32 {
        if position < self.ramp_samples {
            // Attack: raised cosine ramp up
            0.5 * (1.0 - (std::f32::consts::PI * position as f32 / self.ramp_samples as f32).cos())
        } else if position >= total.saturating_sub(self.ramp_samples) {
            // Release: raised cosine ramp down
            let release_pos = position - (total - self.ramp_samples);
            0.5 * (1.0
                + (std::f32::consts::PI * release_pos as f32 / self.ramp_samples as f32).cos())
        } else {
            1.0
        }
    }

    /// Reset phase (for starting fresh)
    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
    }
}

/// Convert a character to Morse elements
pub fn char_to_morse(ch: char) -> Option<Vec<MorseElement>> {
    use MorseElement::{Dah, Dit};

    let code = match ch.to_ascii_uppercase() {
        'A' => vec![Dit, Dah],
        'B' => vec![Dah, Dit, Dit, Dit],
        'C' => vec![Dah, Dit, Dah, Dit],
        'D' => vec![Dah, Dit, Dit],
        'E' => vec![Dit],
        'F' => vec![Dit, Dit, Dah, Dit],
        'G' => vec![Dah, Dah, Dit],
        'H' => vec![Dit, Dit, Dit, Dit],
        'I' => vec![Dit, Dit],
        'J' => vec![Dit, Dah, Dah, Dah],
        'K' => vec![Dah, Dit, Dah],
        'L' => vec![Dit, Dah, Dit, Dit],
        'M' => vec![Dah, Dah],
        'N' => vec![Dah, Dit],
        'O' => vec![Dah, Dah, Dah],
        'P' => vec![Dit, Dah, Dah, Dit],
        'Q' => vec![Dah, Dah, Dit, Dah],
        'R' => vec![Dit, Dah, Dit],
        'S' => vec![Dit, Dit, Dit],
        'T' => vec![Dah],
        'U' => vec![Dit, Dit, Dah],
        'V' => vec![Dit, Dit, Dit, Dah],
        'W' => vec![Dit, Dah, Dah],
        'X' => vec![Dah, Dit, Dit, Dah],
        'Y' => vec![Dah, Dit, Dah, Dah],
        'Z' => vec![Dah, Dah, Dit, Dit],
        '0' => vec![Dah, Dah, Dah, Dah, Dah],
        '1' => vec![Dit, Dah, Dah, Dah, Dah],
        '2' => vec![Dit, Dit, Dah, Dah, Dah],
        '3' => vec![Dit, Dit, Dit, Dah, Dah],
        '4' => vec![Dit, Dit, Dit, Dit, Dah],
        '5' => vec![Dit, Dit, Dit, Dit, Dit],
        '6' => vec![Dah, Dit, Dit, Dit, Dit],
        '7' => vec![Dah, Dah, Dit, Dit, Dit],
        '8' => vec![Dah, Dah, Dah, Dit, Dit],
        '9' => vec![Dah, Dah, Dah, Dah, Dit],
        '/' => vec![Dah, Dit, Dit, Dah, Dit],
        '?' => vec![Dit, Dit, Dah, Dah, Dit, Dit],
        '.' => vec![Dit, Dah, Dit, Dah, Dit, Dah],
        ',' => vec![Dah, Dah, Dit, Dit, Dah, Dah],
        '=' => vec![Dah, Dit, Dit, Dit, Dah], // BT
        _ => return None,
    };

    Some(code)
}

/// Convert text to a sequence of Morse elements
pub fn text_to_morse(text: &str) -> Vec<MorseElement> {
    let mut elements = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    for (word_idx, word) in words.iter().enumerate() {
        for (char_idx, ch) in word.chars().enumerate() {
            if let Some(code) = char_to_morse(ch) {
                for (elem_idx, &elem) in code.iter().enumerate() {
                    elements.push(elem);
                    // Add element gap after each dit/dah except the last in character
                    if elem_idx < code.len() - 1 {
                        elements.push(MorseElement::ElementGap);
                    }
                }
            }
            // Add character gap after each character except the last in word
            if char_idx < word.chars().count() - 1 {
                elements.push(MorseElement::CharGap);
            }
        }
        // Add word gap after each word except the last
        if word_idx < words.len() - 1 {
            elements.push(MorseElement::WordGap);
        }
    }

    elements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_to_morse() {
        use MorseElement::{Dah, Dit};
        assert_eq!(char_to_morse('A'), Some(vec![Dit, Dah]));
        assert_eq!(char_to_morse('S'), Some(vec![Dit, Dit, Dit]));
        assert_eq!(char_to_morse('O'), Some(vec![Dah, Dah, Dah]));
    }

    #[test]
    fn test_text_to_morse() {
        let elements = text_to_morse("SOS");
        // S = ...  O = ---  S = ...
        // With gaps: . _ . _ . CharGap - _ - _ - CharGap . _ . _ .
        assert!(!elements.is_empty());
    }

    #[test]
    fn test_morse_timer() {
        let timer = MorseTimer::new(44100, 20);
        // At 20 WPM, 1 unit = 60ms = 2646 samples at 44100Hz
        assert!(timer.samples_per_unit() > 2000);
        assert!(timer.samples_per_unit() < 3000);
    }
}
