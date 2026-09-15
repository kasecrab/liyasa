//! Durations and instants in JSON.
//!
//! Both encode as whole milliseconds: no date crate is in the dependency table
//! (§6.2.1), and the build clock (§6.6.2) is the only time source that reaches
//! output, so a calendar type would buy nothing.

pub mod duration_ms {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(value.as_millis().min(u128::from(u64::MAX)) as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        u64::deserialize(d).map(Duration::from_millis)
    }
}

pub mod system_time_ms {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let millis = value
            .duration_since(UNIX_EPOCH)
            .map_err(|_| serde::ser::Error::custom("time is before the unix epoch"))?
            .as_millis();
        s.serialize_u64(millis.min(u128::from(u64::MAX)) as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let millis = u64::deserialize(d)?;
        UNIX_EPOCH
            .checked_add(Duration::from_millis(millis))
            .ok_or_else(|| serde::de::Error::custom("timestamp out of range"))
    }
}
