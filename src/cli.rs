//! The command line of `atari-mac`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;

use crate::error::FrontendError;

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

Atari 65XE emulator (NTSC).

Options:
  --os <file>          OS ROM, 16384 bytes (default: roms/altirraos-xl.rom)
  --basic <file>       BASIC ROM, 8192 bytes (default: roms/altirra-basic.rom)
  --cart <file>        Cartridge: a raw image of 8192 or 16384 bytes
  --xex <file>         Executable (not supported yet)
  --disk <file>        ATR disk image (not supported yet)
  --scale <n>          Window scale, 1 to 8 (default: 2)
  --frames <n>         Run n frames, then print the elapsed time and exit
  --headless           No window and no audio (needs --frames)
  --screenshot <file>  At exit, save the last frame as a PNG file
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
    /// Run without a window.
    pub headless: bool,
    /// The number of frames to run. `None` runs until the user quits.
    pub frames: Option<u64>,
    /// The PNG file for the last frame.
    pub screenshot: Option<PathBuf>,
    /// The window scale.
    pub scale: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            os: PathBuf::from(DEFAULT_OS_ROM),
            basic: PathBuf::from(DEFAULT_BASIC_ROM),
            cart: None,
            headless: false,
            frames: None,
            screenshot: None,
            scale: DEFAULT_SCALE,
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
/// [`FrontendError::Unsupported`] for `--xex` and `--disk`.
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
            "--disk" => return Err(FrontendError::Unsupported("--disk")),
            "--headless" => options.headless = true,
            "--frames" => options.frames = Some(number(&mut args, "--frames")?),
            "--screenshot" => options.screenshot = Some(value(&mut args, "--screenshot")?.into()),
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
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{Command, Options, parse};
    use crate::error::FrontendError;

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
            "--headless",
            "--frames",
            "180",
            "--screenshot",
            "out.png",
            "--scale",
            "3",
        ]);
        assert_eq!(
            options,
            Options {
                os: "a.rom".into(),
                basic: "b.rom".into(),
                cart: Some("c.car".into()),
                headless: true,
                frames: Some(180),
                screenshot: Some("out.png".into()),
                scale: 3,
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
        ] {
            let error = parse_err(list);
            assert!(error.starts_with(message), "{list:?}: {error}");
        }
    }

    #[test]
    fn xex_and_disk_are_not_supported() {
        for flag in ["--xex", "--disk"] {
            let result = parse(args(&[flag, "game"]));
            assert!(
                matches!(result, Err(FrontendError::Unsupported(f)) if f == flag),
                "{flag}"
            );
        }
    }
}
