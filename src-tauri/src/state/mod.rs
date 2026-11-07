//! State/diagnostics module: versioned state files with atomic replacement for
//! preferences, cached validated metadata and installed-release records, plus local
//! redacted diagnostics. No database. Transaction journals stay owned by Install.
//!
//! Implemented by #34.
