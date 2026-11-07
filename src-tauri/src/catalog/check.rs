//! Field checks shared by the feed and manifest readers. Path and hash validity
//! come from [`crate::contract`]; these only attach the field name and rejection class.

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::contract::{ProductId, RelativePath, Sha256Digest};

use super::{CatalogError, SCHEMA_VERSION};

/// Key fragments that mean "run something". Unknown keys are rejected anyway; this
/// gives commands and scripts their own error. `execute` does not match `executable`.
const EXECUTABLE_KEYS: [&str; 7] = [
    "command", "cmd", "script", "execute", "shell", "hook", "argument",
];

/// JSON -> schema check -> declarative check -> typed raw struct, in that order, so
/// a newer schema always reports "launcher update required" whatever its shape.
pub(super) fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, CatalogError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| CatalogError::Malformed(e.to_string()))?;
    // Schema 2 (the current packager output) spells it `schemaVersion`.
    let found = value
        .get("schema_version")
        .or_else(|| value.get("schemaVersion"))
        .and_then(Value::as_u64)
        .ok_or_else(|| CatalogError::Malformed("missing schema_version".to_owned()))?;
    if found != u64::from(SCHEMA_VERSION) {
        return Err(CatalogError::UnsupportedSchema { found });
    }
    if let Some(key) = executable_key(&value) {
        return Err(CatalogError::ExecutableMetadata { key });
    }
    serde_json::from_value(value).map_err(|e| CatalogError::Malformed(e.to_string()))
}

fn executable_key(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => map.iter().find_map(|(k, v)| {
            let lower = k.to_ascii_lowercase();
            if EXECUTABLE_KEYS.iter().any(|bad| lower.contains(bad)) {
                Some(k.clone())
            } else {
                executable_key(v)
            }
        }),
        Value::Array(items) => items.iter().find_map(executable_key),
        _ => None,
    }
}

pub(super) fn digest(field: &str, v: Option<String>) -> Result<Sha256Digest, CatalogError> {
    let placeholder = || CatalogError::PlaceholderVerification {
        field: field.to_owned(),
    };
    let s = v.ok_or_else(placeholder)?;
    if s.bytes().all(|b| b == b'0') {
        return Err(placeholder());
    }
    s.parse().map_err(|_: String| CatalogError::InvalidField {
        field: field.to_owned(),
        reason: "must be 64 hex characters",
    })
}

/// A byte size that must be known and non-zero (archives, manifests, executables).
pub(super) fn size(field: &str, v: Option<u64>) -> Result<u64, CatalogError> {
    v.filter(|&n| n > 0)
        .ok_or_else(|| CatalogError::PlaceholderVerification {
            field: field.to_owned(),
        })
}

pub(super) fn https(field: &str, url: String) -> Result<String, CatalogError> {
    let host = url
        .strip_prefix("https://")
        .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or(""));
    // `@` would let `https://github.com@evil.example/` pass as a github.com URL.
    match host {
        Some(h) if !h.is_empty() && !h.contains('@') && !url.chars().any(char::is_whitespace) => {
            Ok(url)
        }
        _ => Err(CatalogError::NotHttps {
            field: field.to_owned(),
        }),
    }
}

pub(super) fn path(field: &str, p: String) -> Result<RelativePath, CatalogError> {
    RelativePath::new(p).map_err(|error| CatalogError::UnsafePath {
        field: field.to_owned(),
        error,
    })
}

/// Product and release IDs: short, lowercase, filename-safe, never a display name.
pub(super) fn ident(field: &str, s: String) -> Result<String, CatalogError> {
    let ok = (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        && s.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(s)
    } else {
        Err(CatalogError::InvalidField {
            field: field.to_owned(),
            reason: "must be 1-64 of a-z 0-9 . _ - starting with a letter or digit",
        })
    }
}

pub(super) fn product_id(field: &str, s: String) -> Result<ProductId, CatalogError> {
    ident(field, s).map(ProductId)
}

pub(super) fn text(field: &str, s: String) -> Result<String, CatalogError> {
    if s.trim().is_empty() {
        Err(CatalogError::InvalidField {
            field: field.to_owned(),
            reason: "must not be empty",
        })
    } else {
        Ok(s)
    }
}

/// `MAJOR.MINOR.PATCH`, digits only.
pub(super) fn version(field: &str, s: &str) -> Result<[u64; 3], CatalogError> {
    let bad = || CatalogError::InvalidField {
        field: field.to_owned(),
        reason: "must be MAJOR.MINOR.PATCH",
    };
    let parts: Vec<&str> = s.split('.').collect();
    let mut out = [0; 3];
    if parts.len() != 3 {
        return Err(bad());
    }
    for (slot, part) in out.iter_mut().zip(parts) {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        *slot = part.parse().map_err(|_| bad())?;
    }
    Ok(out)
}

/// `YYYY-MM-DD`.
pub(super) fn date(field: &str, s: String) -> Result<String, CatalogError> {
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| {
        b.get(r.clone())
            .filter(|d| d.iter().all(u8::is_ascii_digit))
            .and_then(|_| s[r].parse::<u32>().ok())
    };
    let ok = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && num(0..4).is_some()
        && num(5..7).is_some_and(|m| (1..=12).contains(&m))
        && num(8..10).is_some_and(|d| (1..=31).contains(&d));
    if ok {
        Ok(s)
    } else {
        Err(CatalogError::InvalidField {
            field: field.to_owned(),
            reason: "must be YYYY-MM-DD",
        })
    }
}

/// Validates each item of a list, naming it `field[i]`.
pub(super) fn each<R, T>(
    field: &str,
    items: Vec<R>,
    mut f: impl FnMut(&str, R) -> Result<T, CatalogError>,
) -> Result<Vec<T>, CatalogError> {
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| f(&format!("{field}[{i}]"), item))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_rejects_other_schemes_userinfo_and_empty_hosts() {
        for bad in [
            "http://github.com/x",
            "HTTPS://github.com/x",
            "https://",
            "https:///x",
            "https://github.com@evil.example/x",
            "https://github.com/a b",
            "file:///C:/x",
            "ftp://x",
        ] {
            assert_eq!(
                https("u", bad.to_owned()),
                Err(CatalogError::NotHttps {
                    field: "u".to_owned()
                }),
                "{bad}"
            );
        }
        assert!(https("u", "https://github.com/x?y#z".to_owned()).is_ok());
    }

    #[test]
    fn digest_treats_missing_empty_and_zero_as_placeholders() {
        let placeholder = Err(CatalogError::PlaceholderVerification {
            field: "h".to_owned(),
        });
        assert_eq!(digest("h", None), placeholder);
        assert_eq!(digest("h", Some(String::new())), placeholder);
        assert_eq!(digest("h", Some("0".repeat(64))), placeholder);
        assert!(matches!(
            digest("h", Some("abc".to_owned())),
            Err(CatalogError::InvalidField { .. })
        ));
    }

    #[test]
    fn version_and_date_formats() {
        assert_eq!(version("v", "1.20.3"), Ok([1, 20, 3]));
        for bad in ["1.2", "1.2.3.4", "1.+2.3", "a.b.c", "1..3", ""] {
            assert!(version("v", bad).is_err(), "{bad}");
        }
        assert!(date("d", "2026-11-01".to_owned()).is_ok());
        for bad in [
            "2026-13-01",
            "2026-11-1",
            "26-11-01",
            "2026/11/01",
            "2026-11-00",
        ] {
            assert!(date("d", bad.to_owned()).is_err(), "{bad}");
        }
    }

    #[test]
    fn ident_rejects_display_names() {
        assert!(ident("id", "pd2r-0.4.0".to_owned()).is_ok());
        for bad in ["", "PD2R", "Project Diablo 2", "-x", "a/b", &"a".repeat(65)] {
            assert!(ident("id", bad.to_owned()).is_err(), "{bad}");
        }
    }
}
