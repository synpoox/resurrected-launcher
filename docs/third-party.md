# Third-party components

## 7-Zip console (bundled)

The launcher extracts release archives only with this pinned 7-Zip build (#35, architecture resolution in #8). Rust refuses to run any other file: `SevenZip` in `src-tauri/src/install/extract.rs` checks the size and SHA-256 below when it is located and again before every invocation.

| | |
| --- | --- |
| Product | 7-Zip Extra, standalone console `7za.exe` (x64) |
| Version | 26.04 (2026-10-05) |
| Author | Igor Pavlov |
| Source | <https://www.7-zip.org/a/7z2604-extra.7z>, served by www.7-zip.org itself |
| Package size | 1,764,616 bytes |
| Package SHA-256 | `dc4b11d3399db18b063630137145f5585d8f7ac847bf3639bd1185d7d1f7cee0` |
| Unpacked with | <https://www.7-zip.org/a/7zr.exe> (26.04, 602,624 bytes, SHA-256 `256feca8e274e5da655e2a284fabafd9f554365eb164862089dacd4e8276d282`) |
| Cross-check | Both SHA-256 values equal the `digest` GitHub reports for the same assets of the author's official release `ip7z/7zip` tag `26.04` |
| Signature | None: 7-Zip binaries are not Authenticode-signed |
| License | GNU LGPL 2.1+, with BSD 3-clause and BSD 2-clause parts (see `License.txt`) |

Bundled files in `src-tauri/resources/7zip/`:

| File | From the package | Size | SHA-256 |
| --- | --- | --- | --- |
| `7za.exe` | `x64/7za.exe` | 1,335,808 | `15d4c788c148e3677e2fc1c4a01f191fb6669eba4f6acf8a465f0f1a6f1e1260` |
| `License.txt` | `License.txt` | 5,188 | `39d2187b942cc88818dd959e53c6b7dddd152097a42061467c4d56050168d646` |

`License.txt` is shipped unchanged because the license requires binary redistributions to reproduce it. `7za.exe` needs no DLLs.

Why 26.04: it was the newest release served from www.7-zip.org itself on 2026-11-07. Newer releases (26.05, 26.06) are linked from the download page but hosted only on GitHub. 26.04 includes the fixes for CVE-2026-14266 (26.02) and CVE-2026-58052 (26.03), but not the unnamed "bugs and vulnerabilities" fixes that 26.05 and 26.06 list. Re-pin once a newer release is served from www.7-zip.org.

The launcher runs only two fixed invocations: `l -slt` (list) and `x` (extract into fresh staging). The archive format is always named: `-t7z`, the only format the release manifest allows. Nothing reaches these invocations from the frontend. Tests also use `-tzip` and `-ttar` fixtures.

Not yet proven: compatibility with real PD2R release archives (slice 3 gate in #8).

### Updating the pin

1. Download `7z<ver>-extra.7z` and `7zr.exe` from www.7-zip.org into a new empty directory, and compare their SHA-256 with the official GitHub release digests.
2. Unpack with that `7zr.exe`, then copy only `x64/7za.exe` and `License.txt` here.
3. Update `SEVEN_ZIP_VERSION`, `SEVEN_ZIP_SIZE` and `SEVEN_ZIP_SHA256` in `extract.rs` and the tables above, then run `cargo test`.
