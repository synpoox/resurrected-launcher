//! D2RLoader guidance (#59). The launcher never downloads, bundles or runs the
//! loader installer: when the loader is missing or unsupported it points the
//! player to the official download page, and "Check again" reassesses readiness.
#![allow(unused_variables)] // shortcut: frozen stub (#55); remove when #59 implements it.

use crate::catalog::ReleaseManifest;
use crate::contract::{LoaderGuidance, Readiness};

/// Guidance for `readiness`, or `None` unless it is `loader_missing` or
/// `loader_unsupported`. `manifest` is the one the readiness was assessed against
/// (installed, else recommended); `allowed` is its `compatibility.loader_versions`,
/// `found` the unsupported version (`None` when missing), and `download_page`
/// [`crate::contract::LOADER_DOWNLOAD_PAGE`].
pub fn guidance(readiness: &Readiness, manifest: &ReleaseManifest) -> Option<LoaderGuidance> {
    todo!("#59")
}
