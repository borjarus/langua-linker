//! Pure domain layer: entities, value types and invariants. No Tauri, SQL or network dependency.
//!
//! Persistence codes are explicit (`db_code`/`from_db_code`) and never derived from enum order,
//! so reordering a variant cannot silently change stored data (plan Phase 7 step 2).

pub mod card;
pub mod content;
pub mod deck;
pub mod ids;
pub mod note;
pub mod review;
pub mod text;
pub mod validation;

pub use ids::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, Guid, MediaId, NoteId, NoteTypeId,
    ReviewLogId, SessionId, TimestampMs, WordLinkId,
};
pub use validation::{ValidationError, ValidationResult};

/// Unknown persisted code for an enum; the database holds a value this build does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} code {code}")]
pub struct UnknownCode {
    pub kind: &'static str,
    pub code: i64,
}

/// Declares an enum with explicit, stable persistence codes and their reverse mapping.
macro_rules! coded_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident = $code:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name { $($(#[$vmeta])* $variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub const fn db_code(self) -> i64 {
                match self { $($name::$variant => $code),+ }
            }

            pub fn from_db_code(code: i64) -> Result<Self, $crate::UnknownCode> {
                match code {
                    $(c if c == $code => Ok($name::$variant),)+
                    _ => Err($crate::UnknownCode { kind: stringify!($name), code }),
                }
            }
        }
    };
}
pub(crate) use coded_enum;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    coded_enum! {
        /// Codes deliberately differ from declaration order.
        enum Sample {
            First = 10,
            Second = 2,
            Third = -1,
        }
    }

    #[test]
    fn db_codes_are_the_declared_ones_not_the_variant_order() {
        assert_eq!(Sample::First.db_code(), 10);
        assert_eq!(Sample::Second.db_code(), 2);
        assert_eq!(Sample::Third.db_code(), -1);
    }

    #[test]
    fn every_variant_round_trips_through_its_code() {
        for variant in Sample::ALL {
            assert_eq!(Sample::from_db_code(variant.db_code()), Ok(*variant));
        }
    }

    #[test]
    fn all_lists_variants_in_declaration_order() {
        assert_eq!(
            Sample::ALL,
            [Sample::First, Sample::Second, Sample::Third].as_slice()
        );
        let codes: HashSet<i64> = Sample::ALL.iter().map(|v| v.db_code()).collect();
        assert_eq!(codes.len(), Sample::ALL.len(), "codes are unique");
    }

    #[test]
    fn unknown_code_names_the_enum_and_the_value() {
        let error = Sample::from_db_code(99).unwrap_err();
        assert_eq!(
            error,
            UnknownCode {
                kind: "Sample",
                code: 99
            }
        );
        assert_eq!(error.to_string(), "unknown Sample code 99");
    }

    #[test]
    fn index_of_a_variant_is_not_mistaken_for_its_code() {
        // Position 1 is `Second`, but code 1 is not assigned to anything.
        assert!(Sample::from_db_code(1).is_err());
        assert!(Sample::from_db_code(0).is_err());
    }

    #[test]
    fn crate_root_reexports_ids_and_validation() {
        let deck = DeckId(1);
        assert_eq!(deck, ids::DeckId(1));
        let error = ValidationError::new("front", "tooLong");
        let result: ValidationResult<()> = Err(error.clone());
        assert_eq!(result, Err(error));
    }
}
