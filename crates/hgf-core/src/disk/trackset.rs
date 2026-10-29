// SPDX-License-Identifier: GPL-3.0-or-later

//! Greaseweazle track specifications (`TSPEC`), e.g. `c=0-79:h=0-1:step=2`.
//!
//! Only the parts needed by the GUI are modelled: the list of cylinders and heads
//! (to size the disk visualizer) and a syntax check for user-typed specs.

use std::fmt;

/// Cylinders and heads covered by an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackSet {
    pub cyls: Vec<u16>,
    pub heads: Vec<u8>,
}

impl TrackSet {
    /// Build a dense set `c=0-(cyls-1):h=0-(heads-1)`.
    pub fn dense(cyls: u16, heads: u8) -> Self {
        Self { cyls: (0..cyls).collect(), heads: (0..heads).collect() }
    }

    /// Parse the textual form printed by gw (`Reading c=0-79:h=0-1 revs=3`).
    ///
    /// Elements other than `c=` and `h=` (step, hswap, offsets) are accepted and ignored.
    pub fn parse(spec: &str) -> Result<Self, String> {
        validate_tspec(spec)?;
        let mut cyls = None;
        let mut heads = None;
        for part in spec.split(':') {
            if let Some(set) = part.strip_prefix("c=") {
                cyls = Some(parse_set(set)?);
            } else if let Some(set) = part.strip_prefix("h=") {
                let hs = parse_set(set)?;
                heads = Some(hs.into_iter().map(|h| h as u8).collect());
            }
        }
        Ok(Self {
            cyls: cyls.unwrap_or_else(|| (0..80).collect()),
            heads: heads.unwrap_or_else(|| vec![0, 1]),
        })
    }

    pub fn len(&self) -> usize {
        self.cyls.len() * self.heads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Display for TrackSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c: Vec<u32> = self.cyls.iter().map(|&c| c as u32).collect();
        let h: Vec<u32> = self.heads.iter().map(|&h| h as u32).collect();
        write!(f, "c={}:h={}", range_str(&c), range_str(&h))
    }
}

/// Parse a Greaseweazle SET: comma separated integers and ranges (`0-7,9-12`, `0-78/2`).
pub fn parse_set(set: &str) -> Result<Vec<u16>, String> {
    let mut out = Vec::new();
    for item in set.split(',') {
        let item = item.trim();
        if item.is_empty() {
            return Err(format!("empty element in set '{set}'"));
        }
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (r, parse_num(s, set)?.max(1)),
            None => (item, 1),
        };
        let (start, end) = match range.split_once('-') {
            Some((a, b)) => (parse_num(a, set)?, parse_num(b, set)?),
            None => {
                let v = parse_num(range, set)?;
                (v, v)
            }
        };
        if end < start {
            return Err(format!("reversed range '{item}'"));
        }
        out.extend((start..=end).step_by(step as usize));
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn parse_num(s: &str, set: &str) -> Result<u16, String> {
    s.trim().parse::<u16>().map_err(|_| format!("'{s}' is not a number in set '{set}'"))
}

/// Greaseweazle SET notation of a sorted cylinder list: `0,3,5-9,40`.
pub fn set_notation(cyls: &[u16]) -> String {
    range_str(&cyls.iter().map(|&c| c as u32).collect::<Vec<_>>())
}

fn range_str(v: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let start = v[i];
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        if j == i {
            parts.push(start.to_string());
        } else {
            parts.push(format!("{}-{}", start, v[j]));
        }
        i = j + 1;
    }
    parts.join(",")
}

/// Syntax check of a user-provided TSPEC, mirroring `gw`'s own grammar:
/// `c=SET`, `h=SET`, `step=N` (or `step=1/N`), `hswap`, `h0.off=±N`, `h1.off=±N`.
pub fn validate_tspec(spec: &str) -> Result<(), String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("empty track specification".into());
    }
    for part in spec.split(':') {
        let part = part.trim();
        if part == "hswap" {
            continue;
        }
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!("unknown element '{part}'"));
        };
        match key {
            "c" | "h" => {
                let v = parse_set(value)?;
                if key == "h" && v.iter().any(|&h| h > 1) {
                    return Err("heads must be 0 or 1".into());
                }
            }
            "step" => {
                let n = value.strip_prefix("1/").unwrap_or(value);
                match n.parse::<u8>() {
                    Ok(1..=9) => {}
                    _ => return Err(format!("invalid step '{value}' (expected 1-9 or 1/N)")),
                }
            }
            "h0.off" | "h1.off" => {
                let n = value.trim_start_matches(['+', '-']);
                if n.parse::<u8>().is_err() {
                    return Err(format!("invalid offset '{value}'"));
                }
            }
            _ => return Err(format!("unknown element '{part}'")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gw_printed_sets() {
        let t = TrackSet::parse("c=0-79:h=0-1").unwrap();
        assert_eq!(t.cyls.len(), 80);
        assert_eq!(t.heads, vec![0, 1]);
        let t = TrackSet::parse("c=0-7,9-12:h=1:step=2:hswap").unwrap();
        assert_eq!(t.cyls, vec![0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12]);
        assert_eq!(t.heads, vec![1]);
        assert_eq!(t.to_string(), "c=0-7,9-12:h=1");
    }

    #[test]
    fn validates_user_specs() {
        assert!(validate_tspec("c=0-39:h=0:step=2").is_ok());
        assert!(validate_tspec("step=2").is_ok());
        assert!(validate_tspec("h0.off=-1:hswap").is_ok());
        assert!(validate_tspec("c=0-39:h=2").is_err());
        assert!(validate_tspec("cyl=3").is_err());
        assert!(validate_tspec("c=9-3").is_err());
        assert!(validate_tspec("step=0").is_err());
    }
}
