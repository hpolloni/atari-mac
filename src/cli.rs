//! The command line of `atari-mac`.

use core::error::Error as _;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::str::FromStr;

use atari_core::VideoStandard;
use clap::builder::{OsStringValueParser, TypedValueParser};
use clap::error::ErrorKind;
use clap::{Arg, Command as ClapCommand, CommandFactory as _, Error as ClapError};
use clap::{FromArgMatches as _, Parser};

use crate::error::FrontendError;
use crate::frontend::DisplayFilters;
use crate::logging::Categories;
use crate::trace::TraceOptions;

/// The default OS ROM file (the free Altirra OS, see `roms/README.md`).
pub const DEFAULT_OS_ROM: &str = "roms/altirraos-xl.rom";

/// The default BASIC ROM file (the free Altirra BASIC, see `roms/README.md`).
pub const DEFAULT_BASIC_ROM: &str = "roms/altirra-basic.rom";

/// The default window scale (REQ-008).
pub const DEFAULT_SCALE: usize = 2;

/// The largest window scale.
pub const MAX_SCALE: usize = 8;

/// The text of `--help`.
pub const USAGE: &str = "\
Usage: atari-mac [options]

Atari 65XE emulator (NTSC, or PAL with --pal).

Options:
  --os <file>          OS ROM, 16384 bytes (default: roms/altirraos-xl.rom)
  --basic <file>       BASIC ROM, 8192 bytes (default: roms/altirra-basic.rom)
  --cart <file>        Cartridge: a raw image of 8192 or 16384 bytes, or a CAR file
  --xex <file>         Executable (XEX): boot it from D1: with BASIC disabled
  --disk <file>        ATR disk image in drive D1: (the OS boots it)
  --no-fast-disk       Read the disk at the real speed (no SIOV shortcut)
  --cas <file>         CAS tape image: boot it (holds START at power-on)
  --no-fast-tape       Read the tape records at the real speed (no tape shortcut)
  --auto-return        With --cas: press RETURN after the prompt tone
  --no-basic           Hold OPTION at power-on: boot with BASIC off
  --bas <file>         Type a BASIC program from a text file at Ready, then RUN it
  --no-run             With --bas: type the program, but do not RUN it
  --pal                PAL machine (default: NTSC)
  --scale <n>          Window scale, 1 to 8 (default: 2)
  --frames <n>         Run n frames, then print the elapsed time and exit
  --headless           No window and no audio (needs --frames)
  --audio-stats        At exit, print the audio sample rate and the underrun count
  --stats              Each second, write the speed statistics to standard error
  --log <categories>   Write log records to standard error: tape,sio,reset,media,frontend,all
  --trace <file>       Write a line for each CPU instruction and interrupt to the file
                       (- is standard output)
  --trace-from <frame> The first frame of the trace (default: 0)
  --trace-frames <n>   The number of frames of the trace (default: 1)
  --screenshot <file>  At exit, save the last frame as a PNG file
  --artifacting        NTSC artifacting filter: false colors for hires pixel patterns
  --pal-blending       PAL chroma blending filter (needs a PAL machine)
  -h, --help           Show this text

Keys: F1 inverse, F2 START, F3 SELECT, F4 OPTION, F5 RESET, F6 HELP, F7 BREAK,
F9 pause, F10 cold reset, arrow keys and Left Option joystick 1, Command+Escape quit.";

/// The settings of one run.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one flag of the command line"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The OS ROM file.
    pub os: PathBuf,
    /// The BASIC ROM file.
    pub basic: PathBuf,
    /// The cartridge file.
    pub cart: Option<PathBuf>,
    /// The executable file (`--xex`, REQ-012, REQ-612). It boots from D1:.
    pub xex: Option<PathBuf>,
    /// The ATR disk image file for D1: (`--disk`, REQ-012).
    pub disk: Option<PathBuf>,
    /// The CAS tape image file (`--cas`, REQ-081). The machine boots it.
    pub cas: Option<PathBuf>,
    /// The tape shortcut (REQ-644) is on: `true` unless `--no-fast-tape` (REQ-091).
    pub fast_tape: bool,
    /// The SIOV shortcut (REQ-623) is on: `true` unless `--no-fast-disk` (REQ-094).
    pub fast_disk: bool,
    /// With `--cas`: the frontend presses RETURN after the prompt tone (`--auto-return`,
    /// REQ-092).
    pub auto_return: bool,
    /// The frontend holds OPTION at the power-on cold reset until frame 60, so BASIC is off
    /// (`--no-basic`, REQ-093).
    pub no_basic: bool,
    /// Run without a window.
    pub headless: bool,
    /// The number of frames to run. `None` runs until the user quits.
    pub frames: Option<u64>,
    /// The PNG file for the last frame.
    pub screenshot: Option<PathBuf>,
    /// The window scale.
    pub scale: usize,
    /// The video standard of the machine: PAL with `--pal`, else NTSC (REQ-002).
    pub standard: VideoStandard,
    /// The optional display filters (REQ-019).
    pub filters: DisplayFilters,
    /// The BASIC program text file to type (`--bas`, REQ-080).
    pub bas: Option<PathBuf>,
    /// Type the program, but do not type `RUN` (`--no-run`, REQ-080).
    pub no_run: bool,
    /// At exit, print the audio numbers (`--audio-stats`, REQ-011).
    pub audio_stats: bool,
    /// Each second, write the speed statistics to standard error (`--stats`, REQ-083).
    pub stats: bool,
    /// The log categories to write to standard error (`--log`, REQ-085). A second `--log`
    /// replaces the first.
    pub log: Option<Categories>,
    /// The CPU instruction trace (`--trace`, `--trace-from`, `--trace-frames`, REQ-086).
    pub trace: Option<TraceOptions>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            os: PathBuf::from(DEFAULT_OS_ROM),
            basic: PathBuf::from(DEFAULT_BASIC_ROM),
            cart: None,
            xex: None,
            disk: None,
            cas: None,
            fast_tape: true,
            fast_disk: true,
            auto_return: false,
            no_basic: false,
            headless: false,
            frames: None,
            screenshot: None,
            scale: DEFAULT_SCALE,
            standard: VideoStandard::Ntsc,
            filters: DisplayFilters::default(),
            bas: None,
            no_run: false,
            audio_stats: false,
            stats: false,
            log: None,
            trace: None,
        }
    }
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show the help text.
    Help,
    /// Run the emulator.
    Run(Box<Options>),
}

/// Makes a usage error.
const fn usage(message: String) -> FrontendError {
    FrontendError::Usage(message)
}

/// The command line as clap reads it. Each option is one argument (`--name`, never
/// `--name=value`), and an option with a value takes the next argument as the value, also
/// when it starts with `-`. A later option replaces an earlier one. The text of `--help` is
/// [`USAGE`].
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is one flag of the command line"
)]
#[derive(Debug, Parser)]
#[command(
    name = "atari-mac",
    no_binary_name = true,
    override_help = USAGE,
    disable_version_flag = true,
    args_override_self = true,
    allow_hyphen_values = true
)]
struct Args {
    /// `--os <file>`.
    #[arg(long, value_parser = path())]
    os: Option<PathBuf>,
    /// `--basic <file>`.
    #[arg(long, value_parser = path())]
    basic: Option<PathBuf>,
    /// `--cart <file>`.
    #[arg(long, value_parser = path())]
    cart: Option<PathBuf>,
    /// `--xex <file>`.
    #[arg(long, value_parser = path())]
    xex: Option<PathBuf>,
    /// `--disk <file>`.
    #[arg(long, value_parser = path())]
    disk: Option<PathBuf>,
    /// `--no-fast-disk`.
    #[arg(long)]
    no_fast_disk: bool,
    /// `--cas <file>`.
    #[arg(long, value_parser = path())]
    cas: Option<PathBuf>,
    /// `--no-fast-tape`.
    #[arg(long)]
    no_fast_tape: bool,
    /// `--auto-return`.
    #[arg(long)]
    auto_return: bool,
    /// `--no-basic`.
    #[arg(long)]
    no_basic: bool,
    /// `--bas <file>`.
    #[arg(long, value_parser = path())]
    bas: Option<PathBuf>,
    /// `--no-run`.
    #[arg(long)]
    no_run: bool,
    /// `--pal`.
    #[arg(long)]
    pal: bool,
    /// `--scale <n>`.
    #[arg(long, value_parser = scale())]
    scale: Option<usize>,
    /// `--frames <n>`.
    #[arg(long, value_parser = number::<u64>("--frames"))]
    frames: Option<u64>,
    /// `--headless`.
    #[arg(long)]
    headless: bool,
    /// `--audio-stats`.
    #[arg(long)]
    audio_stats: bool,
    /// `--stats`.
    #[arg(long)]
    stats: bool,
    /// `--log <categories>`.
    #[arg(long, value_parser = log_categories())]
    log: Option<Categories>,
    /// `--trace <file>`.
    #[arg(long, value_parser = path())]
    trace: Option<PathBuf>,
    /// `--trace-from <frame>`.
    #[arg(long, value_parser = number::<u64>("--trace-from"))]
    trace_from: Option<u64>,
    /// `--trace-frames <n>`.
    #[arg(long, value_parser = number::<u64>("--trace-frames"))]
    trace_frames: Option<u64>,
    /// `--screenshot <file>`.
    #[arg(long, value_parser = path())]
    screenshot: Option<PathBuf>,
    /// `--artifacting`.
    #[arg(long)]
    artifacting: bool,
    /// `--pal-blending`.
    #[arg(long)]
    pal_blending: bool,
}

impl Args {
    /// The settings of the run, with the defaults for each option that is not given.
    ///
    /// # Errors
    ///
    /// The errors of [`TraceArgs::options`].
    fn options(self) -> Result<Options, FrontendError> {
        let trace = TraceArgs {
            file: self.trace,
            from: self.trace_from,
            frames: self.trace_frames,
        };
        Ok(Options {
            os: self.os.unwrap_or_else(|| PathBuf::from(DEFAULT_OS_ROM)),
            basic: self
                .basic
                .unwrap_or_else(|| PathBuf::from(DEFAULT_BASIC_ROM)),
            cart: self.cart,
            xex: self.xex,
            disk: self.disk,
            cas: self.cas,
            fast_tape: !self.no_fast_tape,
            fast_disk: !self.no_fast_disk,
            auto_return: self.auto_return,
            no_basic: self.no_basic,
            headless: self.headless,
            frames: self.frames,
            screenshot: self.screenshot,
            scale: self.scale.unwrap_or(DEFAULT_SCALE),
            standard: if self.pal {
                VideoStandard::Pal
            } else {
                VideoStandard::Ntsc
            },
            filters: DisplayFilters {
                artifacting: self.artifacting,
                pal_blending: self.pal_blending,
            },
            bas: self.bas,
            no_run: self.no_run,
            audio_stats: self.audio_stats,
            stats: self.stats,
            log: self.log,
            trace: trace.options()?,
        })
    }
}

/// The value parser of a file option: any value, also an empty one.
fn path() -> impl TypedValueParser<Value = PathBuf> {
    OsStringValueParser::new().map(PathBuf::from)
}

/// The value parser of the numeric option `name`.
fn number<T>(name: &'static str) -> impl TypedValueParser<Value = T>
where
    T: FromStr + Clone + Send + Sync + 'static,
{
    OsStringValueParser::new().try_map(move |text| parse_number(name, &text))
}

/// The value parser of `--scale`: a number from 1 to [`MAX_SCALE`].
fn scale() -> impl TypedValueParser<Value = usize> {
    OsStringValueParser::new().try_map(|text| {
        let scale = parse_number("--scale", &text)?;
        if (1..=MAX_SCALE).contains(&scale) {
            Ok(scale)
        } else {
            Err(format!("--scale must be 1 to {MAX_SCALE}, not {scale}"))
        }
    })
}

/// The value parser of `--log` ([`Categories::parse`]).
fn log_categories() -> impl TypedValueParser<Value = Categories> {
    OsStringValueParser::new().try_map(|text| {
        let text = text
            .to_str()
            .ok_or_else(|| format!("unknown --log category {:?}", text.display().to_string()))?;
        Categories::parse(text).map_err(|error| match error {
            FrontendError::Usage(message) => message,
            other => other.to_string(),
        })
    })
}

/// Parses `text`, the value of the numeric option `name`.
///
/// # Errors
///
/// The message of the usage error for a value that is not a number of type `T`.
fn parse_number<T: FromStr>(name: &str, text: &OsStr) -> Result<T, String> {
    text.to_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| {
            format!(
                "{name} needs a number, not {:?}",
                text.display().to_string()
            )
        })
}

/// The trace options as the command line gives them (REQ-086).
#[derive(Debug, Default)]
struct TraceArgs {
    /// `--trace <file>`.
    file: Option<PathBuf>,
    /// `--trace-from <frame>`.
    from: Option<u64>,
    /// `--trace-frames <n>`.
    frames: Option<u64>,
}

impl TraceArgs {
    /// The trace options, with the defaults (from frame 0, 1 frame).
    ///
    /// # Errors
    ///
    /// [`FrontendError::Usage`] for `--trace-frames 0`, and for `--trace-from` or
    /// `--trace-frames` without `--trace`.
    fn options(self) -> Result<Option<TraceOptions>, FrontendError> {
        if self.frames == Some(0) {
            return Err(usage("--trace-frames must be at least 1".to_owned()));
        }
        match self.file {
            Some(file) => Ok(Some(TraceOptions {
                file,
                from: self.from.unwrap_or(0),
                frames: self.frames.unwrap_or(1),
            })),
            None if self.from.is_some() || self.frames.is_some() => Err(usage(
                "--trace-from and --trace-frames need --trace".to_owned(),
            )),
            None => Ok(None),
        }
    }
}

/// The number of arguments at the start of `args` that clap reads. The first argument after
/// them is unknown: it is not exactly an option of `command` (`--name` or `-h`), and it is not
/// the value of the option before it. clap does not get it, because clap reads some unknown
/// arguments as options (`--os=file`, `-hx`) or as the end of the options (`--`).
fn known_len(command: &ClapCommand, args: &[OsString]) -> usize {
    let mut len = 0;
    while let Some(option) = args.get(len).and_then(|arg| find_option(command, arg)) {
        len += if option.get_action().takes_values() {
            2
        } else {
            1
        };
    }
    len.min(args.len())
}

/// The option of `command` that `arg` names exactly, for example `--os` or `-h`.
fn find_option<'a>(command: &'a ClapCommand, arg: &OsStr) -> Option<&'a Arg> {
    let text = arg.to_str()?;
    command.get_arguments().find(|option| {
        option
            .get_long()
            .is_some_and(|long| text.strip_prefix("--") == Some(long))
            || option.get_short().is_some_and(|short| {
                text.strip_prefix('-')
                    .is_some_and(|rest| rest.chars().eq([short]))
            })
    })
}

/// The usage error for the clap error `error`. `read` are the arguments that clap read.
fn clap_error(error: &ClapError, read: &[OsString]) -> FrontendError {
    let message = match error.kind() {
        ErrorKind::ValueValidation => error.source().map(ToString::to_string),
        // Only the last argument can miss its value: each option takes the next argument.
        ErrorKind::InvalidValue => read
            .last()
            .map(|name| format!("{} needs a value", name.display())),
        _ => None,
    };
    usage(message.unwrap_or_else(|| error.to_string()))
}

/// Parses the command line arguments (without the program name).
///
/// # Errors
///
/// [`FrontendError::Usage`] for an argument that is not valid, for `--xex` with `--disk`
/// (both use D1:) or with `--bas` (an executable starts with BASIC disabled), and for `--cas`
/// with `--bas` or `--xex` (each makes key presses at the start, REQ-081 criterion 6), and
/// for `--no-basic` with `--bas` (REQ-093 criterion 5).
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, FrontendError> {
    let args: Vec<OsString> = args.into_iter().collect();
    let mut command = Args::command();
    command.build();
    let (read, unknown) = args.split_at(known_len(&command, &args));
    let matches = match command.try_get_matches_from_mut(read) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => return Ok(Command::Help),
        Err(error) => return Err(clap_error(&error, read)),
    };
    if let Some(arg) = unknown.first() {
        return Err(usage(format!(
            "unknown argument {:?}",
            arg.display().to_string()
        )));
    }
    let options = Args::from_arg_matches(&matches)
        .map_err(|error| clap_error(&error, read))?
        .options()?;
    check_combinations(&options)?;
    check_filters(&options)?;
    Ok(Command::Run(Box::new(options)))
}

/// Checks the options that need another option or that cannot be used together, in this
/// order.
///
/// # Errors
///
/// [`FrontendError::Usage`] that names the options of the first broken rule.
fn check_combinations(options: &Options) -> Result<(), FrontendError> {
    let program = options.bas.is_some();
    let tape = options.cas.is_some();
    let executable = options.xex.is_some();
    let rules = [
        (
            options.headless && options.frames.is_none(),
            "--headless needs --frames",
        ),
        (options.no_run && !program, "--no-run needs --bas"),
        (
            executable && options.disk.is_some(),
            "--xex and --disk cannot be used together: both use drive D1:",
        ),
        (
            executable && program,
            "--xex and --bas cannot be used together: an executable starts with BASIC disabled",
        ),
        (
            tape && program,
            "--cas and --bas cannot be used together: both make key presses at the start",
        ),
        (
            options.no_basic && program,
            "--no-basic and --bas cannot be used together: --bas types the program into BASIC",
        ),
        (
            options.auto_return && !tape,
            "--auto-return needs --cas: it presses RETURN for a tape boot",
        ),
        (
            tape && executable,
            "--cas and --xex cannot be used together: both make key presses at the start",
        ),
    ];
    rules
        .into_iter()
        .find(|&(broken, _)| broken)
        .map_or(Ok(()), |(_, message)| Err(usage(message.to_owned())))
}

/// The name of a video standard in messages.
#[must_use]
pub const fn standard_name(standard: VideoStandard) -> &'static str {
    match standard {
        VideoStandard::Ntsc => "NTSC",
        VideoStandard::Pal => "PAL",
    }
}

/// Checks that each display filter matches the video standard (REQ-019 criterion 5).
///
/// # Errors
///
/// [`FrontendError::Usage`] for `--artifacting` on a PAL machine or `--pal-blending` on an
/// NTSC machine. The message names the flag and the video standard.
pub fn check_filters(options: &Options) -> Result<(), FrontendError> {
    let name = standard_name(options.standard);
    let wrong = |flag: &str, needs: &str| {
        Err(usage(format!(
            "{flag} needs {needs} machine, but the machine is {name}"
        )))
    };
    match options.standard {
        VideoStandard::Pal if options.filters.artifacting => wrong("--artifacting", "an NTSC"),
        VideoStandard::Ntsc if options.filters.pal_blending => wrong("--pal-blending", "a PAL"),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use atari_core::VideoStandard;

    use super::{Command, Options, check_filters, parse};
    use crate::error::FrontendError;
    use crate::frontend::DisplayFilters;
    use crate::logging::Categories;
    use crate::trace::TraceOptions;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn parse_ok(list: &[&str]) -> Options {
        match parse(args(list)) {
            Ok(Command::Run(options)) => *options,
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    fn parse_err(list: &[&str]) -> String {
        match parse(args(list)) {
            Err(error) => error.to_string(),
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    /// The text of the usage error of `list`. The error has exit code 2.
    fn usage_message(list: &[&str]) -> String {
        let error = match parse(args(list)) {
            Err(error) => error,
            other => panic!("{list:?} gave {other:?}"),
        };
        assert_eq!(error.exit_code(), 2, "exit code of {list:?}");
        match error {
            FrontendError::Usage(message) => message,
            other => panic!("{list:?} gave {other:?}, not a usage error"),
        }
    }

    /// Checks that each command line of `cases` gives the usage error with its message.
    fn assert_usage(cases: &[(&[&str], &str)]) {
        for &(list, message) in cases {
            assert_eq!(usage_message(list), message, "{list:?}");
        }
    }

    #[test]
    fn defaults() {
        let options = parse_ok(&[]);
        assert_eq!(options, Options::default(), "no arguments");
        assert_eq!(
            options.os,
            PathBuf::from("roms/altirraos-xl.rom"),
            "default OS ROM"
        );
        assert_eq!(
            options.basic,
            PathBuf::from("roms/altirra-basic.rom"),
            "default BASIC ROM"
        );
        assert_eq!(options.scale, 2, "default scale");
    }

    #[test]
    fn all_options() {
        let options = parse_ok(&[
            "--os",
            "a.rom",
            "--basic",
            "b.rom",
            "--cart",
            "c.car",
            "--disk",
            "d.atr",
            "--headless",
            "--frames",
            "180",
            "--screenshot",
            "out.png",
            "--scale",
            "3",
            "--pal",
            "--bas",
            "game.bas",
            "--no-run",
            "--audio-stats",
            "--stats",
            "--log",
            "tape",
            "--log",
            "sio:debug,frontend",
        ]);
        assert_eq!(
            options,
            Options {
                os: "a.rom".into(),
                basic: "b.rom".into(),
                cart: Some("c.car".into()),
                xex: None,
                disk: Some("d.atr".into()),
                cas: None,
                fast_tape: true,
                fast_disk: true,
                auto_return: false,
                no_basic: false,
                headless: true,
                frames: Some(180),
                screenshot: Some("out.png".into()),
                scale: 3,
                standard: VideoStandard::Pal,
                filters: DisplayFilters::default(),
                bas: Some("game.bas".into()),
                no_run: true,
                audio_stats: true,
                stats: true,
                log: Categories::parse("sio:debug,frontend").ok(),
                trace: None,
            },
            "all options"
        );
    }

    #[test]
    fn help() {
        for flag in ["-h", "--help"] {
            assert_eq!(parse(args(&[flag])).ok(), Some(Command::Help), "{flag}");
        }
    }

    #[test]
    fn bad_arguments() {
        for (list, message) in [
            (&["--os"][..], "--os needs a value"),
            (&["--frames", "x"], "--frames needs a number, not \"x\""),
            (&["--frames", "-1"], "--frames needs a number"),
            (&["--scale", "0"], "--scale must be 1 to 8, not 0"),
            (&["--scale", "9"], "--scale must be 1 to 8, not 9"),
            (&["--fast"], "unknown argument \"--fast\""),
            (&["--headless"], "--headless needs --frames"),
            (&["--bas"], "--bas needs a value"),
            (&["--disk"], "--disk needs a value"),
            (&["--cas"], "--cas needs a value"),
            (&["--no-run"], "--no-run needs --bas"),
            (&["--log"], "--log needs a value"),
            (&["--log", "disk"], "unknown --log category \"disk\""),
        ] {
            let error = parse_err(list);
            assert!(error.starts_with(message), "{list:?}: {error}");
        }
    }

    /// Verifies REQ-019: the flags set the filters, and each filter needs its video standard
    /// (criterion 5, for an NTSC and a PAL machine).
    #[test]
    fn filter_flags_and_standards() {
        let options = parse_ok(&["--artifacting"]);
        assert!(options.filters.artifacting, "--artifacting");
        assert!(!options.filters.pal_blending, "no --pal-blending");
        assert!(
            parse_err(&["--pal-blending"])
                .starts_with("--pal-blending needs a PAL machine, but the machine is NTSC"),
            "--pal-blending on NTSC"
        );
        let pal = |artifacting, pal_blending| Options {
            standard: VideoStandard::Pal,
            filters: DisplayFilters {
                artifacting,
                pal_blending,
            },
            ..Options::default()
        };
        assert!(
            check_filters(&pal(false, true)).is_ok(),
            "--pal-blending on PAL"
        );
        assert_eq!(
            parse_ok(&["--pal", "--pal-blending"]),
            pal(false, true),
            "--pal --pal-blending"
        );
        assert!(
            parse_err(&["--pal", "--artifacting"])
                .starts_with("--artifacting needs an NTSC machine, but the machine is PAL"),
            "--pal --artifacting"
        );
        let error = check_filters(&pal(true, false)).map_err(|e| e.to_string());
        assert!(
            matches!(&error, Err(m) if m.starts_with("--artifacting needs an NTSC machine, but the machine is PAL")),
            "--artifacting on PAL: {error:?}"
        );
    }

    #[test]
    fn xex_option() {
        assert_eq!(
            parse_ok(&["--xex", "game.xex"]).xex,
            Some(PathBuf::from("game.xex")),
            "--xex"
        );
        assert!(
            parse_err(&["--xex"]).starts_with("--xex needs a value"),
            "--xex without a file"
        );
        assert!(
            parse_err(&["--xex", "a.xex", "--disk", "b.atr"])
                .starts_with("--xex and --disk cannot be used together"),
            "--xex with --disk"
        );
        assert!(
            parse_err(&["--bas", "a.bas", "--xex", "a.xex"])
                .starts_with("--xex and --bas cannot be used together"),
            "--xex with --bas"
        );
    }

    /// `--cas` sets the tape file, and `--cas` with `--bas` or `--xex` is a usage error that
    /// names both options.
    #[test]
    fn cas_option() {
        assert_eq!(
            parse_ok(&["--cas", "game.cas"]).cas,
            Some(PathBuf::from("game.cas")),
            "--cas"
        );
        assert!(
            parse_err(&["--cas", "a.cas", "--bas", "a.bas"])
                .starts_with("--cas and --bas cannot be used together"),
            "--cas with --bas"
        );
        assert!(
            parse_err(&["--xex", "a.xex", "--cas", "a.cas"])
                .starts_with("--cas and --xex cannot be used together"),
            "--cas with --xex"
        );
    }

    /// Verifies REQ-091: the tape shortcut is on without `--no-fast-tape`, and
    /// `--no-fast-tape` turns it off (criteria 1, 2).
    #[test]
    fn no_fast_tape_parses() {
        assert!(parse_ok(&["--cas", "a.cas"]).fast_tape, "on by default");
        assert!(
            !parse_ok(&["--cas", "a.cas", "--no-fast-tape"]).fast_tape,
            "--no-fast-tape"
        );
    }

    /// Verifies REQ-094: the SIOV shortcut is on without `--no-fast-disk`, also with `--xex`
    /// and with no media, and `--no-fast-disk` turns it off (criteria 1, 2).
    #[test]
    fn no_fast_disk_parses() {
        assert!(parse_ok(&["--disk", "a.atr"]).fast_disk, "on with --disk");
        assert!(parse_ok(&["--xex", "a.xex"]).fast_disk, "on with --xex");
        assert!(parse_ok(&[]).fast_disk, "on with no media");
        assert!(
            !parse_ok(&["--disk", "a.atr", "--no-fast-disk"]).fast_disk,
            "--no-fast-disk"
        );
    }

    /// The options that take a value, as the user types them.
    const VALUE_OPTIONS: [&str; 14] = [
        "--os",
        "--basic",
        "--cart",
        "--xex",
        "--disk",
        "--cas",
        "--bas",
        "--scale",
        "--frames",
        "--log",
        "--trace",
        "--trace-from",
        "--trace-frames",
        "--screenshot",
    ];

    /// Verifies REQ-096 criterion 1: an argument that is not an option and not the value of
    /// an option is unknown, also an option with `=` and a value, two short options in one
    /// argument, `--` and an empty argument, at the start and after a valid option.
    #[test]
    fn unknown_argument_forms() {
        let forms = [
            "--os=file",
            "--pal=1",
            "--frames=10",
            "--help=1",
            "-hx",
            "-hh",
            "--",
            "",
            "-",
            "-x",
            "--fast",
            "--PAL",
            "pal",
            "file.rom",
            "---pal",
        ];
        for form in forms {
            let message = format!("unknown argument {form:?}");
            for list in [&[form][..], &["--pal", form], &["--os", "a.rom", form]] {
                assert_eq!(usage_message(list), message, "{list:?}");
            }
        }
    }

    /// Verifies REQ-096 criterion 2: an option that takes a value, as the last argument,
    /// needs a value, and the next argument is the value, also when it starts with `-`.
    #[test]
    fn missing_value_is_last() {
        for option in VALUE_OPTIONS {
            let message = format!("{option} needs a value");
            assert_eq!(usage_message(&[option]), message, "{option} alone");
            assert_eq!(usage_message(&["--pal", option]), message, "--pal {option}");
        }
        for value in [
            "--pal",
            "-h",
            "--help",
            "--",
            "-",
            "--os",
            "--os=file",
            "-hx",
        ] {
            let os = parse_ok(&["--os", value]).os;
            assert_eq!(os, PathBuf::from(value), "--os {value}");
        }
        assert_eq!(
            parse_ok(&["--screenshot", "--headless"]).screenshot,
            Some(PathBuf::from("--headless")),
            "--screenshot --headless"
        );
        assert!(!parse_ok(&["--bas", "--no-run"]).no_run, "--bas --no-run");
        assert_usage(&[
            (
                &["--frames", "--pal"],
                "--frames needs a number, not \"--pal\"",
            ),
            (&["--scale", "-h"], "--scale needs a number, not \"-h\""),
        ]);
    }

    /// Verifies REQ-096 criterion 3: a number option with a value that is not a decimal
    /// number (digits only, with an optional leading `+`) needs a number, and a decimal
    /// number is its value.
    #[test]
    fn bad_number_values() {
        let values = [
            "x", "", "-1", "1.5", " 1", "1 ", "0x10", "1e3", "-0", "++1", "+",
        ];
        for option in ["--frames", "--trace-from", "--trace-frames"] {
            for value in values {
                let message = format!("{option} needs a number, not {value:?}");
                let list = ["--trace", "t.txt", "--headless", option, value];
                assert_eq!(usage_message(&list), message, "{list:?}");
            }
        }
        let list = ["--frames", "0", "--trace", "t", "--trace-from", "007"];
        let options = parse_ok(&list);
        assert_eq!(options.frames, Some(0), "--frames 0");
        assert_eq!(options.trace.map(|trace| trace.from), Some(7), "007");
        let max = "18446744073709551615";
        assert_eq!(parse_ok(&["--frames", max]).frames, Some(u64::MAX), "max");
        assert_eq!(parse_ok(&["--frames", "+1"]).frames, Some(1), "--frames +1");
        let list = ["--trace", "t", "--trace-from", "+2", "--trace-frames", "+3"];
        let trace = parse_ok(&list)
            .trace
            .map(|trace| (trace.from, trace.frames));
        assert_eq!(trace, Some((2, 3)), "{list:?}");
    }

    /// Verifies REQ-096 criterion 4: `--scale` outside 1 to 8 is an error that names the
    /// value, and 1 to 8 are the scale.
    #[test]
    fn scale_out_of_range() {
        for (value, shown) in [
            ("0", "0"),
            ("9", "9"),
            ("10", "10"),
            ("100", "100"),
            ("00", "0"),
            ("18446744073709551615", "18446744073709551615"),
        ] {
            let message = format!("--scale must be 1 to 8, not {shown}");
            assert_eq!(
                usage_message(&["--scale", value]),
                message,
                "--scale {value}"
            );
        }
        for scale in 1..=8_usize {
            let value = scale.to_string();
            assert_eq!(
                parse_ok(&["--scale", &value]).scale,
                scale,
                "--scale {value}"
            );
        }
    }

    /// Verifies REQ-096 criterion 6: with more than one syntax error, the message is for the
    /// first error from the left.
    #[test]
    fn first_syntax_error_wins() {
        let frames = "--frames needs a number, not \"x\"";
        let scale = "--scale must be 1 to 8, not 0";
        let unknown = "unknown argument \"--fast\"";
        assert_usage(&[
            (&["--frames", "x", "--fast"], frames),
            (&["--fast", "--frames", "x"], unknown),
            (&["--frames", "x", "--scale", "0"], frames),
            (&["--scale", "0", "--frames", "x"], scale),
            (&["--scale", "0", "--fast"], scale),
            (&["--fast", "--scale", "0"], unknown),
            (&["--frames", "x", "--os"], frames),
            (&["--scale", "0", "--os"], scale),
            (&["--fast", "--os"], unknown),
            (&["--fast", "--", "--os=a"], unknown),
            (&["--frames", "x", "--frames", "1"], frames),
            (&["--frames", "1", "--frames", "x"], frames),
        ]);
    }

    /// Verifies REQ-097 criteria 1 to 6: the message of each combination rule, and the
    /// command lines that keep each rule.
    #[test]
    fn combination_messages() {
        let trace = "--trace-from and --trace-frames need --trace";
        let at_least = "--trace-frames must be at least 1";
        assert_usage(&[
            (&["--headless"], "--headless needs --frames"),
            (
                &["--xex", "a.xex", "--disk", "d.atr"],
                "--xex and --disk cannot be used together: both use drive D1:",
            ),
            (
                &["--xex", "a.xex", "--bas", "p.bas"],
                "--xex and --bas cannot be used together: an executable starts with BASIC disabled",
            ),
            (
                &["--cas", "t.cas", "--xex", "a.xex"],
                "--cas and --xex cannot be used together: both make key presses at the start",
            ),
            (&["--trace-from", "1"], trace),
            (&["--trace-frames", "2"], trace),
            (&["--trace-from", "1", "--trace-frames", "2"], trace),
            (&["--trace-frames", "0"], at_least),
            (&["--trace", "t", "--trace-frames", "0"], at_least),
        ]);
        let trace_options = |from, frames| {
            Some(TraceOptions {
                file: PathBuf::from("t"),
                from,
                frames,
            })
        };
        let list = ["--trace", "t"];
        assert_eq!(parse_ok(&list).trace, trace_options(0, 1), "{list:?}");
        let list = ["--trace", "t", "--trace-from", "3", "--trace-frames", "2"];
        assert_eq!(parse_ok(&list).trace, trace_options(3, 2), "{list:?}");
        let list = ["--headless", "--frames", "1"];
        assert_eq!(parse_ok(&list).frames, Some(1), "{list:?}");
        let list = ["--cas", "t.cas", "--auto-return"];
        assert!(parse_ok(&list).auto_return, "{list:?}");
    }

    /// A command line that breaks each rule of REQ-097 criterion 8, in the order of the
    /// criterion, with the message of the rule.
    const RULES: [(&[&str], &str); 11] = [
        (
            &["--trace-frames", "0"],
            "--trace-frames must be at least 1",
        ),
        (
            &["--trace-from", "1"],
            "--trace-from and --trace-frames need --trace",
        ),
        (&["--headless"], "--headless needs --frames"),
        (&["--no-run"], "--no-run needs --bas"),
        (
            &["--xex", "a.xex", "--disk", "d.atr"],
            "--xex and --disk cannot be used together: both use drive D1:",
        ),
        (
            &["--xex", "a.xex", "--bas", "p.bas"],
            "--xex and --bas cannot be used together: an executable starts with BASIC disabled",
        ),
        (
            &["--cas", "t.cas", "--bas", "p.bas"],
            "--cas and --bas cannot be used together: both make key presses at the start",
        ),
        (
            &["--no-basic", "--bas", "p.bas"],
            "--no-basic and --bas cannot be used together: --bas types the program into BASIC",
        ),
        (
            &["--auto-return"],
            "--auto-return needs --cas: it presses RETURN for a tape boot",
        ),
        (
            &["--cas", "t.cas", "--xex", "a.xex"],
            "--cas and --xex cannot be used together: both make key presses at the start",
        ),
        (
            &["--pal-blending"],
            "--pal-blending needs a PAL machine, but the machine is NTSC",
        ),
    ];

    /// The index in [`RULES`] of `--auto-return` without `--cas`. A command line cannot break
    /// it together with the next rule (`--cas` with `--xex`), so the test pairs it with the
    /// filter rule.
    const AUTO_RETURN: usize = 8;

    /// Verifies REQ-097 criterion 8: for each adjacent pair of rules, with the arguments in
    /// both orders, the message is for the earlier rule. The pair `--auto-return` without
    /// `--cas` and `--cas` with `--xex` cannot both be broken (the first needs no `--cas`, the
    /// second needs `--cas`), so it has no order to test: each of the two is paired with the
    /// filter rule instead. Also verifies REQ-097 criterion 7 (second sentence): a syntax
    /// error comes before each combination error.
    #[test]
    fn combination_order() {
        let syntax = "unknown argument \"--fast\"";
        for (index, &(list, message)) in RULES.iter().enumerate() {
            assert_eq!(usage_message(list), message, "rule {index} alone");
            let later = if index == AUTO_RETURN {
                index + 2
            } else {
                index + 1
            };
            let Some(&(other, _)) = RULES.get(later) else {
                continue;
            };
            for both in [[list, other].concat(), [other, list].concat()] {
                assert_eq!(usage_message(&both), message, "{both:?}");
                let unknown = [both.as_slice(), &["--fast"]].concat();
                assert_eq!(usage_message(&unknown), syntax, "{unknown:?}");
            }
        }
        let all: Vec<&str> = RULES
            .iter()
            .flat_map(|&(list, _)| list.iter().copied())
            .collect();
        assert_eq!(usage_message(&all), RULES[0].1, "{all:?}");
        let scale = [all.as_slice(), &["--scale", "9"]].concat();
        let message = "--scale must be 1 to 8, not 9";
        assert_eq!(usage_message(&scale), message, "{scale:?}");
    }
}
