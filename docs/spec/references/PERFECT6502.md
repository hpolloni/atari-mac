---
id: PERFECT6502
title: "perfect6502"
edition: "Commit 09fc542877a84318291aa42dab143a3e2c3db974"
author: "Michael Steil; netlist by Greg James, Brian Silverman and Barry Silverman"
license: "Code: MIT License, Copyright (c) 2010-2020 Michael Steil, Brian Silverman, Barry Silverman. Netlist: Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported license, Copyright (c) 2010 Greg James, Brian Silverman, Barry Silverman"
storage: pin
url: "https://raw.githubusercontent.com/mist64/perfect6502/09fc542877a84318291aa42dab143a3e2c3db974/README.md"
sha256: "36970252b97213d485ae93faf4f2521c644782e8d3ff0525c3492f7aa001896a"
file: "reference/pinned/perfect6502-README.md"
---
# PERFECT6502: perfect6502

A transistor-level simulation of the NMOS 6502, made from the netlist of the visual6502.org
project. Its behavior is half-cycle exact. The repository is
`https://github.com/mist64/perfect6502` at the commit of `edition`.

The pinned file is the `README.md` of the repository at the commit. It identifies the
repository and the commit; the `sha256` is the SHA-256 of that file.

The section index is `PERFECT6502.sections.txt`. It has no section lines, so a citation is only
the ID: `PERFECT6502`.

## License

The repository has two licenses. The `README.md` has no license text.

The code has the MIT license. The file `LICENSE` at the commit says:

> MIT License
>
> Copyright (c) 2010-2020 Michael Steil, Brian Silverman, Barry Silverman
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all
> copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

The netlist, `netlist_6502.h`, has the Creative Commons BY-NC-SA 3.0 license. Its header says:

> Copyright (c) 2010 Greg James, Brian Silverman, Barry Silverman
>
> The following is provided under terms of the Creative Commons
> Attribution-NonCommercial-ShareAlike 3.0 Unported license:
> http://creativecommons.org/licenses/by-nc-sa/3.0/

No code of the project uses perfect6502 (OI-041), so the project pins it and does not put it
in git.
