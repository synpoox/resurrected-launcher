//! Handoff to the pinned official D2RLoader installer (#59). The installer is
//! downloaded into the launcher's local data dir, never the D2R folder, and is
//! never rehosted, bundled or elevated by the launcher.
#![allow(unused_variables, dead_code)] // shortcut: frozen stubs (#55); remove when #59 implements them.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::GameHost;
use crate::catalog::{LoaderPin, ReleaseManifest};
use crate::contract::{GameLocation, LauncherError, LoaderSetupOffer};
use crate::install::download::Downloader;

/// An installer file whose size and SHA-256 matched its [`LoaderPin`] when
/// [`download`] returned it.
#[derive(Debug)]
pub struct VerifiedInstaller {
    path: PathBuf,
}

/// The offer shown before consent, from `pin` (`signed` disclosed as is,
/// `source_url` = `pin.url`). `token` is derived deterministically from every
/// field of `pin`, so [`run`] can check it without stored state.
pub fn offer(pin: &LoaderPin) -> LoaderSetupOffer {
    todo!("#59")
}

/// Downloads `pin.url` with `downloader` (HTTPS only) to a file under `data_dir`
/// and verifies its size and SHA-256. On a mismatch the file is deleted.
///
/// Errors: [`crate::contract::ErrorCode::DownloadFailed`],
/// [`crate::contract::ErrorCode::VerificationFailed`],
/// [`crate::contract::ErrorCode::Cancelled`], or local I/O codes.
pub fn download(
    pin: &LoaderPin,
    downloader: &Downloader,
    data_dir: &Path,
    cancel: &AtomicBool,
) -> Result<VerifiedInstaller, LauncherError> {
    todo!("#59")
}

/// Runs the verified installer through [`GameHost::run_installer`] and returns
/// after it exits; the caller then reassesses readiness. Refuses, in order:
///
/// 1. `consent_token` differs from `offer(pin).token`:
///    [`crate::contract::ErrorCode::LoaderSetupConsentRequired`].
/// 2. Anything [`super::process::running_under`] `location`:
///    [`crate::contract::ErrorCode::GameRunning`].
/// 3. [`super::readiness::loader_change_safe`] for `pin.installs_loader_version`
///    and `managed` fails: [`crate::contract::ErrorCode::LoaderChangeUnsafe`].
/// 4. The installer file no longer matches `pin`:
///    [`crate::contract::ErrorCode::VerificationFailed`].
///
/// A failed start, declined UAC prompt or non-zero exit code is
/// [`crate::contract::ErrorCode::LoaderSetupFailed`].
pub fn run(
    pin: &LoaderPin,
    consent_token: &str,
    installer: &VerifiedInstaller,
    location: &GameLocation,
    managed: &[&ReleaseManifest],
    host: &dyn GameHost,
) -> Result<(), LauncherError> {
    todo!("#59")
}

/// See [`GameHost::run_installer`] (`ShellExecuteExW` "open", no arguments).
pub(super) fn system_run_installer(exe: &Path) -> io::Result<u32> {
    todo!("#59")
}
