use std::collections::HashMap;
use std::path::Path;

/// Parsed entry from cty.dat representing a DXCC entity
#[derive(Debug, Clone)]
pub struct DxccEntity {
    pub name: String,
    pub cq_zone: u8,
    pub itu_zone: u8,
    pub continent: String,
    pub primary_prefix: String,
}

/// A prefix or callsign entry with optional zone overrides
#[derive(Debug, Clone)]
struct PrefixEntry {
    cq_zone: u8,
    itu_zone: u8,
    is_exact: bool, // true if this is an exact callsign match (prefixed with =)
    country_prefix: String, // the primary prefix for the country this entry belongs to
}

/// CTY.DAT database for callsign lookups
pub struct CtyDat {
    /// Exact callsign matches (highest priority)
    exact_calls: HashMap<String, PrefixEntry>,
    /// Prefix matches, sorted by length descending for longest-match lookup
    prefixes: Vec<(String, PrefixEntry)>,
}

impl CtyDat {
    /// Load and parse a cty.dat file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse(&content))
    }

    /// Parse cty.dat content from a string
    pub fn parse(content: &str) -> Self {
        let mut exact_calls: HashMap<String, PrefixEntry> = HashMap::new();
        let mut prefixes: Vec<(String, PrefixEntry)> = Vec::new();

        let mut current_entity: Option<DxccEntity> = None;
        let mut alias_buffer = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Check if this is a header line (contains colons in specific positions)
            if Self::is_header_line(line) {
                // Process any pending aliases from previous entity
                if let Some(ref entity) = current_entity {
                    Self::parse_aliases(&alias_buffer, entity, &mut exact_calls, &mut prefixes);
                }
                alias_buffer.clear();

                // Parse the new header
                current_entity = Self::parse_header(line);
            } else {
                // This is an alias line, append to buffer
                alias_buffer.push_str(line);
                alias_buffer.push(' ');
            }
        }

        // Process final entity's aliases
        if let Some(ref entity) = current_entity {
            Self::parse_aliases(&alias_buffer, entity, &mut exact_calls, &mut prefixes);
        }

        // Sort prefixes by length descending for longest-match lookup
        prefixes.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

        Self {
            exact_calls,
            prefixes,
        }
    }

    /// Check if a line is a header line (entity definition)
    fn is_header_line(line: &str) -> bool {
        // Header lines have the format:
        // "Country Name:  CQ:  ITU:  Cont:  Lat:  Lon:  TZ:  Prefix:"
        // They contain multiple colons and end with a prefix (not a semicolon)
        let colon_count = line.matches(':').count();
        colon_count >= 7 && !line.ends_with(';') && !line.ends_with(',')
    }

    /// Parse a header line into a DxccEntity
    fn parse_header(line: &str) -> Option<DxccEntity> {
        // Split by colon and parse fields
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 8 {
            return None;
        }

        let name = parts[0].trim().to_string();
        let cq_zone = parts[1].trim().parse().unwrap_or(0);
        let itu_zone = parts[2].trim().parse().unwrap_or(0);
        let continent = parts[3].trim().to_string();
        // parts[4] = lat, parts[5] = lon, parts[6] = tz offset
        let primary_prefix = parts[7].trim().trim_start_matches('*').to_string();

        Some(DxccEntity {
            name,
            cq_zone,
            itu_zone,
            continent,
            primary_prefix,
        })
    }

    /// Parse alias entries and add them to the lookup tables
    fn parse_aliases(
        aliases: &str,
        entity: &DxccEntity,
        exact_calls: &mut HashMap<String, PrefixEntry>,
        prefixes: &mut Vec<(String, PrefixEntry)>,
    ) {
        // Remove trailing semicolon and split by comma
        let aliases = aliases.trim().trim_end_matches(';');

        for alias in aliases.split(',') {
            let alias = alias.trim();
            if alias.is_empty() {
                continue;
            }

            let (call_or_prefix, cq_override, itu_override, is_exact) = Self::parse_alias(alias);

            let entry = PrefixEntry {
                cq_zone: cq_override.unwrap_or(entity.cq_zone),
                itu_zone: itu_override.unwrap_or(entity.itu_zone),
                is_exact,
                country_prefix: entity.primary_prefix.to_uppercase(),
            };

            if is_exact {
                exact_calls.insert(call_or_prefix.to_uppercase(), entry);
            } else {
                prefixes.push((call_or_prefix.to_uppercase(), entry));
            }
        }
    }

    /// Parse a single alias entry, extracting zone overrides
    /// Returns (callsign_or_prefix, cq_zone_override, itu_zone_override, is_exact_match)
    fn parse_alias(alias: &str) -> (String, Option<u8>, Option<u8>, bool) {
        let mut s = alias;
        let mut cq_override = None;
        let mut itu_override = None;
        let mut is_exact = false;

        // Check for exact match prefix
        if s.starts_with('=') {
            is_exact = true;
            s = &s[1..];
        }

        // Extract the base callsign/prefix and any overrides
        let mut result = String::new();

        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '(' => {
                    // CQ zone override
                    let mut num = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc == ')' {
                            chars.next();
                            break;
                        }
                        num.push(nc);
                        chars.next();
                    }
                    cq_override = num.parse().ok();
                }
                '[' => {
                    // ITU zone override
                    let mut num = String::new();
                    while let Some(&nc) = chars.peek() {
                        if nc == ']' {
                            chars.next();
                            break;
                        }
                        num.push(nc);
                        chars.next();
                    }
                    itu_override = num.parse().ok();
                }
                '{' => {
                    // Continent override - skip it
                    while let Some(&nc) = chars.peek() {
                        if nc == '}' {
                            chars.next();
                            break;
                        }
                        chars.next();
                    }
                }
                '<' => {
                    // Lat/lon override - skip it
                    while let Some(&nc) = chars.peek() {
                        if nc == '>' {
                            chars.next();
                            break;
                        }
                        chars.next();
                    }
                }
                '~' => {
                    // Time zone override - skip it
                    while let Some(&nc) = chars.peek() {
                        if nc == '~' {
                            chars.next();
                            break;
                        }
                        chars.next();
                    }
                }
                _ => {
                    result.push(c);
                }
            }
        }

        (result, cq_override, itu_override, is_exact)
    }

    /// Look up CQ zone for a callsign
    pub fn lookup_cq_zone(&self, callsign: &str) -> Option<u8> {
        let call = callsign.to_uppercase();

        // First try exact match
        if let Some(entry) = self.exact_calls.get(&call) {
            return Some(entry.cq_zone);
        }

        // Then try longest prefix match
        for (prefix, entry) in &self.prefixes {
            if call.starts_with(prefix) {
                return Some(entry.cq_zone);
            }
        }

        None
    }

    /// Look up both CQ and ITU zones for a callsign
    pub fn lookup_zones(&self, callsign: &str) -> Option<(u8, u8)> {
        let call = callsign.to_uppercase();

        // First try exact match
        if let Some(entry) = self.exact_calls.get(&call) {
            return Some((entry.cq_zone, entry.itu_zone));
        }

        // Then try longest prefix match
        for (prefix, entry) in &self.prefixes {
            if call.starts_with(prefix) {
                return Some((entry.cq_zone, entry.itu_zone));
            }
        }

        None
    }

    /// Look up the matching prefix for a callsign (represents the DXCC entity/country)
    pub fn lookup_prefix(&self, callsign: &str) -> Option<String> {
        let call = callsign.to_uppercase();

        // First try exact match - return the country prefix, not the callsign
        if let Some(entry) = self.exact_calls.get(&call) {
            return Some(entry.country_prefix.clone());
        }

        // Then try longest prefix match
        for (prefix, entry) in &self.prefixes {
            if call.starts_with(prefix) {
                return Some(entry.country_prefix.clone());
            }
        }

        None
    }

    /// Check if two callsigns are from the same country (matching prefix)
    pub fn same_country(&self, call1: &str, call2: &str) -> bool {
        match (self.lookup_prefix(call1), self.lookup_prefix(call2)) {
            (Some(p1), Some(p2)) => p1 == p2,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header() {
        let line = "United States:            05:  08:  NA:   37.60:    91.87:     5.0:  K:";
        let entity = CtyDat::parse_header(line).unwrap();
        assert_eq!(entity.name, "United States");
        assert_eq!(entity.cq_zone, 5);
        assert_eq!(entity.itu_zone, 8);
        assert_eq!(entity.continent, "NA");
        assert_eq!(entity.primary_prefix, "K");
    }

    #[test]
    fn test_parse_alias_simple() {
        let (prefix, cq, itu, exact) = CtyDat::parse_alias("W1");
        assert_eq!(prefix, "W1");
        assert_eq!(cq, None);
        assert_eq!(itu, None);
        assert!(!exact);
    }

    #[test]
    fn test_parse_alias_with_cq_override() {
        let (prefix, cq, itu, exact) = CtyDat::parse_alias("K0(4)[7]");
        assert_eq!(prefix, "K0");
        assert_eq!(cq, Some(4));
        assert_eq!(itu, Some(7));
        assert!(!exact);
    }

    #[test]
    fn test_parse_alias_exact() {
        let (prefix, cq, itu, exact) = CtyDat::parse_alias("=W1AW");
        assert_eq!(prefix, "W1AW");
        assert_eq!(cq, None);
        assert_eq!(itu, None);
        assert!(exact);
    }

    #[test]
    fn test_parse_alias_exact_with_override() {
        let (prefix, cq, itu, exact) = CtyDat::parse_alias("=AL7BX(4)[7]");
        assert_eq!(prefix, "AL7BX");
        assert_eq!(cq, Some(4));
        assert_eq!(itu, Some(7));
        assert!(exact);
    }

    #[test]
    fn test_lookup() {
        let content = r#"
United States:            05:  08:  NA:   37.60:    91.87:     5.0:  K:
    K,W,N,AA,
    K0(4)[7],W0(4)[7],N0(4)[7],
    K6(3)[6],W6(3)[6],N6(3)[6],
    =W1AW(5)[8];
Germany:                  14:  28:  EU:   51.00:   -10.00:    -1.0:  DL:
    DA,DB,DC,DD,DE,DF,DG,DH,DI,DJ,DK,DL,DM,DN,DO,DP,DQ,DR;
"#;
        let cty = CtyDat::parse(content);

        // Test basic prefix lookup
        assert_eq!(cty.lookup_cq_zone("K1ABC"), Some(5));
        assert_eq!(cty.lookup_cq_zone("W2XYZ"), Some(5));

        // Test prefix with zone override
        assert_eq!(cty.lookup_cq_zone("K0ABC"), Some(4));
        assert_eq!(cty.lookup_cq_zone("W6ABC"), Some(3));

        // Test exact callsign match
        assert_eq!(cty.lookup_cq_zone("W1AW"), Some(5));

        // Test German callsign
        assert_eq!(cty.lookup_cq_zone("DL1ABC"), Some(14));
    }

    #[test]
    fn test_real_cty_file() {
        // Test with the actual embedded cty.dat
        let cty_data = include_str!("../data/cty.dat");
        let cty = CtyDat::parse(cty_data);

        // US callsigns should have different zones based on call district
        // Zone 5 = Eastern US (1, 2, 3, 4, 8)
        assert_eq!(cty.lookup_cq_zone("W1AW"), Some(5));
        assert_eq!(cty.lookup_cq_zone("K2ABC"), Some(5));
        assert_eq!(cty.lookup_cq_zone("N3XYZ"), Some(5));

        // Zone 4 = Central US (0, 9)
        assert_eq!(cty.lookup_cq_zone("W0ABC"), Some(4));
        assert_eq!(cty.lookup_cq_zone("K9XYZ"), Some(4));

        // Zone 3 = Western US (6, 7)
        assert_eq!(cty.lookup_cq_zone("W6ABC"), Some(3));
        assert_eq!(cty.lookup_cq_zone("K7XYZ"), Some(3));

        // International callsigns
        assert_eq!(cty.lookup_cq_zone("DL1ABC"), Some(14)); // Germany
        assert_eq!(cty.lookup_cq_zone("JA1ABC"), Some(25)); // Japan
        assert_eq!(cty.lookup_cq_zone("VK2ABC"), Some(30)); // Australia
    }

    #[test]
    fn test_same_country() {
        let content = r#"
United States:            05:  08:  NA:   37.60:    91.87:     5.0:  K:
    K,W,N,AA,
    K0(4)[7],W0(4)[7],N0(4)[7],
    K6(3)[6],W6(3)[6],N6(3)[6],
    =W1AW(5)[8];
Germany:                  14:  28:  EU:   51.00:   -10.00:    -1.0:  DL:
    DA,DB,DC,DD,DE,DF,DG,DH,DI,DJ,DK,DL,DM,DN,DO,DP,DQ,DR;
"#;
        let cty = CtyDat::parse(content);

        // Two regular US callsigns should be same country
        assert!(cty.same_country("K1ABC", "W2XYZ"));

        // Exact match callsign (W1AW) should match regular US callsigns
        assert!(cty.same_country("W1AW", "K1ABC"));
        assert!(cty.same_country("K2XYZ", "W1AW"));

        // US and German callsigns should not be same country
        assert!(!cty.same_country("K1ABC", "DL1ABC"));
        assert!(!cty.same_country("W1AW", "DL1ABC"));

        // Two German callsigns should be same country
        assert!(cty.same_country("DL1ABC", "DK2XYZ"));
    }
}
