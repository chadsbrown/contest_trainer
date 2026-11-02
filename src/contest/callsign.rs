use rand::seq::IndexedRandom;
use rand::Rng;
use std::collections::HashSet;
use std::path::Path;

use super::types::{CallsignSource, Contest, Exchange};

/// Pool of callsigns loaded from file
pub struct CallsignPool {
    callsigns: Vec<String>,
    used: HashSet<String>,
}

impl CallsignPool {
    /// Load callsigns from a file
    ///
    /// Supported formats:
    /// - One callsign per line
    /// - Lines starting with # are comments
    /// - Empty lines are ignored
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let callsigns: Vec<String> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                // Handle CSV format - take first field
                line.split(',').next().unwrap_or(line).trim().to_uppercase()
            })
            .filter(|call| Self::is_valid_callsign(call))
            .collect();

        if callsigns.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid callsigns found in file",
            ));
        }

        Ok(Self {
            callsigns,
            used: HashSet::new(),
        })
    }

    /// Create a pool with default callsigns (for when no file is available)
    pub fn default_pool() -> Self {
        let callsigns = vec![
            "W1AW", "K1TTT", "N1MM", "W2FU", "K2LE", "N2IC", "W3LPL", "K3LR", "N3RS", "W4MYA",
            "K4JA", "N4AF", "W5WMU", "K5ZD", "N5TJ", "W6YX", "K6XX", "N6TV", "W7RN", "K7RL",
            "N7DR", "W8ND", "K8ND", "N8II", "W9RE", "K9CT", "N9RV", "W0AIH", "K0RF", "N0AX",
            "VE3EJ", "VE7CC", "VA3DX", "VE2IM", "VE6SV", "DL1A", "DL6FBL", "G3PXT", "G4AMJ",
            "JA1ABC", "JH1NBN", "PY2SEX", "LU1FAM", "ZS6EZ", "VK2GR", "ZL1BQD",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        Self {
            callsigns,
            used: HashSet::new(),
        }
    }

    /// Get a random callsign (avoiding recently used ones)
    pub fn random(&mut self) -> Option<String> {
        // Fast path: while most of the pool is unused, pick random indices until
        // we hit an unused callsign (avoids scanning the whole list each call).
        if !self.callsigns.is_empty() && self.used.len() < self.callsigns.len() / 2 {
            let mut rng = rand::rng();
            for _ in 0..16 {
                let call = &self.callsigns[rng.random_range(0..self.callsigns.len())];
                if !self.used.contains(call) {
                    let call = call.clone();
                    self.used.insert(call.clone());
                    return Some(call);
                }
            }
        }

        let available: Vec<_> = self
            .callsigns
            .iter()
            .filter(|c| !self.used.contains(*c))
            .collect();

        if available.is_empty() {
            // Reset if all used
            self.used.clear();
            return self.callsigns.choose(&mut rand::rng()).cloned();
        }

        let call = (*available.choose(&mut rand::rng())?).clone();
        self.used.insert(call.clone());
        Some(call)
    }

    /// Basic callsign validation
    fn is_valid_callsign(call: &str) -> bool {
        if call.len() < 3 || call.len() > 10 {
            return false;
        }
        // Must contain at least one letter and one number
        call.chars().any(|c| c.is_ascii_alphabetic())
            && call.chars().any(|c| c.is_ascii_digit())
            && call.chars().all(|c| c.is_ascii_alphanumeric() || c == '/')
    }
}

/// Generic callsign source using a file-backed callsign pool
pub struct FileCallsignSource {
    pool: CallsignPool,
}

impl FileCallsignSource {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        Ok(Self {
            pool: CallsignPool::load(path)?,
        })
    }

    pub fn default_pool() -> Self {
        Self {
            pool: CallsignPool::default_pool(),
        }
    }
}

impl CallsignSource for FileCallsignSource {
    fn random(
        &mut self,
        contest: &dyn Contest,
        serial: u32,
        settings: &toml::Value,
    ) -> Option<(String, Exchange)> {
        let callsign = self.pool.random()?;
        let exchange = contest.generate_exchange(&callsign, serial, settings);
        Some((callsign, exchange))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_does_not_repeat_until_exhausted() {
        let mut pool = CallsignPool::default_pool();
        let n = pool.callsigns.len();
        let drawn: HashSet<String> = (0..n).map(|_| pool.random().unwrap()).collect();
        assert_eq!(
            drawn.len(),
            n,
            "every callsign drawn exactly once before reset"
        );
        // Pool resets and keeps producing callsigns once exhausted
        assert!(pool.random().is_some());
    }
}
