//! The slice 3 gate (#61): the #8 slice 3 clauses, one named test per clause,
//! through the registered Tauri commands (mock runtime) on disposable
//! installations in real temp dirs, with the fixture HTTPS server, the pinned
//! 7-Zip, injected faults, and **real** processes: a fake D2RLoader
//! ([`fake_loader`]) started for real and found by the real process listing.
//! Nothing here writes to a real D2R copy, except [`phase_b`] on a copy the
//! owner approved and names; nothing starts anything from one.
//!
//! | Clause | Tests |
//! | --- | --- |
//! | Build and loader matrix | [`build_loader_matrix`]: supported; same size, different hash; missing; link; unsupported loader; real version resource |
//! | Save and config preservation (real layout) | [`preservation`]: edited TOML kept; required-setting conflict pauses; saves folder never opened for write |
//! | Running-process protection | [`running_process`]: fake loader under the root blocks every operation; a process outside the root does not count |
//! | Explicit launch argv | [`explicit_launch`] |
//! | Restart | [`restart`]: pending journal blocks Play until recovery; running game rediscovered; Quit leaves the game running |
//! | Phase A read-only | [`phase_a`]: guard tripwire; phase A rehearsed on a sentinel-less real-layout copy; opt-in run on a real copy |
//! | Phase B, owner-approved writes | [`phase_b`]: adopt, repair, crash recovery, uninstall and install on a sentinel-less real-layout copy; opt-in run on an approved real copy |

#[path = "../support/mod.rs"]
mod support;

mod fake_loader;
mod rig;

mod build_loader_matrix;
mod explicit_launch;
mod phase_a;
mod phase_b;
mod preservation;
mod restart;
mod running_process;
