//! The log output of `--log` (REQ-085): the categories, the line format and the logger
//! that writes to standard error.
//!
//! Design: `docs/design/architecture.md`, "Frontend logger". The module is not named
//! `log`, because that name hides the `log` crate.

use std::io::{self, Write as _};

use log::kv::Key;
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::error::FrontendError;

/// The target of the records of the frontend (REQ-085 criterion 3).
pub const FRONTEND_TARGET: &str = "atari::frontend";

/// The prefix of the target of each category.
const TARGET_PREFIX: &str = "atari::";

/// A category of `--log` (REQ-085 criterion 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The cassette deck (`atari::tape`, REQ-084).
    Tape,
    /// The SIO bus and the SIOV shortcut (`atari::sio`, REQ-084).
    Sio,
    /// The cold and warm resets (`atari::reset`, REQ-084).
    Reset,
    /// The media that the machine gets (`atari::media`, REQ-084).
    Media,
    /// The frontend: turbo, pause, cold reset from F10 (`atari::frontend`).
    Frontend,
}

impl Category {
    /// Each category, in the order of the help text.
    pub const ALL: [Self; 5] = [
        Self::Tape,
        Self::Sio,
        Self::Reset,
        Self::Media,
        Self::Frontend,
    ];

    /// `tape`, `sio`, `reset`, `media`, `frontend`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tape => "tape",
            Self::Sio => "sio",
            Self::Reset => "reset",
            Self::Media => "media",
            Self::Frontend => "frontend",
        }
    }

    /// The category of the target `atari::<name>`. `None` for another target.
    #[must_use]
    pub fn from_target(target: &str) -> Option<Self> {
        let name = target.strip_prefix(TARGET_PREFIX)?;
        Self::from_name(name)
    }

    /// The category with the name `name`.
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }

    /// The index of the category in [`Category::ALL`].
    const fn index(self) -> usize {
        match self {
            Self::Tape => 0,
            Self::Sio => 1,
            Self::Reset => 2,
            Self::Media => 3,
            Self::Frontend => 4,
        }
    }
}

/// The categories that `--log` turns on, each at `Info` or `Debug`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Categories {
    /// The highest level of each category of [`Category::ALL`], or `None` when it is off.
    levels: [Option<Level>; 5],
}

impl Categories {
    /// Parses the value of `--log`: a list separated by commas of the category names and
    /// `all`, each with an optional `:debug`. When a category is in more than one item,
    /// the highest level applies.
    ///
    /// # Errors
    ///
    /// `FrontendError::Usage` that names the first item that is not valid (also an empty
    /// item).
    pub fn parse(text: &str) -> Result<Self, FrontendError> {
        let mut categories = Self::default();
        for item in text.split(',') {
            let (name, level) = item
                .strip_suffix(":debug")
                .map_or((item, Level::Info), |name| (name, Level::Debug));
            let selected: &[Category] = if name == "all" {
                &Category::ALL
            } else {
                match Category::from_name(name) {
                    Some(category) => &[category][..],
                    None => {
                        return Err(FrontendError::Usage(format!(
                            "unknown --log category \"{item}\" (known: tape, sio, reset, media, frontend, all, each with an optional :debug)"
                        )));
                    }
                }
            };
            for category in selected {
                let slot = &mut categories.levels[category.index()];
                *slot = Some(slot.map_or(level, |old| old.max(level)));
            }
        }
        Ok(categories)
    }

    /// A record of `target` at `level` is written.
    #[must_use]
    pub fn enabled(&self, target: &str, level: Level) -> bool {
        Category::from_target(target)
            .and_then(|category| self.levels[category.index()])
            .is_some_and(|max| level <= max)
    }

    /// `Debug` when a category has `:debug`, else `Info`.
    #[must_use]
    pub fn max_level(&self) -> LevelFilter {
        if self.levels.contains(&Some(Level::Debug)) {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        }
    }
}

/// The key-value `key` of `record` as a `u64`, as text, or `-` when the record does not
/// have it.
fn count(record: &Record<'_>, key: &str) -> String {
    record
        .key_values()
        .get(Key::from_str(key))
        .and_then(|value| value.to_u64())
        .map_or_else(|| "-".to_owned(), |value| value.to_string())
}

/// The line of a record: `[frame <f> cycle <c>] <category>: <message>`. A count that the
/// record does not have is `-`. `None` for a target that is not a category.
#[must_use]
pub fn format_line(record: &Record<'_>) -> Option<String> {
    let category = Category::from_target(record.target())?;
    Some(format!(
        "[frame {} cycle {}] {}: {}",
        count(record, "frame"),
        count(record, "cycle"),
        category.name(),
        record.args()
    ))
}

/// The logger of `--log`: writes the line of each enabled record to standard error.
#[derive(Debug, Clone, Copy)]
pub struct StderrLogger {
    /// The categories that `--log` turns on.
    categories: Categories,
}

impl StderrLogger {
    /// A logger for `categories`.
    #[must_use]
    pub const fn new(categories: Categories) -> Self {
        Self { categories }
    }
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.categories.enabled(metadata.target(), metadata.level())
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if let Some(mut line) = format_line(record) {
            line.push('\n');
            // One write to the locked standard error, so that the lines of two threads do
            // not mix. A logger cannot give an error, so a write error is ignored.
            io::stderr().lock().write_all(line.as_bytes()).unwrap_or(());
        }
    }

    fn flush(&self) {}
}

/// Installs a [`StderrLogger`] (`log::set_logger` with `Box::leak`) and sets the maximum
/// level to `categories.max_level()`. A second call keeps the first logger and its level.
pub fn install(categories: Categories) {
    let logger: &'static StderrLogger = Box::leak(Box::new(StderrLogger::new(categories)));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(categories.max_level());
    }
}

#[cfg(test)]
mod tests {
    use log::kv::{ToValue as _, Value};
    use log::{Level, LevelFilter, Record};

    use super::{Categories, Category, format_line};
    use crate::error::FrontendError;

    /// The parsed categories of `text`.
    fn parse(text: &str) -> Categories {
        Categories::parse(text).unwrap_or_else(|error| panic!("{text}: {error}"))
    }

    /// The message of the error of `text`.
    fn parse_error(text: &str) -> String {
        match Categories::parse(text) {
            Err(error @ FrontendError::Usage(_)) => {
                assert_eq!(error.exit_code(), 2, "{text}: exit status");
                error.to_string()
            }
            other => panic!("{text} gave {other:?}"),
        }
    }

    /// Each category of `categories` that writes its records at `level`.
    fn on_at(categories: &Categories, level: Level) -> Vec<&'static str> {
        Category::ALL
            .into_iter()
            .filter(|category| categories.enabled(&format!("atari::{}", category.name()), level))
            .map(Category::name)
            .collect()
    }

    /// Verifies REQ-085: `Categories::parse` accepts `tape`, `sio`, `reset`, `media`,
    /// `frontend`, `all`, each with or without `:debug`, in a list separated by commas.
    /// `all` turns on each category. It rejects `disk`, `sio:trace`, an empty item and an
    /// empty value with `FrontendError::Usage` (exit status 2), and the message names the
    /// item (criteria 1 and 4).
    #[test]
    fn log_categories_parse() {
        let names = ["tape", "sio", "reset", "media", "frontend"];
        for name in names {
            let info = parse(name);
            assert_eq!(on_at(&info, Level::Info), [name], "{name} at Info");
            assert!(on_at(&info, Level::Debug).is_empty(), "{name}: no Debug");
            let debug = parse(&format!("{name}:debug"));
            assert_eq!(on_at(&debug, Level::Info), [name], "{name}:debug at Info");
            assert_eq!(on_at(&debug, Level::Debug), [name], "{name}:debug at Debug");
        }
        assert_eq!(on_at(&parse("all"), Level::Info), names, "all");
        assert!(
            on_at(&parse("all"), Level::Debug).is_empty(),
            "all: no Debug"
        );
        assert_eq!(on_at(&parse("all:debug"), Level::Debug), names, "all:debug");
        let list = parse("tape,media,frontend");
        assert_eq!(
            on_at(&list, Level::Info),
            ["tape", "media", "frontend"],
            "a list"
        );
        assert_eq!(
            parse("sio:debug,sio"),
            parse("sio:debug"),
            "the highest level applies"
        );
        assert_eq!(parse("tape,all"), parse("all"), "all with a category");
        for (text, item) in [
            ("disk", "disk"),
            ("tape,disk", "disk"),
            ("sio:trace", "sio:trace"),
            ("tape,,sio", ""),
            ("tape,", ""),
            ("", ""),
            ("TAPE", "TAPE"),
        ] {
            let message = parse_error(text);
            assert!(
                message.contains(&format!("unknown --log category \"{item}\"")),
                "{text:?}: {message}"
            );
        }
    }

    /// Verifies REQ-085: with `--log tape,sio:debug`, `Categories::enabled` is true for
    /// `atari::tape` at `Info`, for `atari::sio` at `Info` and `Debug`, and false for
    /// `atari::tape` at `Debug`, for `atari::reset` and for a target outside `atari::`.
    /// `max_level` is `Debug`; without `:debug` it is `Info` (criterion 1).
    #[test]
    fn logger_filters_category_and_level() {
        let categories = parse("tape,sio:debug");
        assert!(categories.enabled("atari::tape", Level::Info), "tape Info");
        assert!(categories.enabled("atari::sio", Level::Info), "sio Info");
        assert!(categories.enabled("atari::sio", Level::Debug), "sio Debug");
        assert!(categories.enabled("atari::sio", Level::Warn), "sio Warn");
        assert!(!categories.enabled("atari::sio", Level::Trace), "sio Trace");
        assert!(
            !categories.enabled("atari::tape", Level::Debug),
            "tape Debug"
        );
        assert!(!categories.enabled("atari::reset", Level::Info), "reset");
        assert!(
            !categories.enabled("atari::frontend", Level::Info),
            "frontend"
        );
        assert!(!categories.enabled("cpal", Level::Info), "outside atari::");
        assert!(
            !categories.enabled("tape", Level::Info),
            "no atari:: prefix"
        );
        assert!(
            !categories.enabled("atari::tape::deck", Level::Info),
            "a target below a category"
        );
        assert_eq!(categories.max_level(), LevelFilter::Debug, "with :debug");
        assert_eq!(
            parse("tape,sio").max_level(),
            LevelFilter::Info,
            "no :debug"
        );
        assert_eq!(parse("all").max_level(), LevelFilter::Info, "all");
    }

    /// Verifies REQ-085: `format_line` of a record that the test makes with
    /// `log::Record::builder`, the target `atari::tape`, the keys `frame` = 12 and
    /// `cycle` = 345678 and the message `motor on` is
    /// `[frame 12 cycle 345678] tape: motor on`. A record without the keys gives
    /// `[frame - cycle -]`, and a target outside the categories gives `None` (criterion 2).
    #[test]
    fn log_line_format() {
        let frame: u64 = 12;
        let cycle: u64 = 345_678;
        let pairs: [(&str, Value<'_>); 2] =
            [("frame", frame.to_value()), ("cycle", cycle.to_value())];
        let line = format_line(
            &Record::builder()
                .target("atari::tape")
                .level(Level::Info)
                .key_values(&pairs)
                .args(format_args!("motor on"))
                .build(),
        );
        assert_eq!(
            line.as_deref(),
            Some("[frame 12 cycle 345678] tape: motor on"),
            "with the counts"
        );
        let line = format_line(
            &Record::builder()
                .target("atari::frontend")
                .level(Level::Info)
                .args(format_args!("paused"))
                .build(),
        );
        assert_eq!(
            line.as_deref(),
            Some("[frame - cycle -] frontend: paused"),
            "without the counts"
        );
        let line = format_line(
            &Record::builder()
                .target("cpal")
                .level(Level::Info)
                .key_values(&pairs)
                .args(format_args!("stream"))
                .build(),
        );
        assert_eq!(line, None, "a target outside the categories");
    }
}
