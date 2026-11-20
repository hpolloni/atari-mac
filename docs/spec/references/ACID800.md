---
id: ACID800
title: "Altirra Acid800 test suite"
edition: "1.2"
author: "Avery Lee"
license: "The Acid800 test suite is licensed under the MIT license. Copyright (C) 2010-2022 Avery Lee, All Rights Reserved."
storage: pin
url: "https://web.archive.org/web/20251231052911id_/https://www.virtualdub.org/downloads/Acid800-1.2.7z"
sha256: "4074e6a40db1833e97603e334ca130f5923bff34a0d4f8a83e2ecf9617de1b20"
file: "reference/pinned/Acid800-1.2.7z"
---
# ACID800: Altirra Acid800 test suite

A test suite for the Atari 8-bit computers. Each test checks a corner case of the CPU, ANTIC,
GTIA, POKEY, the PIA or the MMU that passes on real hardware. The archive has the source
files, a bootable ATR disk image, standalone executables and the Acid5200 cartridge images.
The URL is a Wayback Machine snapshot of the file on virtualdub.org.

The section index is `ACID800.sections.txt`. It has one key for each test of the main suite,
the test name that the suite uses, for example `ACID800 antic_wsync`.

## License

`README.html` in the archive, section "License", says:

> The Acid800 test suite is licensed under the MIT license.

Its section "License text" says:

> Altirra Acid800 test suite
> Copyright (C) 2010-2022 Avery Lee, All Rights Reserved.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
> THE SOFTWARE.

Each source file has the same notice with its own years. The MIT license permits
redistribution, but the archive contains ROM and ATR files, which are not in git (OI-041), so
the project pins it.
