//! The exact file inventory a release expects on disk.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A product-root-relative path using `/` separators, as written in the release
/// manifest. This type does not validate: Catalog (#33) and `install::paths` (#35)
/// reject absolute, escaping, reserved or Windows-conflicting paths before use.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelativePath(pub String);

/// A SHA-256 digest. Serialized as 64 lowercase hex characters; parsing accepts
/// either case. An all-zero digest parses (rejecting placeholders is Catalog's job).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sha256Digest(pub [u8; 32]);

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256Digest({self})")
    }
}

impl FromStr for Sha256Digest {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 64 {
            return Err(format!(
                "SHA-256 must be 64 hex characters, got {}",
                bytes.len()
            ));
        }
        let nibble = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
        let mut out = [0u8; 32];
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            match (nibble(pair[0]), nibble(pair[1])) {
                (Some(hi), Some(lo)) => out[i] = hi << 4 | lo,
                _ => return Err(format!("SHA-256 has a non-hex character: {s:?}")),
            }
        }
        Ok(Self(out))
    }
}

impl Serialize for Sha256Digest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// One file a release places in the product root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedFile {
    pub path: RelativePath,
    pub size: u64,
    pub sha256: Sha256Digest,
}

/// The complete inventory of a release: every file with size and SHA-256, plus the
/// directories that must exist even though they are empty. Extraction (#35) and
/// activation verification (#36) check disk contents against it exactly: no extra,
/// missing or differing entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedInventory {
    pub files: Vec<ExpectedFile>,
    pub empty_dirs: Vec<RelativePath>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn sha256_round_trips_as_lowercase_hex() {
        let d: Sha256Digest = HASH.to_uppercase().parse().unwrap();
        assert_eq!(d.to_string(), HASH);
        assert_eq!(serde_json::to_string(&d).unwrap(), format!("\"{HASH}\""));
    }

    #[test]
    fn sha256_rejects_wrong_length_and_non_hex() {
        assert!(HASH[..63].parse::<Sha256Digest>().is_err());
        assert!(format!("{}zz", &HASH[..62])
            .parse::<Sha256Digest>()
            .is_err());
        assert!(format!("{}é", &HASH[..62]).parse::<Sha256Digest>().is_err());
    }

    #[test]
    fn expected_inventory_wire_shape() {
        let json = format!(
            r#"{{"files":[{{"path":"data/global/excel/armor.txt","size":0,"sha256":"{HASH}"}}],
                "empty_dirs":["data/local/lng"]}}"#
        );
        let inv: ExpectedInventory = serde_json::from_str(&json).unwrap();
        assert_eq!(inv.files[0].path.0, "data/global/excel/armor.txt");
        assert_eq!(inv.empty_dirs, vec![RelativePath("data/local/lng".into())]);
    }
}
