//! CLI-31: the exit-code table.
//!
//! Every command returns one of these and nothing else. `liyasa-verify` has the
//! same table for its own report (`liyasa_verify::report::ExitCode`); the test
//! at the bottom of this file keeps the two from drifting.

/// The documented exit status of a `liyasa` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub enum Exit {
    /// Nothing went wrong.
    Success = 0,
    /// The command ran and found errors in the user's project.
    Errors = 1,
    /// The command line itself was wrong: unknown flag, bad value, no such
    /// subcommand. Never used for a problem with the project.
    Usage = 2,
    /// Verification checks failed (`verify`, `test`, `broken-links`).
    Verification = 3,
    /// A network request or an authentication attempt failed.
    Network = 4,
}

impl Exit {
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Whichever of the two is worse, so a command that does several things can
    /// fold its parts together.
    #[must_use]
    pub fn worst(self, other: Self) -> Self {
        if other > self { other } else { self }
    }

    /// The exit for a run that produced diagnostics.
    pub fn of_diagnostics(diagnostics: &liyasa_core::Diagnostics, strict: bool) -> Self {
        if diagnostics.has_errors() || (strict && !diagnostics.is_empty()) {
            Self::Errors
        } else {
            Self::Success
        }
    }
}

impl From<Exit> for std::process::ExitCode {
    fn from(exit: Exit) -> Self {
        // Every variant is 0..=4, so the cast cannot wrap.
        Self::from(exit.code() as u8)
    }
}
