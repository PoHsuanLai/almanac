//! Closed unit enums whose stable slug is their serde `snake_case` form, declared once so the
//! slug and the serde name cannot differ.

/// Declares a unit enum with the usual derives, `snake_case` serde names, `slug()` and `ALL`.
macro_rules! slug_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $slug:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $($(#[$vdoc])* #[serde(rename = $slug)] $variant),+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The stable slug: the serde form, in files, on the bus and in kind tags.
            pub const fn slug(self) -> &'static str {
                match self {
                    $($name::$variant => $slug),+
                }
            }
        }
    };
}

pub(crate) use slug_enum;
