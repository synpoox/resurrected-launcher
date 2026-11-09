//! D2RLoader guidance (#59). The launcher never downloads, bundles or runs the
//! loader installer: when the loader is missing, unsupported or modified it points the
//! player to the official download page, and "Check again" reassesses readiness.

use crate::catalog::ReleaseManifest;
use crate::contract::{LoaderGuidance, Readiness, LOADER_DOWNLOAD_PAGE};

/// Guidance for `readiness`, or `None` unless it is `loader_missing`,
/// `loader_unsupported` or `loader_modified`. `manifest` is the one the readiness was assessed against
/// (installed, else recommended); `allowed` is its `compatibility.loader_versions`,
/// `found` the unsupported or modified file's version (`None` when missing), and `download_page`
/// [`crate::contract::LOADER_DOWNLOAD_PAGE`].
pub fn guidance(readiness: &Readiness, manifest: &ReleaseManifest) -> Option<LoaderGuidance> {
    let found = match readiness {
        Readiness::LoaderMissing => None,
        Readiness::LoaderUnsupported { found, .. } => Some(found.clone()),
        Readiness::LoaderModified { version, .. } => Some(version.clone()),
        _ => return None,
    };
    Some(LoaderGuidance {
        found,
        allowed: manifest.compatibility.loader_versions.clone(),
        download_page: LOADER_DOWNLOAD_PAGE.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ReleaseManifest {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/catalog/valid/release-manifest.json");
        let mut m = ReleaseManifest::parse(&std::fs::read(path).unwrap()).unwrap();
        m.compatibility.loader_versions = vec!["1.3.1-beta".into(), "1.4.0".into()];
        m
    }

    #[test]
    fn missing_loader_has_no_found_version() {
        let g = guidance(&Readiness::LoaderMissing, &manifest()).unwrap();
        assert_eq!(g.found, None);
        assert_eq!(g.allowed, ["1.3.1-beta", "1.4.0"]);
        assert_eq!(g.download_page, LOADER_DOWNLOAD_PAGE);
    }

    #[test]
    fn unsupported_loader_reports_found_and_manifest_allowed() {
        let r = Readiness::LoaderUnsupported {
            found: "0.9".into(),
            // The readiness's own list is ignored: the manifest is the source.
            allowed: vec!["stale".into()],
        };
        let g = guidance(&r, &manifest()).unwrap();
        assert_eq!(g.found.as_deref(), Some("0.9"));
        assert_eq!(g.allowed, ["1.3.1-beta", "1.4.0"]);
        assert_eq!(g.download_page, LOADER_DOWNLOAD_PAGE);
    }

    #[test]
    fn modified_loader_reports_its_allowed_version() {
        let r = Readiness::LoaderModified {
            file: "D2RLoader.exe".into(),
            version: "1.3.1-beta".into(),
        };
        let g = guidance(&r, &manifest()).unwrap();
        assert_eq!(g.found.as_deref(), Some("1.3.1-beta"));
        assert_eq!(g.allowed, ["1.3.1-beta", "1.4.0"]);
        assert_eq!(g.download_page, LOADER_DOWNLOAD_PAGE);
    }

    #[test]
    fn every_other_readiness_has_no_guidance() {
        let others = [
            Readiness::D2rMissing,
            Readiness::Running { since: 1 },
            Readiness::RecoveryRequired,
            Readiness::BuildUnsupported { found: "x".into() },
            Readiness::NotInstalled,
            Readiness::UpdateAvailable,
            Readiness::Ready,
        ];
        for r in others {
            assert_eq!(guidance(&r, &manifest()), None, "{r:?}");
        }
    }
}
