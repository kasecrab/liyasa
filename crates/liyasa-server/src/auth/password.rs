//! The shared site password (AUTH-02, AUTH-09, NFR-12).
//!
//! One password per site or per environment, hashed with Argon2id and stored
//! as a PHC string. The plaintext is never kept: rotation replaces the hash,
//! and every session opened against the old one is invalidated, because
//! "rotated" that leaves the old password's sessions alive has not rotated
//! anything a reader would notice.
//!
//! Verification is rate limited per address and per site. Per address alone is
//! not enough — a distributed guess spreads across addresses and each one stays
//! under the limit — and per site alone is not enough either, because then one
//! noisy address locks every reader out.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

use argon2::{
    Algorithm, Argon2, Params, PasswordHasher as _, PasswordVerifier as _, Version,
    password_hash::phc::PasswordHash,
};
use liyasa_core::diagnostics::{Diagnostic, code};

use crate::auth::clock::{Clock, millis};
use crate::auth::config::{ARGON2_SALT_BYTES, ARGON2_TAG_BYTES, Argon2Params};
use crate::auth::random;

/// AUTH-09: three per minute per address, and a per-site ceiling well above
/// what a site full of readers signing in at once produces.
pub const PER_ADDRESS_PER_MINUTE: u32 = 5;
pub const PER_SITE_PER_MINUTE: u32 = 60;
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Correct,
    Wrong,
    /// The caller is over a limit. The reader is told to wait; nothing about
    /// the password is revealed either way.
    RateLimited,
}

/// Hashes a password with Argon2id at the configured parameters.
pub fn hash(password: &str, params: Argon2Params) -> Result<String, Diagnostic> {
    let salt = random::bytes::<ARGON2_SALT_BYTES>()
        .map_err(|_| Diagnostic::new(code::E0803, "no randomness for a password salt"))?;
    let hasher = argon2(params)?;
    let hash = hasher
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map_err(|error| {
            Diagnostic::new(
                code::E0803,
                format!("the password could not be hashed: {error}"),
            )
        })?;
    Ok(hash.to_string())
}

/// Verifies against a PHC string. The parameters come out of the stored hash,
/// not out of the configuration: a hash written under the old parameters must
/// keep verifying after they are raised.
pub fn verify(password: &str, stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn argon2<'a>(params: Argon2Params) -> Result<Argon2<'a>, Diagnostic> {
    let params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(ARGON2_TAG_BYTES),
    )
    .map_err(|error| {
        Diagnostic::new(
            code::E0803,
            format!("`auth.password.argon2` is not a usable parameter set: {error}"),
        )
    })?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Whether a stored hash still meets the configured parameters. A site that
/// raised them re-hashes on the next correct sign-in rather than locking
/// everyone out.
pub fn needs_rehash(stored: &str, want: Argon2Params) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return true;
    };
    if parsed.algorithm.as_str() != "argon2id" {
        return true;
    }
    let read = |name: &str| {
        parsed
            .params
            .iter()
            .find(|(key, _)| key.as_str() == name)
            .and_then(|(_, value)| value.decimal().ok())
    };
    read("m").is_none_or(|m| m < want.memory_kib)
        || read("t").is_none_or(|t| t < want.iterations)
        || read("p").is_none_or(|p| p < want.parallelism)
}

/// One site's password, its rotation, and the two rate-limit windows.
#[derive(Debug)]
pub struct Passwords {
    /// Environment to PHC string. AUTH-02: per site or per environment.
    hashes: RwLock<BTreeMap<String, String>>,
    attempts: RwLock<Attempts>,
    params: Argon2Params,
    clock: Clock,
}

#[derive(Debug, Default)]
struct Attempts {
    /// Subject to (window start, count).
    per_address: BTreeMap<String, (i64, u32)>,
    per_site: BTreeMap<String, (i64, u32)>,
}

/// What a rotation invalidated, so the caller can drop those sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotated {
    pub env: String,
    pub had_previous: bool,
}

impl Passwords {
    pub fn new(params: Argon2Params, clock: Clock) -> Self {
        Self {
            hashes: RwLock::new(BTreeMap::new()),
            attempts: RwLock::new(Attempts::default()),
            params,
            clock,
        }
    }

    /// Sets or rotates the password for an environment. AUTH-02: from the
    /// dashboard or the CLI, which are two callers of this one function.
    pub fn rotate(&self, env: &str, password: &str) -> Result<Rotated, Diagnostic> {
        let hash = hash(password, self.params)?;
        let previous = self
            .hashes
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(env.to_owned(), hash);
        Ok(Rotated {
            env: env.to_owned(),
            had_previous: previous.is_some(),
        })
    }

    /// Installs a hash that was computed elsewhere — the secret store, where
    /// AUTH-02 says the password hash actually lives.
    pub fn install(&self, env: &str, phc: &str) -> Result<(), Diagnostic> {
        if PasswordHash::new(phc).is_err() {
            return Err(
                Diagnostic::new(code::E0803, "the stored site password is not a PHC hash")
                    .help("rotate the password; the stored value cannot be verified against"),
            );
        }
        self.hashes
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(env.to_owned(), phc.to_owned());
        Ok(())
    }

    pub fn is_set(&self, env: &str) -> bool {
        self.hashes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(env)
    }

    /// `POST /_liyasa/auth/password`. The rate limits are charged before the
    /// hash is computed: an Argon2id verification is 64 MiB of work, and doing
    /// it for every request is the denial of service.
    pub fn check(&self, env: &str, address: &str, password: &str) -> Outcome {
        if !self.charge(env, address) {
            return Outcome::RateLimited;
        }
        let stored = self
            .hashes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(env)
            .cloned();
        match stored {
            // A site with no password set never accepts one, rather than
            // accepting every one.
            None => Outcome::Wrong,
            Some(stored) => match verify(password, &stored) {
                true => Outcome::Correct,
                false => Outcome::Wrong,
            },
        }
    }

    /// Re-hashes at the current parameters after a correct sign-in, if the
    /// stored hash is weaker than the configuration now asks for.
    pub fn upgrade(&self, env: &str, password: &str) -> bool {
        let stored = self
            .hashes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(env)
            .cloned();
        let Some(stored) = stored else {
            return false;
        };
        if !needs_rehash(&stored, self.params) {
            return false;
        }
        match hash(password, self.params) {
            Ok(fresh) => {
                self.hashes
                    .write()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(env.to_owned(), fresh);
                true
            }
            Err(_) => false,
        }
    }

    fn charge(&self, env: &str, address: &str) -> bool {
        let now = self.clock.now_ms();
        let window = millis(WINDOW);
        let mut attempts = self.attempts.write().unwrap_or_else(|e| e.into_inner());
        let bump = |map: &mut BTreeMap<String, (i64, u32)>, key: &str, limit: u32| {
            let slot = map.entry(key.to_owned()).or_insert((now, 0));
            if now.saturating_sub(slot.0) >= window {
                *slot = (now, 0);
            }
            slot.1 += 1;
            slot.1 <= limit
        };
        // Both are charged even when the first refuses, so a caller cannot
        // stay under the site limit by exhausting their own.
        let address_ok = bump(&mut attempts.per_address, address, PER_ADDRESS_PER_MINUTE);
        let site_ok = bump(&mut attempts.per_site, env, PER_SITE_PER_MINUTE);
        address_ok && site_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The floor is 64 MiB and three passes, which is about 40 ms per hash.
    /// A test doing dozens would spend a minute on it, so the unit tests use
    /// the smallest parameters Argon2 accepts and the one test that is about
    /// the floor uses the floor.
    fn cheap() -> Argon2Params {
        Argon2Params {
            memory_kib: 8,
            iterations: 1,
            parallelism: 1,
        }
    }

    fn passwords() -> Passwords {
        Passwords::new(cheap(), Clock::manual())
    }

    #[test]
    fn a_hash_round_trips_and_a_wrong_password_does_not() {
        let stored = hash("correct horse", cheap()).expect("a hash");
        assert!(verify("correct horse", &stored));
        assert!(!verify("correct horse ", &stored));
        assert!(!verify("", &stored));
    }

    #[test]
    fn the_stored_hash_is_argon2id_at_the_configured_parameters() {
        let stored = hash(
            "x",
            Argon2Params {
                memory_kib: 19_456,
                iterations: 2,
                parallelism: 1,
            },
        )
        .expect("a hash");
        assert!(stored.starts_with("$argon2id$"), "{stored}");
        assert!(stored.contains("m=19456"), "{stored}");
        assert!(stored.contains("t=2"), "{stored}");
        assert!(stored.contains("p=1"), "{stored}");
    }

    #[test]
    fn two_hashes_of_one_password_differ_because_the_salt_does() {
        let a = hash("same", cheap()).expect("a hash");
        let b = hash("same", cheap()).expect("a hash");
        assert_ne!(a, b, "a salt that repeats is not a salt");
        assert!(verify("same", &a) && verify("same", &b));
    }

    #[test]
    fn the_nfr_12_floor_produces_a_hash_of_the_documented_shape() {
        let stored = hash("x", Argon2Params::default()).expect("a hash");
        assert!(stored.contains("m=65536"), "{stored}");
        assert!(stored.contains("t=3"), "{stored}");
        let parsed = PasswordHash::new(&stored).expect("a PHC string");
        assert_eq!(
            parsed.salt.map(|s| s.as_ref().len()),
            Some(ARGON2_SALT_BYTES)
        );
        assert_eq!(parsed.hash.map(|h| h.len()), Some(ARGON2_TAG_BYTES));
    }

    #[test]
    fn a_value_that_is_not_a_phc_string_verifies_nothing() {
        for junk in [
            "",
            "hunter2",
            "$argon2id$",
            "$unknown$v=19$m=8,t=1,p=1$c2FsdA$aGFzaA",
        ] {
            assert!(!verify("hunter2", junk), "{junk}");
        }
    }

    #[test]
    fn a_site_with_no_password_set_accepts_none() {
        let passwords = passwords();
        assert!(!passwords.is_set("production"));
        assert_eq!(
            passwords.check("production", "203.0.113.1", "anything"),
            Outcome::Wrong
        );
    }

    #[test]
    fn a_rotated_password_replaces_the_previous_one() {
        let passwords = passwords();
        let first = passwords.rotate("production", "old").expect("a rotation");
        assert!(!first.had_previous);
        assert_eq!(
            passwords.check("production", "203.0.113.1", "old"),
            Outcome::Correct
        );

        let second = passwords.rotate("production", "new").expect("a rotation");
        assert!(second.had_previous);
        assert_eq!(
            passwords.check("production", "203.0.113.1", "old"),
            Outcome::Wrong,
            "the old password must stop working the moment it is rotated"
        );
        assert_eq!(
            passwords.check("production", "203.0.113.1", "new"),
            Outcome::Correct
        );
    }

    #[test]
    fn each_environment_has_its_own_password() {
        let passwords = passwords();
        passwords.rotate("production", "prod").expect("a rotation");
        passwords.rotate("preview", "prev").expect("a rotation");
        assert_eq!(
            passwords.check("production", "203.0.113.1", "prev"),
            Outcome::Wrong
        );
        assert_eq!(
            passwords.check("preview", "203.0.113.1", "prev"),
            Outcome::Correct
        );
    }

    #[test]
    fn one_address_is_cut_off_before_it_can_guess_many_times() {
        let passwords = passwords();
        passwords.rotate("production", "right").expect("a rotation");
        for _ in 0..PER_ADDRESS_PER_MINUTE {
            assert_ne!(
                passwords.check("production", "203.0.113.1", "wrong"),
                Outcome::RateLimited
            );
        }
        assert_eq!(
            passwords.check("production", "203.0.113.1", "wrong"),
            Outcome::RateLimited
        );
        assert_eq!(
            passwords.check("production", "203.0.113.1", "right"),
            Outcome::RateLimited,
            "the limit does not lift for a correct guess"
        );

        // Another address is unaffected, and the window reopens.
        assert_eq!(
            passwords.check("production", "203.0.113.2", "right"),
            Outcome::Correct
        );
        passwords.clock.advance(WINDOW);
        assert_eq!(
            passwords.check("production", "203.0.113.1", "right"),
            Outcome::Correct
        );
    }

    #[test]
    fn a_guess_spread_across_addresses_still_hits_the_site_limit() {
        let passwords = passwords();
        passwords.rotate("production", "right").expect("a rotation");
        let mut refused = 0;
        for n in 0..PER_SITE_PER_MINUTE + 10 {
            // A different address every time, so the per-address limit never
            // fires; only the per-site one can.
            let address = format!("203.0.113.{n}");
            if passwords.check("production", &address, "wrong") == Outcome::RateLimited {
                refused += 1;
            }
        }
        assert!(
            refused >= 10,
            "a distributed guess was not limited: {refused}"
        );
    }

    #[test]
    fn one_noisy_site_does_not_lock_out_another() {
        let passwords = passwords();
        passwords.rotate("production", "p").expect("a rotation");
        passwords.rotate("preview", "v").expect("a rotation");
        for n in 0..PER_SITE_PER_MINUTE + 5 {
            passwords.check("production", &format!("203.0.113.{n}"), "wrong");
        }
        assert_eq!(
            passwords.check("preview", "198.51.100.1", "v"),
            Outcome::Correct
        );
    }

    #[test]
    fn a_hash_below_the_configured_parameters_is_re_hashed_after_a_correct_sign_in() {
        let weak = Passwords::new(cheap(), Clock::manual());
        weak.rotate("production", "secret").expect("a rotation");

        let strong = Passwords::new(
            Argon2Params {
                memory_kib: 19_456,
                iterations: 2,
                parallelism: 1,
            },
            Clock::manual(),
        );
        let stored = weak
            .hashes
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get("production")
            .cloned()
            .expect("a hash");
        strong.install("production", &stored).expect("a valid hash");

        assert_eq!(
            strong.check("production", "203.0.113.1", "secret"),
            Outcome::Correct,
            "an old hash must keep verifying after the parameters are raised"
        );
        assert!(strong.upgrade("production", "secret"));
        assert!(!strong.upgrade("production", "secret"), "once is enough");
        assert_eq!(
            strong.check("production", "203.0.113.1", "secret"),
            Outcome::Correct
        );
    }

    #[test]
    fn a_hash_that_already_meets_the_parameters_is_left_alone() {
        let want = Argon2Params {
            memory_kib: 8,
            iterations: 1,
            parallelism: 1,
        };
        let stored = hash("x", want).expect("a hash");
        assert!(!needs_rehash(&stored, want));
        assert!(needs_rehash(&stored, Argon2Params::default()));
        assert!(needs_rehash("not a hash", want));
    }

    #[test]
    fn installing_something_that_is_not_a_hash_is_refused() {
        let passwords = passwords();
        let error = passwords
            .install("production", "hunter2")
            .expect_err("a plaintext is not a hash");
        assert_eq!(error.code, code::E0803);
        assert!(!passwords.is_set("production"));
    }
}
