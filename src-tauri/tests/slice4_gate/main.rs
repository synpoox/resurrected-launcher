//! The slice 4 gate (#81): the #8 slice 4 clauses, one named test or recorded
//! check per clause, through the registered Tauri commands and the real
//! lifecycle builder (mock runtime) on disposable installations in real temp
//! dirs, with the fixture HTTPS server, injected faults and **real** processes:
//! `examples/fake-d2rloader.rs` as D2RLoader.exe, and helper launcher processes
//! (this test binary re-run). Nothing here writes to, or starts anything from, a
//! real D2R copy, the real launcher's data or its single-instance identifier.
//!
//! The frontend half of the gate is `src/gate/` (axe-core, contrast, keyboard,
//! progress and remote content in the window). Manual checks on Windows 11 are a
//! checklist in the gate PR.
//!
//! | Clause | Tests |
//! | --- | --- |
//! | Progress, errors and retry | [`progress`]: injected download failure then Try again; cancel while downloading; cancel while applying; bytes reach 100%; `startup_failed` exit code |
//! | Hide, reopen, Quit and autostart | [`real_window`]: on a real WebView2 window, Close hides it (never destroys it), the tray's reopen shows it, and a second instance shows the first; [`window_tray`]: first-close notice once; Quit in each state; `--autostart` wiring |
//! | Game left running on Quit | [`quit_game_running`]: fake-d2rloader outlives Quit; the next launch rediscovers it and refuses writes with `game_running` |
//! | No duplicate application writer | [`single_writer`]: a second instance exits before setup while the first holds the operation lock; [`real_window`]: it shows the first's window |
//! | No remote content native privilege | [`remote_content`]: capabilities, ungranted windows and origins, CSP, news by index, URL refusals |
#![cfg(windows)]

#[path = "../support/mod.rs"]
mod support;

// The slice 3 gate's rig and fake loader, reused as they are.
#[allow(dead_code)]
#[path = "../slice3_gate/fake_loader.rs"]
mod fake_loader;
#[allow(dead_code)]
#[path = "../slice3_gate/rig.rs"]
mod rig;

mod helper;

mod progress;
mod quit_game_running;
mod real_window;
mod remote_content;
mod single_writer;
mod window_tray;
