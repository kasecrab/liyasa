//! The one serde helper this package needs that `liyasa_core::serde_time` does
//! not have.
//!
//! `system_time_ms` and `duration_ms` cover the required fields; a drift record
//! also has three optional instants (`gone_since`, `resolved_at`, and a
//! review's `reviewed`), and `#[serde(with = "system_time_ms")]` does not
//! typecheck against an `Option`. Milliseconds since the epoch rather than
//! serde's own `SystemTime` shape, so a drift record's wire form matches
//! `Snapshot::taken_at` and `CheckResult::duration` instead of being the one
//! timestamp in the project that is an object (RFC 2066).

pub mod option_system_time_ms {
    use std::time::SystemTime;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<SystemTime>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(at) => {
                #[derive(Serialize)]
                struct Wrap<'a>(
                    #[serde(with = "liyasa_core::serde_time::system_time_ms")] &'a SystemTime,
                );
                s.serialize_some(&Wrap(at))
            }
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<SystemTime>, D::Error> {
        #[derive(Deserialize)]
        struct Wrap(#[serde(with = "liyasa_core::serde_time::system_time_ms")] SystemTime);
        Ok(Option::<Wrap>::deserialize(d)?.map(|Wrap(at)| at))
    }
}
