---
id: CAS-FMT
title: "A8CAS - description of CAS file format"
edition: "Wayback Machine snapshot of 2026-04-14 (the A8CAS news of the page is dated 2015.12.10)"
author: "A8CAS project (the page does not name an author; FAQ 9.1 names Tomasz Krasuski as the author of the extended format)"
license: none found
storage: pin
url: "https://web.archive.org/web/20260414115505id_/https://a8cas.sourceforge.net/format-cas.html"
sha256: "411f4e953a2f785e4b15d737b95eaf69f002736be1abd61414be87527ec65fca"
file: "reference/pinned/a8cas-format-cas.html"
---
# CAS-FMT: A8CAS - description of CAS file format

A one-page description of the CAS cassette image format, on the web site of A8CAS (software
to read and write Atari 8-bit cassettes). The page gives the chunk structure of the file
(an 8-byte header and the data, all numbers little-endian) and the chunk types: `FUJI`
(tape description, must be first), `baud` (baud rate of the next `data` chunks, default
600), `data` (a standard SIO record with the inter-record gap in ms), `fsk ` (raw signal
lengths in 0.1 ms) and the turbo chunks `pwms`, `pwmc`, `pwmd` and `pwml`. The file is
16,211 bytes.

The URL is a Wayback Machine snapshot (2026-04-14) of
`https://a8cas.sourceforge.net/format-cas.html`. The `id_` form of the URL gives the page
bytes without the Wayback Machine toolbar.

The section index is `CAS-FMT.sections.txt`. Its keys are the headings of the page.

## License

The page has no license text and no copyright notice. The A8CAS main page also has no
license text for the web pages. The page does not give permission to redistribute, so the
project pins it by its URL and SHA-256 and does not put it in git.
