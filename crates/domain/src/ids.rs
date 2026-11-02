//! Identifiers and timestamps. Row ids are SQLite `i64`; they cross IPC as strings (plan §4.1).

use std::fmt;
use std::str::FromStr;

macro_rules! row_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        /// Parses the transport form (decimal string). Never goes through `f64`, so no precision loss.
        impl FromStr for $name {
            type Err = std::num::ParseIntError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map($name)
            }
        }
    )+};
}

row_id!(
    DeckId,
    DeckConfigId,
    NoteTypeId,
    NoteId,
    CardId,
    ReviewLogId,
    AssociationId,
    GrammarTipId,
    WordLinkId,
    MediaId,
    SessionId,
);

/// Globally unique id of a user-owned entity: 32 lowercase hex characters (128 random bits).
/// Stable across export, import and future synchronization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Guid(String);

impl Guid {
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// Accepts stored or bundled GUIDs: 1–64 characters of `[A-Za-z0-9_-]`.
    pub fn parse(value: &str) -> Result<Self, crate::ValidationError> {
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(crate::ValidationError::new("guid", "format"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// UTC instant in epoch milliseconds — the storage form of every timestamp (plan §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimestampMs(pub i64);

impl TimestampMs {
    pub const MS_PER_DAY: i64 = 86_400_000;

    pub fn plus_ms(self, ms: i64) -> Self {
        Self(self.0.saturating_add(ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_beyond_f64_precision_round_trip_through_strings() {
        let id: CardId = "9007199254740993".parse().unwrap();
        assert_eq!(id.0, 9_007_199_254_740_993);
        assert_eq!(id.to_string(), "9007199254740993");
    }

    #[test]
    fn guid_from_bytes_is_lowercase_hex() {
        let guid = Guid::from_bytes([0xab; 16]);
        assert_eq!(guid.as_str().len(), 32);
        assert!(guid.as_str().chars().all(|c| c == 'a' || c == 'b'));
    }

    #[test]
    fn guid_parse_rejects_unsafe_text() {
        assert!(Guid::parse("de-words-a1_001").is_ok());
        for bad in ["", "a b", "x;DROP", &"x".repeat(65)] {
            assert!(Guid::parse(bad).is_err(), "{bad}");
        }
    }

    /// Runs the same checks against every type declared by `row_id!`.
    macro_rules! for_each_row_id {
        ($check:ident) => {
            $check::<DeckId>();
            $check::<DeckConfigId>();
            $check::<NoteTypeId>();
            $check::<NoteId>();
            $check::<CardId>();
            $check::<ReviewLogId>();
            $check::<AssociationId>();
            $check::<GrammarTipId>();
            $check::<WordLinkId>();
            $check::<MediaId>();
            $check::<SessionId>();
        };
    }

    trait RowId:
        FromStr<Err = std::num::ParseIntError> + fmt::Display + Copy + fmt::Debug + Eq
    {
        fn raw(self) -> i64;
        fn new(value: i64) -> Self;
    }

    macro_rules! impl_row_id_probe {
        ($($name:ident),+) => {$(
            impl RowId for $name {
                fn raw(self) -> i64 {
                    self.0
                }
                fn new(value: i64) -> Self {
                    Self(value)
                }
            }
        )+};
    }
    impl_row_id_probe!(
        DeckId,
        DeckConfigId,
        NoteTypeId,
        NoteId,
        CardId,
        ReviewLogId,
        AssociationId,
        GrammarTipId,
        WordLinkId,
        MediaId,
        SessionId
    );

    fn display_and_parse_round_trip<T: RowId>() {
        for value in [0, 1, -7, i64::MAX, i64::MIN] {
            let id = T::new(value);
            assert_eq!(id.to_string(), value.to_string());
            assert_eq!(id.to_string().parse::<T>().unwrap(), id);
            assert_eq!(id.raw(), value);
        }
    }

    fn parse_rejects_non_decimal_text<T: RowId>() {
        for bad in [
            "",
            " 1",
            "1 ",
            "1.0",
            "1e3",
            "0x10",
            "abc",
            "9223372036854775808",
        ] {
            assert!(bad.parse::<T>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_row_id_displays_and_parses_as_a_decimal_string() {
        for_each_row_id!(display_and_parse_round_trip);
    }

    #[test]
    fn every_row_id_rejects_malformed_or_overflowing_text() {
        for_each_row_id!(parse_rejects_non_decimal_text);
    }

    #[test]
    fn row_ids_order_hash_and_copy_by_value() {
        use std::collections::HashSet;

        let (low, high) = (NoteId(1), NoteId(2));
        assert!(low < high);
        let copy = low;
        assert_eq!(copy, low);
        let set: HashSet<NoteId> = [low, copy, high].into();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn row_id_types_are_distinct() {
        // Same raw value, different types: only a compile-time guarantee, so check the raw side.
        assert_eq!(DeckId(5).0, CardId(5).0);
        assert_eq!(format!("{:?}", DeckId(5)), "DeckId(5)");
        assert_eq!(format!("{:?}", CardId(5)), "CardId(5)");
    }
}
