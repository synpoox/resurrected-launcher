//! The exact file inventory a release expects on disk. This is a trust-boundary
//! type: paths and inventories that exist are already valid, so Catalog (#33) and
//! extraction (#35) reuse these checks rather than duplicating them.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why a [`RelativePath`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    Empty,
    /// Contains `\`. Manifests use `/` only.
    Backslash,
    /// Starts with `/` (absolute or UNC-like).
    Absolute,
    /// Contains `:` (drive letter such as `C:` or an alternate data stream).
    Colon,
    /// Empty component (`a//b`, trailing `/`).
    EmptyComponent,
    /// `.` or `..` component.
    DotComponent,
    /// Control character (including NUL) or one of `< > " | ? *`.
    ForbiddenChar,
    /// Component ends with `.` or space, which Windows silently strips.
    TrailingDotOrSpace,
    /// Windows device name (CON, PRN, AUX, NUL, COM0-9, LPT0-9), with or without extension.
    ReservedName,
    /// `~` followed by a digit, which can alias another file's 8.3 short name.
    ShortNameAlias,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for PathError {}

/// A product-root-relative path with `/` separators that is safe on Windows: it
/// cannot be absolute, escape the root, name a device or stream, or be silently
/// rewritten by Windows. Constructed only through [`RelativePath::new`] (or serde,
/// which calls it).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelativePath(String);

const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

fn check_component(c: &str) -> Result<(), PathError> {
    if c.is_empty() {
        return Err(PathError::EmptyComponent);
    }
    if c == "." || c == ".." {
        return Err(PathError::DotComponent);
    }
    if c.chars().any(|ch| ch < ' ' || "<>\"|?*".contains(ch)) {
        return Err(PathError::ForbiddenChar);
    }
    if c.ends_with('.') || c.ends_with(' ') {
        return Err(PathError::TrailingDotOrSpace);
    }
    let stem = c
        .split('.')
        .next()
        .unwrap_or(c)
        .trim_end()
        .to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            matches!(
                n,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    };
    if RESERVED.contains(&stem.as_str()) || numbered("COM") || numbered("LPT") {
        return Err(PathError::ReservedName);
    }
    if c.as_bytes()
        .windows(2)
        .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
    {
        return Err(PathError::ShortNameAlias);
    }
    Ok(())
}

impl RelativePath {
    pub fn new(path: impl Into<String>) -> Result<Self, PathError> {
        let path = path.into();
        if path.is_empty() {
            return Err(PathError::Empty);
        }
        if path.contains('\\') {
            return Err(PathError::Backslash);
        }
        if path.starts_with('/') {
            return Err(PathError::Absolute);
        }
        if path.contains(':') {
            return Err(PathError::Colon);
        }
        path.split('/').try_for_each(check_component)?;
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Key for comparing paths the way Windows does (case-insensitive).
    fn fold(&self) -> String {
        self.0.to_uppercase()
    }
}

impl TryFrom<String> for RelativePath {
    type Error = PathError;
    fn try_from(s: String) -> Result<Self, PathError> {
        Self::new(s)
    }
}

impl From<RelativePath> for String {
    fn from(p: RelativePath) -> String {
        p.0
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A SHA-256 digest. Serialized as 64 lowercase hex characters; parsing accepts
/// either case and rejects the all-zero placeholder.
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
        if out == [0; 32] {
            return Err("SHA-256 is the all-zero placeholder".to_owned());
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
#[serde(deny_unknown_fields)]
pub struct ExpectedFile {
    pub path: RelativePath,
    pub size: u64,
    pub sha256: Sha256Digest,
}

/// Why an [`ExpectedInventory`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryError {
    /// The same path appears twice, possibly differing only in case.
    DuplicatePath(RelativePath),
    /// `inner` lies under `outer`, but `outer` is a file or a required-empty dir.
    PathConflict {
        outer: RelativePath,
        inner: RelativePath,
    },
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePath(p) => write!(f, "duplicate path {p}"),
            Self::PathConflict { outer, inner } => {
                write!(
                    f,
                    "{inner} lies under {outer}, which is a file or empty dir"
                )
            }
        }
    }
}

impl std::error::Error for InventoryError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInventory {
    files: Vec<ExpectedFile>,
    empty_dirs: Vec<RelativePath>,
}

/// The complete inventory of a release: every file with size and SHA-256, plus the
/// directories that must exist even though they are empty. Extraction (#35) and
/// activation verification (#36) check disk contents against it exactly: no extra,
/// missing or differing entries.
///
/// Invariants (checked by [`ExpectedInventory::new`] and serde): no two entries are
/// equal ignoring case, and nothing lies under a file or under a required-empty dir.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawInventory")]
pub struct ExpectedInventory {
    files: Vec<ExpectedFile>,
    empty_dirs: Vec<RelativePath>,
}

impl ExpectedInventory {
    pub fn new(
        files: Vec<ExpectedFile>,
        empty_dirs: Vec<RelativePath>,
    ) -> Result<Self, InventoryError> {
        // Every entry is a leaf: a file or an empty dir. Nothing may sit under a leaf.
        let leaves = files.iter().map(|f| &f.path).chain(&empty_dirs);
        let mut seen: HashMap<String, &RelativePath> = HashMap::new();
        for path in leaves.clone() {
            if seen.insert(path.fold(), path).is_some() {
                return Err(InventoryError::DuplicatePath(path.clone()));
            }
        }
        for inner in leaves {
            let key = inner.fold();
            for (i, _) in key.match_indices('/') {
                if let Some(outer) = seen.get(&key[..i]) {
                    return Err(InventoryError::PathConflict {
                        outer: (*outer).clone(),
                        inner: inner.clone(),
                    });
                }
            }
        }
        Ok(Self { files, empty_dirs })
    }

    pub fn files(&self) -> &[ExpectedFile] {
        &self.files
    }

    pub fn empty_dirs(&self) -> &[RelativePath] {
        &self.empty_dirs
    }
}

impl TryFrom<RawInventory> for ExpectedInventory {
    type Error = InventoryError;
    fn try_from(raw: RawInventory) -> Result<Self, InventoryError> {
        Self::new(raw.files, raw.empty_dirs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn path(s: &str) -> RelativePath {
        RelativePath::new(s).unwrap()
    }

    fn file(p: &str) -> ExpectedFile {
        ExpectedFile {
            path: path(p),
            size: 1,
            sha256: HASH.parse().unwrap(),
        }
    }

    #[test]
    fn sha256_round_trips_as_lowercase_hex() {
        let d: Sha256Digest = HASH.to_uppercase().parse().unwrap();
        assert_eq!(d.to_string(), HASH);
        assert_eq!(serde_json::to_string(&d).unwrap(), format!("\"{HASH}\""));
    }

    #[test]
    fn sha256_rejects_wrong_length_non_hex_and_zero_placeholder() {
        assert!(HASH[..63].parse::<Sha256Digest>().is_err());
        assert!(format!("{}zz", &HASH[..62])
            .parse::<Sha256Digest>()
            .is_err());
        assert!(format!("{}é", &HASH[..62]).parse::<Sha256Digest>().is_err());
        assert!("0".repeat(64).parse::<Sha256Digest>().is_err());
    }

    #[test]
    fn relative_path_accepts_ordinary_release_paths() {
        for ok in [
            "data/global/excel/armor.txt",
            "mod.json",
            ".resurrected-disposable",
            "data/hd/global/ui/a b.sprite",
            "CONSOLE/control.txt",
            "COM10",
            "lpt",
            "ünïcode/ファイル.txt",
        ] {
            assert_eq!(
                RelativePath::new(ok).map(|p| p.0),
                Ok(ok.to_owned()),
                "{ok}"
            );
        }
    }

    #[test]
    fn relative_path_rejects_each_unsafe_class() {
        use PathError::*;
        let cases = [
            ("", Empty),
            ("..\\x", Backslash),
            ("data\\x.txt", Backslash),
            ("/x", Absolute),
            ("//server/share", Absolute),
            ("C:/x", Colon),
            ("C:x", Colon),
            ("a.txt:stream", Colon),
            ("a//b", EmptyComponent),
            ("a/", EmptyComponent),
            ("..", DotComponent),
            ("../x", DotComponent),
            ("a/../../x", DotComponent),
            ("./x", DotComponent),
            ("a\0b", ForbiddenChar),
            ("a\nb", ForbiddenChar),
            ("a?b", ForbiddenChar),
            ("a*", ForbiddenChar),
            ("a<b>", ForbiddenChar),
            ("a\"b", ForbiddenChar),
            ("a|b", ForbiddenChar),
            ("x.", TrailingDotOrSpace),
            ("dir /x", TrailingDotOrSpace),
            ("x ", TrailingDotOrSpace),
            ("CON", ReservedName),
            ("data/aux.txt", ReservedName),
            ("Nul.tar.gz", ReservedName),
            ("com1", ReservedName),
            ("LPT9.log", ReservedName),
            ("COM¹", ReservedName),
            ("CON .txt", ReservedName),
            ("PROGRA~1/x", ShortNameAlias),
        ];
        for (input, expected) in cases {
            assert_eq!(RelativePath::new(input), Err(expected), "{input:?}");
        }
    }

    #[test]
    fn relative_path_deserialization_validates() {
        assert!(serde_json::from_str::<RelativePath>(r#""../evil""#).is_err());
        assert_eq!(
            serde_json::from_str::<RelativePath>(r#""a/b""#).unwrap(),
            path("a/b")
        );
    }

    #[test]
    fn expected_inventory_wire_shape() {
        let json = format!(
            r#"{{"files":[{{"path":"data/global/excel/armor.txt","size":0,"sha256":"{HASH}"}}],
                "empty_dirs":["data/local/lng"]}}"#
        );
        let inv: ExpectedInventory = serde_json::from_str(&json).unwrap();
        assert_eq!(inv.files()[0].path.as_str(), "data/global/excel/armor.txt");
        assert_eq!(inv.empty_dirs(), [path("data/local/lng")]);
        let back = serde_json::to_value(&inv).unwrap();
        assert_eq!(back["empty_dirs"][0], "data/local/lng");
    }

    #[test]
    fn expected_inventory_rejects_unknown_fields_and_bad_paths() {
        let extra = r#"{"files":[],"empty_dirs":[],"scripts":["rm"]}"#;
        assert!(serde_json::from_str::<ExpectedInventory>(extra).is_err());
        let bad = format!(
            r#"{{"files":[{{"path":"C:/x","size":1,"sha256":"{HASH}"}}],"empty_dirs":[]}}"#
        );
        assert!(serde_json::from_str::<ExpectedInventory>(&bad).is_err());
    }

    #[test]
    fn expected_inventory_rejects_duplicates_including_case_only() {
        for (a, b) in [("a/x.txt", "a/x.txt"), ("a/x.txt", "A/X.TXT")] {
            assert_eq!(
                ExpectedInventory::new(vec![file(a), file(b)], vec![]),
                Err(InventoryError::DuplicatePath(path(b)))
            );
        }
        assert_eq!(
            ExpectedInventory::new(vec![file("a/x")], vec![path("A/x")]),
            Err(InventoryError::DuplicatePath(path("A/x")))
        );
    }

    #[test]
    fn expected_inventory_rejects_entries_under_files_or_empty_dirs() {
        let conflict = |outer: &str, inner: &str| InventoryError::PathConflict {
            outer: path(outer),
            inner: path(inner),
        };
        assert_eq!(
            ExpectedInventory::new(vec![file("data"), file("data/x.txt")], vec![]),
            Err(conflict("data", "data/x.txt"))
        );
        assert_eq!(
            ExpectedInventory::new(vec![file("Data/x.txt")], vec![path("data")]),
            Err(conflict("data", "Data/x.txt"))
        );
        assert_eq!(
            ExpectedInventory::new(vec![], vec![path("a"), path("a/b")]),
            Err(conflict("a", "a/b"))
        );
        assert_eq!(
            ExpectedInventory::new(vec![file("a/b/c")], vec![path("a/b")]),
            Err(conflict("a/b", "a/b/c"))
        );
    }

    #[test]
    fn expected_inventory_accepts_siblings_and_shared_prefix_names() {
        let inv = ExpectedInventory::new(
            vec![file("data/x.txt"), file("data2/x.txt"), file("data.txt")],
            vec![path("data/empty"), path("logs")],
        );
        assert!(inv.is_ok(), "{inv:?}");
    }
}
