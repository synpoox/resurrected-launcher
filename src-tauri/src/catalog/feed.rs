//! Release feed, schema 3: product availability and the recommended-release pointer.

use serde::Deserialize;

use crate::contract::{ProductId, Sha256Digest};

use super::check::{self, each};
use super::{CatalogError, ReleaseManifest};

/// Largest release manifest the launcher downloads. The feed is mutable, so its
/// `manifest_size` is capped before anything is fetched.
pub const MAX_MANIFEST_SIZE: u64 = 8 * 1024 * 1024;

/// Where a manifest is and what its bytes must be. Download verifies the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleasePointer {
    pub release_id: String,
    pub manifest_url: String,
    pub manifest_size: u64,
    pub manifest_sha256: Sha256Digest,
}

impl ReleasePointer {
    /// Immutability: a release ID seen before must point at the same manifest bytes.
    pub fn check_unchanged(&self, cached: &ReleasePointer) -> Result<(), CatalogError> {
        if self.release_id == cached.release_id && self != cached {
            return Err(CatalogError::ReleaseChanged {
                release_id: self.release_id.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// Shown as Coming soon; no install action and no artifacts.
    Upcoming,
    /// Installable; the publisher-selected recommended release.
    Available(ReleasePointer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedProduct {
    pub product_id: ProductId,
    pub availability: Availability,
}

impl FeedProduct {
    /// The manifest must be the release this entry recommends.
    pub fn check_manifest(&self, manifest: &ReleaseManifest) -> Result<(), CatalogError> {
        let Availability::Available(pointer) = &self.availability else {
            return Err(CatalogError::ManifestMismatch {
                field: "release_id",
            });
        };
        if manifest.product_id != self.product_id {
            return Err(CatalogError::ManifestMismatch {
                field: "product_id",
            });
        }
        if manifest.release_id != pointer.release_id {
            return Err(CatalogError::ManifestMismatch {
                field: "release_id",
            });
        }
        Ok(())
    }
}

/// A validated release feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub products: Vec<FeedProduct>,
}

impl Feed {
    pub fn parse(bytes: &[u8]) -> Result<Self, CatalogError> {
        check::parse::<RawFeed>(bytes)?.validate()
    }

    pub fn product(&self, id: &ProductId) -> Option<&FeedProduct> {
        self.products.iter().find(|p| &p.product_id == id)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFeed {
    #[allow(dead_code)] // checked by check::parse before this struct is built
    schema_version: u32,
    products: Vec<RawProduct>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawAvailability {
    Upcoming,
    Available,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProduct {
    product_id: String,
    availability: RawAvailability,
    #[serde(default)]
    recommended: Option<RawPointer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPointer {
    release_id: String,
    manifest_url: String,
    manifest_size: Option<u64>,
    manifest_sha256: Option<String>,
}

impl RawFeed {
    fn validate(self) -> Result<Feed, CatalogError> {
        let products = each("products", self.products, |field, p| {
            let product_id = check::product_id(&format!("{field}.product_id"), p.product_id)?;
            let availability = match (p.availability, p.recommended) {
                (RawAvailability::Upcoming, None) => Availability::Upcoming,
                (RawAvailability::Upcoming, Some(_)) => {
                    return Err(CatalogError::UpcomingHasArtifacts { product_id })
                }
                (RawAvailability::Available, None) => {
                    return Err(CatalogError::InvalidField {
                        field: format!("{field}.recommended"),
                        reason: "required for an available product",
                    })
                }
                (RawAvailability::Available, Some(r)) => {
                    let field = format!("{field}.recommended");
                    let size_field = format!("{field}.manifest_size");
                    let manifest_size = check::size(&size_field, r.manifest_size)?;
                    if manifest_size > MAX_MANIFEST_SIZE {
                        return Err(CatalogError::InvalidField {
                            field: size_field,
                            reason: "larger than the 8 MiB manifest limit",
                        });
                    }
                    Availability::Available(ReleasePointer {
                        release_id: check::ident(&format!("{field}.release_id"), r.release_id)?,
                        manifest_url: check::https(
                            &format!("{field}.manifest_url"),
                            r.manifest_url,
                        )?,
                        manifest_size,
                        manifest_sha256: check::digest(
                            &format!("{field}.manifest_sha256"),
                            r.manifest_sha256,
                        )?,
                    })
                }
            };
            Ok(FeedProduct {
                product_id,
                availability,
            })
        })?;
        for (i, p) in products.iter().enumerate() {
            if products[..i].iter().any(|q| q.product_id == p.product_id) {
                return Err(CatalogError::InvalidField {
                    field: format!("products[{i}].product_id"),
                    reason: "duplicate product",
                });
            }
        }
        Ok(Feed { products })
    }
}
