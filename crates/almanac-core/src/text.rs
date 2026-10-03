//! Validated text ids, byte-array ids with hex serde, and the redacting `UserText`.

use porter_core::CoreError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Declares a validated string id: `parse` once where it enters, serde through `String`. A
/// leading `redacted` makes `Debug` print the length only (user text).
macro_rules! text_id {
    (redacted $(#[$doc:meta])* $name:ident, $what:literal, $ok:expr) => {
        $crate::text::text_id!(@type $(#[$doc])* $name, $what, $ok);

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, concat!(stringify!($name), "(<{} bytes>)"), self.0.len())
            }
        }
    };
    ($(#[$doc:meta])* $name:ident, $what:literal, $ok:expr) => {
        $crate::text::text_id!(@type $(#[$doc])* $name, $what, $ok);

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, concat!(stringify!($name), "({:?})"), self.0)
            }
        }
    };
    (@type $(#[$doc:meta])* $name:ident, $what:literal, $ok:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, ::serde::Serialize, ::serde::Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// The id written as `text`, or why it is not one.
            pub fn parse(text: &str) -> Result<Self, ::porter_core::CoreError> {
                let ok: fn(&str) -> bool = $ok;
                if ok(text) {
                    Ok(Self(text.to_owned()))
                } else {
                    Err(::porter_core::CoreError::MalformedId { what: $what, text: text.to_owned() })
                }
            }

            /// The id's text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ::porter_core::CoreError;
            fn try_from(text: String) -> Result<Self, ::porter_core::CoreError> {
                Self::parse(&text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

pub(crate) use text_id;

/// `[a-z][a-z0-9_]*`.
pub(crate) fn element_ok(element: &str) -> bool {
    let mut bytes = element.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Two or more dot-separated `[a-z][a-z0-9_]*` elements.
pub(crate) fn dotted(text: &str) -> bool {
    let mut count = 0;
    let all = text.split('.').all(|e| {
        count += 1;
        element_ok(e)
    });
    all && count >= 2
}

/// Declares a 32-byte value written as 64 lowercase hex digits.
macro_rules! digest {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub [u8; 32]);

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), $crate::text::hex_of(&self.0))
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(&$crate::text::hex_of(&self.0))
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&$crate::text::hex_of(&self.0))
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <String as ::serde::Deserialize>::deserialize(d)?;
                $crate::text::from_hex::<32>(&text)
                    .map(Self)
                    .ok_or_else(|| ::serde::de::Error::custom("expected 64 lowercase hex digits"))
            }
        }
    };
}

digest!(
    /// A keyed digest of an event body (`blake3` keyed hash; see `eventlog`).
    Digest32
);
digest!(
    /// One link of an event log's hash chain: `blake3` of the canonical header bytes.
    Link32
);
digest!(
    /// A file's content digest, as the app or watcher computed it. The convention is plain
    /// `blake3` of the file's bytes (`*blake3::hash(content).as_bytes()`), unkeyed: it must be
    /// the same for the app that says why a file changed and the watcher that saw it, so the
    /// join can compare them. It is not the keyed body digest of the event log (`Digest32`).
    ContentDigest
);

digest!(
    /// The digest of a forget plan's closure: if it changes between plan and apply, the plan is
    /// stale.
    PlanDigest
);

/// Lowercase hex of `bytes`.
pub fn hex_of(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [DIGITS[usize::from(b >> 4)], DIGITS[usize::from(b & 15)]])
        .map(char::from)
        .collect()
}

/// The `N` bytes written as `2 * N` lowercase hex digits, or `None`.
pub fn from_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digit = |b: u8| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let bytes = text.as_bytes();
    if bytes.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    let (pairs, _) = bytes.as_chunks::<2>();
    for (slot, pair) in out.iter_mut().zip(pairs) {
        *slot = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(out)
}

/// Text a person wrote or saw (titles, search text, diffs). Serialises as a plain string;
/// `Debug` prints the length only, so logs never carry it.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserText(String);

impl UserText {
    /// Wraps `text`.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for UserText {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for UserText {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl fmt::Debug for UserText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UserText(<{} bytes>)", self.0.len())
    }
}

/// A JSON document in its text form: the owner's serde form of an opaque payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct JsonText(String);

impl JsonText {
    /// `text` if it is one JSON value.
    pub fn parse(text: &str) -> Result<Self, CoreError> {
        match serde_json::from_str::<serde::de::IgnoredAny>(text) {
            Ok(_) => Ok(Self(text.to_owned())),
            Err(e) => Err(CoreError::MalformedFrame(format!("not JSON: {e}"))),
        }
    }

    /// The JSON text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for JsonText {
    type Error = CoreError;
    fn try_from(text: String) -> Result<Self, CoreError> {
        Self::parse(&text)
    }
}

impl From<JsonText> for String {
    fn from(json: JsonText) -> String {
        json.0
    }
}
