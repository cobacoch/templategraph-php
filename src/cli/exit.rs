//! CLI exit codes.
//!
//! Values are pinned per `docs/13-error-handling.md`:
//! `0` clean success, `1` fatal error, `2` warning-success (graph produced
//! but contained unresolved includes). The `u8` repr is what
//! `process::ExitCode` ultimately emits, and the `Termination` impl below
//! is the only sanctioned way `main` returns these to the OS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Fatal = 1,
    WarningSuccess = 2,
}

impl std::process::Termination for ExitCode {
    fn report(self) -> std::process::ExitCode {
        std::process::ExitCode::from(self as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Termination;

    #[test]
    fn exit_code_values_are_stable() {
        // Numeric values are part of the CLI contract — scripts grep for them.
        assert_eq!(ExitCode::Success as u8, 0);
        assert_eq!(ExitCode::Fatal as u8, 1);
        assert_eq!(ExitCode::WarningSuccess as u8, 2);
    }

    #[test]
    fn termination_impl_is_defined_for_each_variant() {
        // `std::process::ExitCode` is opaque, so we cannot assert the
        // numeric value coming out the other side; this test only proves
        // the conversion is wired up for every variant. The wire contract
        // itself is locked in by `exit_code_values_are_stable`.
        let _ = ExitCode::Success.report();
        let _ = ExitCode::Fatal.report();
        let _ = ExitCode::WarningSuccess.report();
    }
}
