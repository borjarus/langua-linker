//! Local moderation of what is sent to and accepted from the provider (plan Phase 13 step 8).
//!
//! A short term list (PL/EN/DE/ES slurs, sexual and abusive words), compared on folded words:
//! lowercase, diacritics removed, `ß` as `ss`. Input that hits the list is never sent; an answer
//! that hits it is retried within the shared attempt limit and is never stored as visible.
//! Provider-side moderation (a refusal) counts the same way. The list is deliberately small and
//! conservative: words with an everyday meaning (Polish "pedał" = pedal) are left out. It is a
//! safety net, not a classifier — ADR-18 records the limits.

use langualinker_domain::text;

/// Whole folded words that block.
const TERMS: &[&str] = &[
    // English
    "fuck",
    "fucking",
    "fucker",
    "motherfucker",
    "shit",
    "cunt",
    "nigger",
    "nigga",
    "faggot",
    "whore",
    "slut",
    "rape",
    "rapist",
    "porn",
    "porno",
    "pedophile",
    "paedophile",
    "kike",
    "retard",
    "dildo",
    // Polish (folded)
    "kurwa",
    "kurwy",
    "chuj",
    "chuja",
    "huj",
    "pizda",
    "pizdy",
    "cipa",
    "dziwka",
    "ciota",
    "pedofil",
    "jebac",
    "jebany",
    "jebana",
    "zajebac",
    "skurwysyn",
    // German (folded)
    "fotze",
    "wichser",
    "hure",
    "schlampe",
    "neger",
    "schwuchtel",
    "ficken",
    "fick",
    "arschloch",
    "kinderporno",
    // Spanish (folded)
    "puta",
    "puto",
    "mierda",
    "cabron",
    "maricon",
    "gilipollas",
    "joder",
    "follar",
    "pendejo",
];

/// Folded word stems that block wherever a word starts with them.
const STEMS: &[&str] = &[
    "kurw", "pierdol", "jeban", "wypierd", "spierd", "fuck", "porn",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    Blocked,
}

/// Checks one or more texts; any hit blocks.
pub fn check(texts: &[&str]) -> Verdict {
    let hit = texts.iter().any(|value| {
        text::fold(value)
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .any(|word| TERMS.contains(&word) || STEMS.iter().any(|stem| word.starts_with(stem)))
    });
    if hit {
        Verdict::Blocked
    } else {
        Verdict::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_vocabulary_passes() {
        for text in [
            "der Hund",
            "Pedał roweru skrzypi",
            "Ich habe Hunger",
            "Die Bank steht im Park",
            "Shitake? no: shiitake mushrooms",
            "skurcz",
        ] {
            assert_eq!(check(&[text]), Verdict::Allowed, "{text}");
        }
    }

    #[test]
    fn listed_words_block_after_folding() {
        for text in [
            "What the FUCK",
            "Ty kurwo!",
            "JEBAĆ to",
            "du Arschloch",
            "¡Mierda!",
        ] {
            assert_eq!(check(&[text]), Verdict::Blocked, "{text}");
        }
    }

    #[test]
    fn any_text_of_several_blocks() {
        assert_eq!(check(&["der Hund", "pies", "kurwa"]), Verdict::Blocked);
        assert_eq!(check(&[]), Verdict::Allowed);
    }
}
