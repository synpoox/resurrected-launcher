//! Catalog module: read and validate the product catalog, release feed, release
//! manifests and compatibility/ownership data, returning typed product and release
//! descriptions (including [`crate::contract::ExpectedInventory`]).
//!
//! Missing or unsupported remote data never fabricates readiness. Implemented by #33.
