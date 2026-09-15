//! Content-addressed and stable identifiers (PRD §7.16, §34.9).

use std::fmt;

use serde::{Deserialize, Serialize};

/// A blake3 digest. Fingerprints key the artifact cache, the build ID, and
/// every hash in the output manifest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    pub const PREFIX: &'static str = "blake3:";

    pub fn of(bytes: impl AsRef<[u8]>) -> Self {
        Self(*blake3::hash(bytes.as_ref()).as_bytes())
    }

    /// Combines parts unambiguously: each is length-prefixed, so
    /// `["ab", "c"]` and `["a", "bc"]` never collide.
    pub fn of_parts<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut hasher = blake3::Hasher::new();
        for part in parts {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
        Self(*hasher.finalize().as_bytes())
    }

    pub fn to_hex(self) -> String {
        self.0.iter().fold(String::with_capacity(64), |mut out, b| {
            use fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix(Self::PREFIX).unwrap_or(text);
        if hex.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        let (pairs, _) = hex.as_bytes().as_chunks::<2>();
        for (byte, pair) in out.iter_mut().zip(pairs) {
            let text = std::str::from_utf8(pair).ok()?;
            *byte = u8::from_str_radix(text, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::PREFIX, self.to_hex())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({self})")
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a blake3 digest: `{text}`")))
    }
}

impl schemars::JsonSchema for Fingerprint {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Fingerprint".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^blake3:[0-9a-f]{64}$",
        })
    }
}

/// 96 bits of block identity (§7.16).
///
/// Explicit and implicit IDs share one space, separated by a tag byte, so an
/// author-written `{#id}` can never collide with a hash of content.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(pub [u8; 12]);

impl BlockId {
    const TAG_IMPLICIT: u8 = 0;
    const TAG_EXPLICIT: u8 = 1;

    /// `blake3(kind, normalized text, anchor of the nearest preceding heading,
    /// ordinal among identical siblings)` truncated to 96 bits.
    ///
    /// `text` is expected to be normalized already (whitespace runs collapsed,
    /// expansion-generated values removed) so that a fact value changing does
    /// not change the ID of the sentence containing it.
    pub fn implicit(kind: &str, text: &str, heading_anchor: &str, ordinal: u32) -> Self {
        Self::truncate(Fingerprint::of_parts([
            &[Self::TAG_IMPLICIT][..],
            kind.as_bytes(),
            text.as_bytes(),
            heading_anchor.as_bytes(),
            &ordinal.to_le_bytes()[..],
        ]))
    }

    /// The ID an author attached with `{#id}` or `<!-- #id -->`.
    pub fn explicit(id: &str) -> Self {
        Self::truncate(Fingerprint::of_parts([
            &[Self::TAG_EXPLICIT][..],
            id.as_bytes(),
        ]))
    }

    pub fn to_hex(self) -> String {
        self.0.iter().fold(String::with_capacity(24), |mut out, b| {
            use fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 24 {
            return None;
        }
        let mut out = [0u8; 12];
        let (pairs, _) = text.as_bytes().as_chunks::<2>();
        for (byte, pair) in out.iter_mut().zip(pairs) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(Self(out))
    }

    fn truncate(fingerprint: Fingerprint) -> Self {
        let mut out = [0u8; 12];
        out.copy_from_slice(&fingerprint.0[..12]);
        Self(out)
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockId({self})")
    }
}

impl Serialize for BlockId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for BlockId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'_, str>>::deserialize(d)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a block id: `{text}`")))
    }
}

impl schemars::JsonSchema for BlockId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "BlockId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^[0-9a-f]{24}$" })
    }
}

/// Declares a ULID newtype with a string JSON form.
macro_rules! ulid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub ulid::Ulid);

        impl $name {
            pub fn parse(text: &str) -> Option<Self> {
                ulid::Ulid::from_string(text).ok().map($name)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0.to_string())
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'_, str>>::deserialize(d)?;
                Self::parse(&text).ok_or_else(|| {
                    serde::de::Error::custom(format!("not a ULID: `{text}`"))
                })
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "pattern": "^[0-9A-HJKMNP-TV-Z]{26}$" })
            }
        }
    };
}

ulid_id!(
    /// Page identity from the `id` front matter key. Survives `git mv`, editor
    /// moves, and slug changes (§7.16).
    PageId
);
ulid_id!(OrgId);
ulid_id!(ProjectId);
ulid_id!(JobId);

/// Declares a validated string newtype with a transparent JSON form.
macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash,
            Serialize, Deserialize, schemars::JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// A site-relative route with a leading slash and no trailing slash, such
    /// as `/getting-started/install`.
    Route
);
string_id!(
    /// A BCP 47 language tag such as `en` or `pt-BR`.
    Locale
);
string_id!(
    /// A documentation version label such as `v2`, not a semver of Liyasa.
    Version
);
string_id!(FactId);
string_id!(
    /// `<page route>#<block id>#<n>` (§34.9).
    CheckId
);
string_id!(ChunkId);
string_id!(IndexId);
string_id!(TokenId);

/// A build is named by the fingerprint of everything that went into it
/// (§6.6.2).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct BuildId(pub Fingerprint);

impl fmt::Display for BuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
