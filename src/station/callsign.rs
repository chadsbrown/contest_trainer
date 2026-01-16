use rand::seq::SliceRandom;
use std::collections::HashSet;
use std::path::Path;

use crate::contest::Exchange;

/// CWT station data (callsign + name + member number)
#[derive(Clone, Debug)]
pub struct CwtStation {
    pub callsign: String,
    pub name: String,
    pub number: String,
}

/// Pool of callsigns loaded from file
pub struct CallsignPool {
    callsigns: Vec<String>,
    used: HashSet<String>,
}

/// Pool of CWT stations with name/number data
pub struct CwtCallsignPool {
    stations: Vec<CwtStation>,
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
        let available: Vec<_> = self
            .callsigns
            .iter()
            .filter(|c| !self.used.contains(*c))
            .collect();

        if available.is_empty() {
            // Reset if all used
            self.used.clear();
            return self.callsigns.choose(&mut rand::thread_rng()).cloned();
        }

        let call = (*available.choose(&mut rand::thread_rng())?).clone();
        self.used.insert(call.clone());
        Some(call)
    }

    /// Reset used callsigns
    pub fn reset(&mut self) {
        self.used.clear();
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

    pub fn len(&self) -> usize {
        self.callsigns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.callsigns.is_empty()
    }
}

impl CwtCallsignPool {
    /// Load CWT stations from a file
    ///
    /// Format: CSV with fields: callsign, name, number (member # or state/country)
    /// Lines starting with # or ! are ignored
    /// Only lines with non-blank first three fields are accepted
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let stations: Vec<CwtStation> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
                if fields.len() >= 3 {
                    let callsign = fields[0].to_uppercase();
                    let name = fields[1].to_uppercase();
                    let number = fields[2].to_uppercase();

                    // All three fields must be non-blank
                    if !callsign.is_empty()
                        && !name.is_empty()
                        && !number.is_empty()
                        && CallsignPool::is_valid_callsign(&callsign)
                    {
                        return Some(CwtStation {
                            callsign,
                            name,
                            number,
                        });
                    }
                }
                None
            })
            .collect();

        if stations.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid CWT stations found in file",
            ));
        }

        Ok(Self {
            stations,
            used: HashSet::new(),
        })
    }

    /// Create a pool with default CWT stations
    pub fn default_pool() -> Self {
        let stations = vec![
            CwtStation {
                callsign: "W1AW".to_string(),
                name: "JOE".to_string(),
                number: "1".to_string(),
            },
            CwtStation {
                callsign: "K5ZD".to_string(),
                name: "RANDY".to_string(),
                number: "2".to_string(),
            },
            CwtStation {
                callsign: "N1MM".to_string(),
                name: "TOM".to_string(),
                number: "100".to_string(),
            },
            CwtStation {
                callsign: "K3LR".to_string(),
                name: "TIM".to_string(),
                number: "55".to_string(),
            },
            CwtStation {
                callsign: "W9RE".to_string(),
                name: "MIKE".to_string(),
                number: "IN".to_string(),
            },
        ];

        Self {
            stations,
            used: HashSet::new(),
        }
    }

    /// Get a random station (avoiding recently used ones)
    /// Returns (callsign, Exchange::Cwt)
    pub fn random(&mut self) -> Option<(String, Exchange)> {
        let available: Vec<_> = self
            .stations
            .iter()
            .filter(|s| !self.used.contains(&s.callsign))
            .collect();

        let station = if available.is_empty() {
            // Reset if all used
            self.used.clear();
            self.stations.choose(&mut rand::thread_rng())?
        } else {
            *available.choose(&mut rand::thread_rng())?
        };

        self.used.insert(station.callsign.clone());
        Some((
            station.callsign.clone(),
            Exchange::Cwt {
                name: station.name.clone(),
                number: station.number.clone(),
            },
        ))
    }

    /// Reset used stations
    pub fn reset(&mut self) {
        self.used.clear();
    }
}
