# ROMs

Free replacement ROMs by Avery Lee, from the Altirra emulator. The ROM files are not in git.
Run `scripts/fetch-roms.sh` (or `make roms`) to download them. The script extracts them from the
C arrays in the atari800 project, commit `98cc2dc749867c9c7317a0271ba062a6c797f886`
(`src/roms/altirraos_xl.c` and `src/roms/altirra_basic.c`).

| File | Contents | Size | SHA-256 |
| --- | --- | --- | --- |
| `altirraos-xl.rom` | AltirraOS 3.11, XL/XE OS ROM (`$C000-$FFFF`) | 16384 | `9de5a313fe3946f04fe236a8d3ceacb471fbed4ec5fc5db009732e1169946ccf` |
| `altirra-basic.rom` | Altirra BASIC 1.59 (`$A000-$BFFF`) | 8192 | `19fd64377895eb5d4414319855322dd3f860c21fdc1df9adc0e98ab72c540913` |

## License

Both files carry this notice:

> Copyright (C) 2008-2018 Avery Lee (AltirraOS), Copyright (C) 2008-2022 Avery Lee (Altirra BASIC)
>
> Copying and distribution of this file, with or without modification, are permitted in any
> medium without royalty provided the copyright notice and this notice are preserved. This
> file is offered as-is, without any warranty.
