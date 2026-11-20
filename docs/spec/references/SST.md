---
id: SST
title: "SingleStepTests 65x02"
edition: "Commit 2f6980a2d95757486c7bee24355c360e40e2a224, test sets v1"
author: "Thomas Harte et al."
license: "MIT License. Copyright (c) 2024 Thomas Harte et al"
storage: pin
url: "https://raw.githubusercontent.com/SingleStepTests/65x02/2f6980a2d95757486c7bee24355c360e40e2a224/LICENSE"
sha256: "5fb64d1f2e4d74fabc85c768e1f6ffa73b95dc93e217c55acfe7e4a89ba32b04"
file: "reference/pinned/single-step-65x02-LICENSE"
---
# SST: SingleStepTests 65x02

Test data for processors of the 6502 family. Each test executes one instruction and gives the
processor and memory state before and after, and the bus activity of each cycle. The
repository is about 1.1 GB.

The pinned file is the `LICENSE` file of the repository at the commit, from
`https://raw.githubusercontent.com/SingleStepTests/65x02/2f6980a2d95757486c7bee24355c360e40e2a224/LICENSE`.
The `sha256` is the SHA-256 of that file. The test data itself is not checked by this entry:
`scripts/single-step-6502.sha1` holds the SHA-1 of each JSON file of `6502/v1`.

The section index is `SST.sections.txt`. It has one key for each test folder, for example
`SST 6502`.

## License

The file `LICENSE` at the commit says:

> MIT License
>
> Copyright (c) 2024 Thomas Harte et al
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

The MIT license permits redistribution, but the repository is too large for git (OI-041), so
the project pins it.
