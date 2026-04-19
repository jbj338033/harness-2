use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    #[must_use]
    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    #[must_use]
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// # Errors
    /// Returns [`time::error::ComponentRange`] if the millisecond value is
    /// outside the range representable by [`OffsetDateTime`].
    pub fn to_datetime(self) -> Result<OffsetDateTime, time::error::ComponentRange> {
        let nanos = i128::from(self.0) * 1_000_000;
        OffsetDateTime::from_unix_timestamp_nanos(nanos)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_datetime() {
            Ok(dt) => write!(f, "{dt}"),
            Err(_) => write!(f, "<invalid timestamp {}>", self.0),
        }
    }
}

#[must_use]
pub fn now() -> Timestamp {
    let dt = OffsetDateTime::now_utc();
    let nanos = dt.unix_timestamp_nanos();
    let ms = i64::try_from(nanos / 1_000_000).unwrap_or(i64::MAX);
    Timestamp(ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_recent() {
        let t = now();
        assert!(t.as_millis() > 1_577_836_800_000);
    }

    #[test]
    fn roundtrip_through_datetime() {
        let t = now();
        let dt = t.to_datetime().unwrap();
        let nanos = dt.unix_timestamp_nanos();
        let ms = i64::try_from(nanos / 1_000_000).unwrap();
        assert_eq!(ms, t.as_millis());
    }

    #[test]
    fn ordering_works() {
        let a = Timestamp::from_millis(100);
        let b = Timestamp::from_millis(200);
        assert!(a < b);
    }

    #[test]
    fn serde_roundtrip() {
        let t = Timestamp::from_millis(1_700_000_000_000);
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "1700000000000");
        let back: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }
}
