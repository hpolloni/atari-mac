# References

This directory holds the references that the specification cites and that git stores
(storage `commit`, REQ-046). Each file is a verbatim copy: the licenses below permit
redistribution only without changes. `.gitattributes` stops git from changing the bytes of
the files in this directory. The registry entries are in
[`docs/spec/references/`](../docs/spec/references/).

| File | Registry entry | SHA-256 |
| --- | --- | --- |
| `altirra-hardware-reference.pdf` | [`HRM`](../docs/spec/references/HRM.md) | `1909927618a7f4a6771d2dc7ef1785ba9b13453edba57122165f57c1504d1c89` |
| `atari-8bit-faq.txt` | [`FAQ`](../docs/spec/references/FAQ.md) | `a3033e32d7faea60dcdb02eb2ddb323f1ea76e2062f187c6f86b5261667239b7` |

Git ignores two subdirectories:

- `pinned/`: the references with the storage `pin`. `make reference` downloads them.
- `text/`: text that you extract from a PDF on your computer, for example with
  `pdftotext -layout altirra-hardware-reference.pdf text/altirra-hardware-reference.txt`.

## Altirra Hardware Reference Manual

`altirra-hardware-reference.pdf`, 2026-01-02 Edition, by Avery Lee. Downloaded from
<https://www.virtualdub.org/downloads/Altirra%20Hardware%20Reference%20Manual.pdf>.

The copyright notice and the license, from the manual:

> Copyright © 2009-2024 Avery Lee, All Rights Reserved.
>
> Permission is granted to redistribute this document in verbatim form as long as it is done
> free of charge and for non-commercial purposes.

This repository redistributes the manual free of charge and for non-commercial purposes.

## Atari 8-Bit Computers FAQ

`atari-8bit-faq.txt`, "Last-modified: August 2, 2026", by Michael D. Current. Downloaded from
<https://mcurrent.name/atari-8-bit/faq.txt>.

The copyright notice and the license, from the header of the file:

> Copyright (c) 1992-2026 by Michael D. Current, and others where noted.  Feel
> free to reproduce this file, in whole or in part, so long as the content of
> that portion reproduced is not modified, and so long as credit is given to
> this FAQ list or its Maintainer, or the author of that section reproduced
> when given.

Credit: the Atari 8-Bit Computers Frequently Asked Questions List of the comp.sys.atari.8bit
newsgroup, maintained by Michael D. Current, and the authors of its
sections where the file names them. The file in this directory is the complete list, not
modified.
