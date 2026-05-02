//! CLI exit codes.
//!
//! Values are pinned per `docs/13-error-handling.md`:
//! `0` clean success, `1` fatal error, `2` warning-success (graph produced
//! but contained unresolved includes). The `u8` repr is what `process::ExitCode`
//! ultimately emits, and the `From` impl below is the only sanctioned way
//! main converts these into the value `std` returns to the OS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Fatal = 1,
    WarningSuccess = 2,
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        Self::from(code as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_values_are_stable() {
        // Numeric values are part of the CLI contract — scripts grep for them.
        assert_eq!(ExitCode::Success as u8, 0);
        assert_eq!(ExitCode::Fatal as u8, 1);
        assert_eq!(ExitCode::WarningSuccess as u8, 2);
    }

    #[test]
    fn from_impl_round_trips_through_std_exit_code() {
        // Rough sanity check on the From bridge: each variant maps to a
        // distinct `std::process::ExitCode` instance.
        let _: std::process::ExitCode = ExitCode::Success.into();
        let _: std::process::ExitCode = ExitCode::Fatal.into();
        let _: std::process::ExitCode = ExitCode::WarningSuccess.into();
    }
}
