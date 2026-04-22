// IMPLEMENTS: D-447
//! 64-bit Hybrid Logical Clock (48-bit ms + 16-bit counter). One
//! HLC stamp per event lets us reconstruct a partial order across
//! sources of equal physical timestamp without giving up wall-time
//! grounding.
//!
//! Layout (LE):
//!  - bits  0..16: counter
//!  - bits 16..64: physical ms

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct HlcTimestamp(pub u64);

const COUNTER_BITS: u32 = 16;
const COUNTER_MASK: u64 = (1u64 << COUNTER_BITS) - 1;
const MAX_PHYSICAL_MS: u64 = (1u64 << 48) - 1;

impl HlcTimestamp {
    #[must_use]
    pub fn pack(physical_ms: u64, counter: u16) -> Self {
        let physical = physical_ms.min(MAX_PHYSICAL_MS);
        Self((physical << COUNTER_BITS) | u64::from(counter))
    }

    #[must_use]
    pub fn physical_ms(self) -> u64 {
        self.0 >> COUNTER_BITS
    }

    #[must_use]
    pub fn counter(self) -> u16 {
        let raw = self.0 & COUNTER_MASK;
        u16::try_from(raw).unwrap_or(u16::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hlc {
    pub last: HlcTimestamp,
}

impl Default for Hlc {
    fn default() -> Self {
        Self {
            last: HlcTimestamp::pack(0, 0),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HlcError {
    #[error("HLC counter overflow at physical {physical_ms} ms")]
    CounterOverflow { physical_ms: u64 },
}

/// Advance the local HLC by observing the current wall-clock ms.
/// Returns the new stamp.
pub fn tick_with(hlc: &mut Hlc, physical_now_ms: u64) -> Result<HlcTimestamp, HlcError> {
    let last_phys = hlc.last.physical_ms();
    let next = if physical_now_ms > last_phys {
        HlcTimestamp::pack(physical_now_ms, 0)
    } else {
        let new_counter = hlc
            .last
            .counter()
            .checked_add(1)
            .ok_or(HlcError::CounterOverflow {
                physical_ms: last_phys,
            })?;
        HlcTimestamp::pack(last_phys, new_counter)
    };
    hlc.last = next;
    Ok(next)
}

#[must_use]
pub fn max_hlc(a: HlcTimestamp, b: HlcTimestamp) -> HlcTimestamp {
    if a.0 >= b.0 { a } else { b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_and_unpack_roundtrip() {
        let t = HlcTimestamp::pack(1_700_000_000_000, 7);
        assert_eq!(t.physical_ms(), 1_700_000_000_000);
        assert_eq!(t.counter(), 7);
    }

    #[test]
    fn forward_clock_resets_counter() {
        let mut h = Hlc::default();
        let _ = tick_with(&mut h, 100).unwrap();
        let t = tick_with(&mut h, 200).unwrap();
        assert_eq!(t.physical_ms(), 200);
        assert_eq!(t.counter(), 0);
    }

    #[test]
    fn same_physical_increments_counter() {
        let mut h = Hlc::default();
        let _ = tick_with(&mut h, 100).unwrap();
        let t = tick_with(&mut h, 100).unwrap();
        assert_eq!(t.physical_ms(), 100);
        assert_eq!(t.counter(), 1);
    }

    #[test]
    fn ordering_is_lexicographic_on_packed_u64() {
        let a = HlcTimestamp::pack(100, 5);
        let b = HlcTimestamp::pack(100, 6);
        let c = HlcTimestamp::pack(101, 0);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(max_hlc(a, c), c);
    }
}
