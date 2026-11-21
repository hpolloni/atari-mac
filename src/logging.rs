//! The log output of `--log` (REQ-085): the categories, the line format and the logger
//! that writes to standard error.
//!
//! Design: `docs/design/architecture.md`, "Frontend logger". SLICE-09 WP-2 adds the
//! parser `Categories::parse`, `format_line` and `StderrLogger`. The module is not named
//! `log`, because that name hides the `log` crate.

#[cfg(test)]
mod tests {
    use crate::planned::planned;

    /// Verifies REQ-085: `Categories::parse` accepts `tape`, `sio`, `reset`, `media`,
    /// `frontend`, `all`, each with or without `:debug`, in a list separated by commas.
    /// `all` turns on each category. It rejects `disk`, `sio:trace`, an empty item and an
    /// empty value with `FrontendError::Usage` (exit status 2), and the message names the
    /// item (criteria 1 and 4).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn log_categories_parse() {
        planned();
    }

    /// Verifies REQ-085: with `--log tape,sio:debug`, `Categories::enabled` is true for
    /// `atari::tape` at `Info`, for `atari::sio` at `Info` and `Debug`, and false for
    /// `atari::tape` at `Debug`, for `atari::reset` and for a target outside `atari::`.
    /// `max_level` is `Debug`; without `:debug` it is `Info` (criterion 1).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn logger_filters_category_and_level() {
        planned();
    }

    /// Verifies REQ-085: `format_line` of a record that the test makes with
    /// `log::Record::builder`, the target `atari::tape`, the keys `frame` = 12 and
    /// `cycle` = 345678 and the message `motor on` is
    /// `[frame 12 cycle 345678] tape: motor on`. A record without the keys gives
    /// `[frame - cycle -]`, and a target outside the categories gives `None` (criterion 2).
    #[test]
    #[ignore = "planned: SLICE-09"]
    fn log_line_format() {
        planned();
    }
}
