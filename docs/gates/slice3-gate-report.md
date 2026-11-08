# Slice 3 gate report (#61): owner's D2R copy

Part of #41. The fixture half landed in #94. This report covers phase A and
phase B on the owner's D2R copy.

## Approval and target

- Approval: the owner's comment on #61 (2026-11-08): "run phase A and phase B
  against `C:\Users\micah\source\repos\d2r-game`, an exact clone of the owner's
  game copy. Never touch any other D2R install."
- Target: `C:\Users\micah\source\repos\d2r-game` only. No other D2R install was
  read or written. The saves folder (`%USERPROFILE%\Saved Games\...`) was not
  watched or touched: phase A ran with `SLICE3_GATE_SAVES` pointing to a
  folder that does not exist, so it watched no saves.
- The clone holds 29,869 files (47.6 GB). It contains D2R, D2RLoader, and the
  mods `PD2Resurrected`, `ProjectForever`, `Reimagined` and `ReimaginedLadder`.
- Backup: before phase B, a full copy was made to
  `C:\Users\micah\source\repos\d2r-game-backup-2026-11-08` with robocopy. The
  copy was verified by hashing every file with SHA-256 and comparing against
  the clone: 0 differences.

## Environment

| | |
| --- | --- |
| Windows | Windows 11 Pro 25H2, build 26200.9711 |
| D2R.exe | 3.3.93847, 32,109,264 bytes, SHA-256 `E1F5436E3D9687F644EF16938B1B183D1FDEF434F18CF66D852CF68F48CC8936` |
| D2RLoader.exe | 1.3.1-beta |
| Release | PD2R `pd2r-0.1.0-alpha` (`fixtures/catalog/real`), archive 72,790,802 bytes from GitHub Releases |
| Toolchain | rustc 1.90.0, debug test profile |
| Phase A commit | `eef70a2` (origin/main, guard unchanged) |
| Phase B commit | `8e52e68` (the run used this code before one rustfmt line join in `rig.rs`) |

## Phase A: read-only, guard unchanged (commit `eef70a2`)

Command (from `src-tauri`):

```
SLICE3_GATE_D2R=C:\Users\micah\source\repos\d2r-game
SLICE3_GATE_SAVES=<a folder that does not exist>
cargo test --test slice3_gate phase_a -- --nocapture
```

Result: **3 passed, 0 failed** (411.8 s).

| Checklist step | Evidence | Result |
| --- | --- | --- |
| Guard unchanged | `write_guard_is_unchanged_before_phase_b` | pass |
| 3. Snapshot hashes outside the tree | SHA-256 of all 29,869 files before and after, kept outside the repo and the clone | 0 differences |
| 4. Detection doesn't list the copy | `detected: []` | pass |
| 4. A folder without `D2R.exe` gives an inline error | `d2r_not_found` | pass |
| 4. Browse to the copy | build `3.3.93847`, store `Unknown` | pass |
| 5. Readiness | `NotInstalled`; `Ready` when recorded as adopted; `UpdateAvailable` with a newer release | pass |
| 5. Adoption inspection | 0 added, 0 replaced, preserved `d2rloader/config/d2rloader.toml` (edited), no conflicts | pass |
| 6. Every write refused | adopt, install, clean_install, repair, uninstall and update all failed with `product_root_not_disposable` | pass |
| 7. Diff against the snapshot | 0 changed entries (metadata of 32,482 entries in the test; SHA-256 of every file outside it) | pass |
| Nothing started | no spawns | pass |

Checklist step 8 (start the game, restart the launcher, quit the game) needs
the owner at the keyboard. See [Owner steps](#owner-steps-not-run-here).

## Phase B: the guard change

`WriteGuard` (`src-tauri/src/contract/guard.rs`) now accepts a product root that
is a **mod folder**, `<D2R>\mods\<folder>`, where:

- the parent is named `mods` (any case);
- `<D2R>\D2R.exe` is a regular file, not a link;
- `<D2R>` is not a drive root;
- no folder from the root up to the drive is a link or junction (reparse point);
- the root may be absent, so a fresh install works.

A root that holds the `.resurrected-disposable` sentinel still passes, so
fixtures and tests keep working. `check_with_backup` (clean-install recovery)
applies the same rule. The guard tripwire now pins the phase B guard
(`write_guard_is_the_phase_b_guard`). Phase A's real-copy test no longer
attempts writes, because they would now succeed. The refusals above are the
record of phase A.

## Phase B on the clone

Command (from `src-tauri`):

```
SLICE3_GATE_D2R_PHASE_B=C:\Users\micah\source\repos\d2r-game
cargo test --test slice3_gate phase_b_on_the_owner -- --nocapture
```

Result: **1 passed, 0 failed** (1,701.5 s; most of it is three downloads of
the 72.8 MB archive from GitHub). The launcher downloaded with the production
`Downloader` and extracted with the bundled 7-Zip. Its game host used the real
registry, PE versions and process list, and could not start anything.

Each step diffs a snapshot of the D2R folder. Under `mods`, the snapshot holds
the size and SHA-256 of every file; elsewhere, it holds metadata. It has
32,481 entries.

| Checklist step | Expected | Seen | Result |
| --- | --- | --- | --- |
| 1. Adopt | only `mods\PD2Resurrected.resurrected-lock` is new; product files byte-identical | `+mods/PD2Resurrected.resurrected-lock`, nothing else; Ready | pass |
| 3. Edit `d2rloader.toml`, delete `PD2Resurrected.mpq/data/global/excel/armor.txt`, Repair | file restored (manifest hash), edit kept, plugins, other mods and saves unchanged | the only change since adopt is the TOML edit; inspection listed it as preserved | pass |
| 4. Repair killed during activation, then reopen | recovery before Play, complete installation, old or new | injected crash at `StagedMovedToLive`; new launcher: `RecoveryRequired`, Play refused with `recovery_required`; startup recovery **rolled back** to the installation before the repair (missing only the deleted file), journal gone, Ready; a second Repair then restored the file | pass |
| 5. Uninstall | only managed files go; edited config, plugins, other mods, D2R, loader stay; kept paths shown | 644 entries removed, all managed files or folders in `mods/PD2Resurrected/`; inspection's kept paths: `d2rloader/config/d2rloader.toml` (edit intact); nothing outside the product folder changed | pass |
| 6. Install fresh | 72.8 MB archive downloads, verifies and extracts with the bundled 7-Zip | succeeded, Ready, every managed file matches the manifest hash, TOML edit kept; the tree equals the one after step 3 | pass |
| Nothing started | | no spawns | pass |

Outside the test, SHA-256 of all 29,869 files after phase B against the
backup shows exactly two differences: the new
`mods\PD2Resurrected.resurrected-lock` and the edited
`mods\PD2Resurrected\d2rloader\config\d2rloader.toml`. Both were then reverted
(the TOML copied back from the backup, the lock deleted), so the clone matches
its original state for the owner's steps.

The plugins that ship with PD2R (`d2rloader/plugins/*.dll`) are managed files,
so Repair and Install restore them by hash. Unknown plugins and the loader's
`d2rloader/logs` are left untouched; the fixture rehearsal
(`phase_b_writes_only_the_mod_folder_of_a_real_layout_copy`) checks an unknown
plugin and another mod by hash on every CI run.

The first attempt timed out. The rig waited only 120 s for an outcome, so the
test exited during Repair's download and activation, and left a journal at
`StagedMovedToLive`. `mods\` was restored from the backup and checked by
SHA-256 (17,240 files, 0 differences). The online rig now waits 30 minutes,
and the second run is the one reported above.

## Owner steps (not run here)

These need someone at the keyboard, or another machine:

On the clone (`C:\Users\micah\source\repos\d2r-game`, now back to its original
state), with the launcher built from this branch:

1. Phase A step 8: start the game your usual way. Expected: Running, and every
   operation is refused with GameRunning. Restart the launcher: still Running.
   Quit the game: back to ready.
2. Phase B step 2: Adopt, then Play. Expected: `D2RLoader.exe -mod
   PD2Resurrected` starts and your characters load. Repair and Update are
   disabled while the game runs. Exiting the game returns to Ready. Quitting
   the launcher during play leaves the game running.
3. Phase B step 4 for real: kill the launcher in Task Manager during Repair's
   activation, then reopen it. Expected: recovery runs before Play.
4. Phase B step 7: on a copy without the loader, run Set up D2RLoader.
   Expected: an unsigned disclosure and a consent prompt, the installer's own
   UI or UAC prompt, then the loader shows ready.
5. Phase B step 8: repeat steps 1-3 on Windows 10 22H2 and on a stock
   Battle.net install, and record the exact versions.

To restore the clone afterwards:
`robocopy C:\Users\micah\source\repos\d2r-game-backup-2026-11-08 C:\Users\micah\source\repos\d2r-game /MIR`.

## Gaps

- The owner steps above are not run. Because of them, this PR is "Part of
  #61", not "Closes #61".
- After a rolled-back recovery, readiness is Ready even though the old
  installation lacks the file the repair was restoring. This matches the
  fixture restart test and the spec ("complete installation, old or new"),
  since "old" here was already missing that file. Repair fixes it.
- No production defects were found. The junctioned remembered-folder defect
  from #94's Gaps was already fixed on main (`1ea5a4d`); its test passes.
