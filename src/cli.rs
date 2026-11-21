//! The command line of `atari-mac`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;

use atari_core::VideoStandard;

use crate::error::FrontendError;
use crate::frontend::DisplayFilters;

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
  --xex <file>         Executable (not supported yet)
  --disk <file>        ATR disk image in drive D1: (the OS boots it)
  --bas <file>         Type a BASIC program from a text file at Ready, then RUN it
  --no-run             With --bas: type the program, but do not RUN it
  --pal                PAL machine (default: NTSC)
  --scale <n>          Window scale, 1 to 8 (default: 2)
  --frames <n>         Run n frames, then print the elapsed time and exit
  --headless           No window and no audio (needs --frames)
  --audio-stats        At exit, print the audio sample rate and the underrun count
  --screenshot <file>  At exit, save the last frame as a PNG file
  --artifacting        NTSC artifacting filter: false colors for hires pixel patterns
  --pal-blending       PAL chroma blending filter (needs a PAL machine)
  -h, --help           Show this text

Keys: F1 inverse, F2 START, F3 SELECT, F4 OPTION, F5 RESET, F6 HELP, F7 BREAK,
F9 pause, F10 cold reset, arrow keys and Left Option joystick 1, Command+Escape quit.";

/// The settings of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The OS ROM file.
    pub os: PathBuf,
    /// The BASIC ROM file.
    pub basic: PathBuf,
    /// The cartridge file.
    pub cart: Option<PathBuf>,
    /// The ATR disk image file for D1: (`--disk`, REQ-012).
    pub disk: Option<PathBuf>,
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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            os: PathBuf::from(DEFAULT_OS_ROM),
            basic: PathBuf::from(DEFAULT_BASIC_ROM),
            cart: None,
            disk: None,
            headless: false,
            frames: None,
            screenshot: None,
            scale: DEFAULT_SCALE,
            standard: VideoStandard::Ntsc,
            filters: DisplayFilters::default(),
            bas: None,
            no_run: false,
            audio_stats: false,
        }
    }
}

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Show the help text.
    Help,
    /// Run the emulator.
    Run(Options),
}

/// Makes a usage error.
const fn usage(message: String) -> FrontendError {
    FrontendError::Usage(message)
}

/// Takes the value of the option `name`.
fn value(args: &mut impl Iterator<Item = OsString>, name: &str) -> Result<OsString, FrontendError> {
    args.next()
        .ok_or_else(|| usage(format!("{name} needs a value")))
}

/// Takes the numeric value of the option `name`.
fn number<T: FromStr>(
    args: &mut impl Iterator<Item = OsString>,
    name: &str,
) -> Result<T, FrontendError> {
    let text = value(args, name)?;
    text.to_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| {
            usage(format!(
                "{name} needs a number, not {:?}",
                text.display().to_string()
            ))
        })
}

/// Parses the command line arguments (without the program name).
///
/// # Errors
///
/// [`FrontendError::Usage`] for an argument that is not valid, and
/// [`FrontendError::Unsupported`] for `--xex`.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, FrontendError> {
    let mut args = args.into_iter();
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.to_str().unwrap_or_default() {
            "-h" | "--help" => return Ok(Command::Help),
            "--os" => options.os = value(&mut args, "--os")?.into(),
            "--basic" => options.basic = value(&mut args, "--basic")?.into(),
            "--cart" => options.cart = Some(value(&mut args, "--cart")?.into()),
            "--xex" => return Err(FrontendError::Unsupported("--xex")),
            "--disk" => options.disk = Some(value(&mut args, "--disk")?.into()),
            "--headless" => options.headless = true,
            "--pal" => options.standard = VideoStandard::Pal,
            "--frames" => options.frames = Some(number(&mut args, "--frames")?),
            "--screenshot" => options.screenshot = Some(value(&mut args, "--screenshot")?.into()),
            "--artifacting" => options.filters.artifacting = true,
            "--pal-blending" => options.filters.pal_blending = true,
            "--bas" => options.bas = Some(value(&mut args, "--bas")?.into()),
            "--no-run" => options.no_run = true,
            "--audio-stats" => options.audio_stats = true,
            "--scale" => {
                let scale = number(&mut args, "--scale")?;
                if !(1..=MAX_SCALE).contains(&scale) {
                    return Err(usage(format!(
                        "--scale must be 1 to {MAX_SCALE}, not {scale}"
                    )));
                }
                options.scale = scale;
            }
            _ => {
                return Err(usage(format!(
                    "unknown argument {:?}",
                    arg.display().to_string()
                )));
            }
        }
    }
    if options.headless && options.frames.is_none() {
        return Err(usage("--headless needs --frames".to_owned()));
    }
    if options.no_run && options.bas.is_none() {
        return Err(usage("--no-run needs --bas".to_owned()));
    }
    check_filters(&options)?;
    Ok(Command::Run(options))
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

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn parse_ok(list: &[&str]) -> Options {
        match parse(args(list)) {
            Ok(Command::Run(options)) => options,
            other => panic!("{list:?} gave {other:?}"),
        }
    }

    fn parse_err(list: &[&str]) -> String {
        match parse(args(list)) {
            Err(error) => error.to_string(),
            other => panic!("{list:?} gave {other:?}"),
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
        ]);
        assert_eq!(
            options,
            Options {
                os: "a.rom".into(),
                basic: "b.rom".into(),
                cart: Some("c.car".into()),
                disk: Some("d.atr".into()),
                headless: true,
                frames: Some(180),
                screenshot: Some("out.png".into()),
                scale: 3,
                standard: VideoStandard::Pal,
                filters: DisplayFilters::default(),
                bas: Some("game.bas".into()),
                no_run: true,
                audio_stats: true,
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
            (&["--no-run"], "--no-run needs --bas"),
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
    fn xex_is_not_supported() {
        let result = parse(args(&["--xex", "game"]));
        assert!(
            matches!(result, Err(FrontendError::Unsupported(f)) if f == "--xex"),
            "--xex"
        );
    }
}
