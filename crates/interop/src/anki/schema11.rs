//! Legacy `.apkg` writer: a ZIP with `collection.anki2` (SQLite, Anki schema 11) and an empty
//! `media` map (plan Phase 14 step 3).
//!
//! - **Notes.** The note GUID is kept (Anki matches re-imports by it); fields are joined with
//!   U+001F and HTML-escaped (the collection stores plain text; a line break becomes `<br>`);
//!   `sfld` is the front, `csum` the first 32 bits of its SHA-1. Tags are space-separated, so a tag
//!   holding whitespace is written with `_` and reported.
//! - **Note types.** One per built-in kind, with fixed ids so a second export updates the same
//!   Anki note type: forward `{{Front}}` → `{{Back}}`, reverse `{{Back}}` → `{{Front}}`.
//! - **Cards.** Scheduling through the [`codec`](super::codec): v2 numbers, `ivl`, legacy
//!   `factor`, reps/lapses, flags, FSRS memory as card data.
//! - **Review log.** One `revlog` row per review that was not undone: rating, interval before and
//!   after (days, or negative seconds while learning), legacy ease, time (capped at 60 s as
//!   Anki does), kind. The FSRS snapshot of each review has no column and is reported.
//! - **Ids.** Anki ids are epoch milliseconds: creation times, moved forward on a collision.

use std::collections::{BTreeMap, HashSet};
use std::io::{Seek, Write};
use std::path::Path;

use langualinker_domain::card::{Card, CardState};
use langualinker_domain::deck::Deck;
use langualinker_domain::note::{Note, NoteTypeKind};
use langualinker_domain::review::{ReviewKind, ReviewLog};
use langualinker_domain::{CardId, DeckId, NoteId, TimestampMs};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use super::codec::{AnkiClock, encode};
use super::{Loss, Losses};

pub const COLLECTION_ENTRY: &str = "collection.anki2";
pub const MEDIA_ENTRY: &str = "media";
/// Anki caps a review's answer time at 60 s.
const MAX_ANSWER_MS: i64 = 60_000;
const DEFAULT_DECK_ID: i64 = 1;
const DEFAULT_CONF_ID: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE col (
    id integer primary key, crt integer not null, mod integer not null, scm integer not null,
    ver integer not null, dty integer not null, usn integer not null, ls integer not null,
    conf text not null, models text not null, decks text not null, dconf text not null,
    tags text not null
);
CREATE TABLE notes (
    id integer primary key, guid text not null, mid integer not null, mod integer not null,
    usn integer not null, tags text not null, flds text not null, sfld integer not null,
    csum integer not null, flags integer not null, data text not null
);
CREATE TABLE cards (
    id integer primary key, nid integer not null, did integer not null, ord integer not null,
    mod integer not null, usn integer not null, type integer not null, queue integer not null,
    due integer not null, ivl integer not null, factor integer not null, reps integer not null,
    lapses integer not null, left integer not null, odue integer not null, odid integer not null,
    flags integer not null, data text not null
);
CREATE TABLE revlog (
    id integer primary key, cid integer not null, usn integer not null, ease integer not null,
    ivl integer not null, lastIvl integer not null, factor integer not null, time integer not null,
    type integer not null
);
CREATE TABLE graves (usn integer not null, oid integer not null, type integer not null);
CREATE INDEX ix_notes_usn on notes (usn);
CREATE INDEX ix_cards_usn on cards (usn);
CREATE INDEX ix_revlog_usn on revlog (usn);
CREATE INDEX ix_cards_nid on cards (nid);
CREATE INDEX ix_cards_sched on cards (did, queue, due);
CREATE INDEX ix_revlog_cid on revlog (cid);
CREATE INDEX ix_notes_csum on notes (csum);
";

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("inconsistent export: {0}")]
    Inconsistent(&'static str),
}

/// What one export contains. `decks[0]` is the exported root; the others are its subdecks.
pub struct Export<'a> {
    pub decks: &'a [Deck],
    pub notes: &'a [(Note, NoteTypeKind)],
    /// Live, not archived cards of the notes.
    pub cards: &'a [Card],
    /// Reviews of the cards (undone ones are skipped).
    pub reviews: &'a [ReviewLog],
    pub clock: AnkiClock,
    pub now: TimestampMs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    pub notes: u32,
    pub cards: u32,
    pub reviews: u32,
    pub losses: Losses,
}

/// Fixed Anki note type id of a built-in kind.
pub fn model_id(kind: NoteTypeKind) -> i64 {
    1_735_689_600_000 + kind.db_code()
}

fn model_name(kind: NoteTypeKind) -> &'static str {
    match kind {
        NoteTypeKind::Basic => "LanguaLinker Basic",
        NoteTypeKind::BasicAndReversed => "LanguaLinker Basic (and reversed card)",
        NoteTypeKind::Sentence => "LanguaLinker Sentence",
        NoteTypeKind::SentenceAndReversed => "LanguaLinker Sentence (and reversed card)",
    }
}

/// HTML of a plain-text field.
pub fn field_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Anki's `csum`: the first 8 hex digits of the SHA-1 of the sort field, as an integer.
pub fn checksum(sort_field: &str) -> i64 {
    let digest = Sha1::digest(sort_field.as_bytes());
    i64::from(u32::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3],
    ]))
}

/// Allocates unique millisecond ids: the wanted one, or after the highest taken on a collision
/// (constant time: many notes share one creation instant after an import).
#[derive(Default)]
struct Ids {
    taken: HashSet<i64>,
    highest: i64,
}

impl Ids {
    fn take(&mut self, wanted: i64) -> i64 {
        let mut id = wanted.max(1);
        if self.taken.contains(&id) {
            id = self.highest + 1;
        }
        self.taken.insert(id);
        self.highest = self.highest.max(id);
        id
    }
}

fn model_json(kind: NoteTypeKind, now_secs: i64) -> Value {
    let field = |name: &str, ord: u32| {
        json!({
            "name": name, "ord": ord, "sticky": false, "rtl": false, "font": "Arial", "size": 20,
            "media": [], "description": ""
        })
    };
    let template = |name: &str, ord: u32, question: &str, answer: &str| {
        json!({
            "name": name, "ord": ord, "qfmt": format!("{{{{{question}}}}}"),
            "afmt": format!("{{{{FrontSide}}}}\n\n<hr id=answer>\n\n{{{{{answer}}}}}"),
            "bqfmt": "", "bafmt": "", "did": null, "bfont": "", "bsize": 0
        })
    };
    let mut templates = vec![template("Card 1", 0, "Front", "Back")];
    let mut req = vec![json!([0, "any", [0]])];
    if kind.templates().len() > 1 {
        templates.push(template("Card 2", 1, "Back", "Front"));
        req.push(json!([1, "any", [1]]));
    }
    json!({
        "id": model_id(kind),
        "name": model_name(kind),
        "type": 0,
        "mod": now_secs,
        "usn": 0,
        "sortf": 0,
        "did": DEFAULT_DECK_ID,
        "tmpls": templates,
        "flds": [field("Front", 0), field("Back", 1)],
        "css": ".card {\n font-family: arial;\n font-size: 20px;\n text-align: center;\n color: black;\n background-color: white;\n}\n",
        "latexPre": "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n",
        "latexPost": "\\end{document}",
        "latexsvg": false,
        "req": req,
        "tags": [],
        "vers": []
    })
}

fn deck_json(id: i64, name: &str, description: &str, now_secs: i64) -> Value {
    json!({
        "id": id, "name": name, "desc": description, "mod": now_secs, "usn": 0,
        "collapsed": false, "browserCollapsed": false, "dyn": 0, "conf": DEFAULT_CONF_ID,
        "extendNew": 0, "extendRev": 0,
        "newToday": [0, 0], "revToday": [0, 0], "lrnToday": [0, 0], "timeToday": [0, 0]
    })
}

fn dconf_json() -> Value {
    json!({ DEFAULT_CONF_ID.to_string(): {
        "id": DEFAULT_CONF_ID, "name": "Default", "mod": 0, "usn": 0, "maxTaken": 60,
        "autoplay": true, "timer": 0, "replayq": true, "dyn": false,
        "new": { "bury": false, "delays": [1.0, 10.0], "initialFactor": 2500, "ints": [1, 4, 0],
                 "order": 1, "perDay": 20 },
        "rev": { "bury": false, "ease4": 1.3, "ivlFct": 1.0, "maxIvl": 36500, "perDay": 200,
                 "hardFactor": 1.2 },
        "lapse": { "delays": [10.0], "leechAction": 1, "leechFails": 8, "minInt": 1, "mult": 0.0 }
    }})
}

/// Full Anki deck names (`Parent::Child`), relative to `decks[0]`.
fn deck_paths(decks: &[Deck]) -> Result<BTreeMap<DeckId, String>, WriteError> {
    let root = decks.first().ok_or(WriteError::Inconsistent("no deck"))?;
    let mut paths = BTreeMap::new();
    paths.insert(root.id, root.name.replace("::", ":"));
    // Parents come before children in any order of the list after a few passes.
    for _ in 0..decks.len() {
        for deck in &decks[1..] {
            if paths.contains_key(&deck.id) {
                continue;
            }
            if let Some(parent) = deck.parent_id.and_then(|parent| paths.get(&parent)) {
                let path = format!("{parent}::{}", deck.name.replace("::", ":"));
                paths.insert(deck.id, path);
            }
        }
    }
    if paths.len() != decks.len() {
        return Err(WriteError::Inconsistent("deck outside the exported tree"));
    }
    Ok(paths)
}

/// Writes the collection into `path` (a new SQLite file).
pub fn write_collection(export: &Export<'_>, path: &Path) -> Result<Written, WriteError> {
    let mut written = Written::default();
    let now_secs = export.now.0.div_euclid(1000);
    let clock = &export.clock;
    let (rollover, rounded) = clock.rollover_hour();
    if rounded {
        written.losses.add(Loss::CutoffRounded, 1);
    }

    let mut conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    let tx = conn.transaction()?;

    // Decks.
    let paths = deck_paths(export.decks)?;
    let mut ids = Ids::default();
    ids.take(DEFAULT_DECK_ID);
    let mut deck_ids: BTreeMap<DeckId, i64> = BTreeMap::new();
    let mut decks_json = serde_json::Map::new();
    decks_json.insert(
        DEFAULT_DECK_ID.to_string(),
        deck_json(DEFAULT_DECK_ID, "Default", "", now_secs),
    );
    for deck in export.decks {
        let id = ids.take(deck.created_at.0);
        deck_ids.insert(deck.id, id);
        decks_json.insert(
            id.to_string(),
            deck_json(
                id,
                &paths[&deck.id],
                &field_html(&deck.description),
                now_secs,
            ),
        );
    }

    // Note types actually used.
    let mut models = serde_json::Map::new();
    for (_, kind) in export.notes {
        models
            .entry(model_id(*kind).to_string())
            .or_insert_with(|| model_json(*kind, now_secs));
    }
    let current_model = export
        .notes
        .first()
        .map_or(model_id(NoteTypeKind::Basic), |(_, kind)| model_id(*kind));

    // Notes.
    let mut note_ids: BTreeMap<NoteId, i64> = BTreeMap::new();
    {
        let mut insert = tx.prepare(
            "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, 0, '')",
        )?;
        let mut note_id_set = Ids::default();
        for (note, kind) in export.notes {
            let id = note_id_set.take(note.created_at.0);
            note_ids.insert(note.id, id);
            let mut tags = Vec::with_capacity(note.tags.len());
            for tag in &note.tags {
                if tag.chars().any(char::is_whitespace) {
                    written.losses.add(Loss::TagWhitespace, 1);
                    tags.push(tag.split_whitespace().collect::<Vec<_>>().join("_"));
                } else {
                    tags.push(tag.clone());
                }
            }
            let tags = if tags.is_empty() {
                String::new()
            } else {
                format!(" {} ", tags.join(" "))
            };
            let front = field_html(&note.fields.front);
            let back = field_html(&note.fields.back);
            insert.execute(params![
                id,
                note.guid.as_str(),
                model_id(*kind),
                note.updated_at.0.div_euclid(1000),
                tags,
                format!("{front}\u{1f}{back}"),
                note.fields.front,
                checksum(&note.fields.front),
            ])?;
            written.notes += 1;
        }
    }

    // Cards.
    let mut card_ids: BTreeMap<CardId, i64> = BTreeMap::new();
    let mut next_position = 0i64;
    {
        let mut insert = tx.prepare(
            "INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due, ivl, factor, reps,
                                lapses, left, odue, odid, flags, data)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 0, 0, ?14, ?15)",
        )?;
        let mut card_id_set = Ids::default();
        for card in export.cards {
            let note = *note_ids
                .get(&card.note_id)
                .ok_or(WriteError::Inconsistent("card without its note"))?;
            let deck = *deck_ids
                .get(&card.deck_id)
                .ok_or(WriteError::Inconsistent("card outside the exported decks"))?;
            let id = card_id_set.take(card.created_at.0);
            card_ids.insert(card.id, id);
            let anki = encode(card, clock, &mut written.losses);
            if card.state == CardState::New {
                next_position = next_position.max(anki.due + 1);
            }
            insert.execute(params![
                id,
                note,
                deck,
                card.template_ordinal,
                card.updated_at.0.div_euclid(1000),
                anki.ctype,
                anki.queue,
                anki.due,
                anki.ivl,
                anki.factor,
                anki.reps,
                anki.lapses,
                anki.left,
                anki.flags,
                anki.data,
            ])?;
            written.cards += 1;
        }
    }

    // Review log.
    {
        let mut insert = tx.prepare(
            "INSERT INTO revlog (id, cid, usn, ease, ivl, lastIvl, factor, time, type)
             VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        let mut review_ids = Ids::default();
        for log in export.reviews {
            if log.invalidated_at.is_some() {
                continue;
            }
            let Some(card) = card_ids.get(&log.card_id) else {
                continue;
            };
            let interval = |snapshot: &langualinker_domain::review::CardSnapshot| -> i64 {
                match snapshot.state {
                    CardState::Review => snapshot.fsrs.scheduled_days.max(1),
                    CardState::New => 0,
                    // Learning: Anki stores the delay as negative seconds.
                    _ => snapshot
                        .due_at
                        .map_or(0, |due| -((due.0 - log.reviewed_at.0).max(0) / 1000)),
                }
            };
            let ease = if log.kind == ReviewKind::Manual {
                0
            } else {
                log.rating.db_code()
            };
            insert.execute(params![
                review_ids.take(log.reviewed_at.0),
                card,
                ease,
                interval(&log.card_after),
                interval(&log.card_before),
                log.card_after.legacy_ease.unwrap_or(0),
                log.duration_ms.clamp(0, MAX_ANSWER_MS),
                log.kind.db_code(),
            ])?;
            written.reviews += 1;
        }
        written.losses.add(Loss::ReviewSnapshots, written.reviews);
    }

    let conf = json!({
        "activeDecks": [DEFAULT_DECK_ID], "curDeck": DEFAULT_DECK_ID, "newSpread": 0,
        "collapseTime": 1200, "timeLim": 0, "estTimes": true, "dueCounts": true,
        "curModel": current_model, "nextPos": next_position, "sortType": "noteFld",
        "sortBackwards": false, "addToCur": true, "dayLearnFirst": false,
        "schedVer": 2, "rollover": rollover, "creationOffset": clock.creation_offset()
    });
    tx.execute(
        "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags)
         VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, ?3, ?4, ?5, ?6, '{}')",
        params![
            clock.crt_secs(),
            export.now.0,
            conf.to_string(),
            Value::Object(models).to_string(),
            Value::Object(decks_json).to_string(),
            dconf_json().to_string(),
        ],
    )?;
    tx.commit()?;
    conn.close().map_err(|(_, error)| error)?;
    Ok(written)
}

/// Writes the `.apkg` (ZIP) to `out`; `work_dir` holds the temporary SQLite file.
pub fn write_apkg<W: Write + Seek>(
    export: &Export<'_>,
    work_dir: &Path,
    out: W,
) -> Result<Written, WriteError> {
    let temp = work_dir.join(format!(
        ".apkg-{}-{}.anki2",
        std::process::id(),
        export.now.0
    ));
    let _ = std::fs::remove_file(&temp);
    let result = write_collection(export, &temp).and_then(|written| {
        let bytes = std::fs::read(&temp)?;
        let mut zip = zip::ZipWriter::new(out);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(COLLECTION_ENTRY, options)?;
        zip.write_all(&bytes)?;
        zip.start_file(MEDIA_ENTRY, options)?;
        zip.write_all(b"{}")?;
        zip.finish()?;
        Ok(written)
    });
    let _ = std::fs::remove_file(&temp);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_become_html_and_checksums_match_anki() {
        assert_eq!(field_html("a < b & c\nnext"), "a &lt; b &amp; c<br>next");
        let digest = Sha1::digest(b"Hund");
        assert_eq!(
            checksum("Hund"),
            i64::from(u32::from_be_bytes([
                digest[0], digest[1], digest[2], digest[3]
            ]))
        );
        assert!(checksum("Hund") >= 0);
    }

    #[test]
    fn ids_move_forward_on_collisions() {
        let mut ids = Ids::default();
        assert_eq!(
            [
                ids.take(5),
                ids.take(5),
                ids.take(6),
                ids.take(0),
                ids.take(3)
            ],
            [5, 6, 7, 1, 3]
        );
    }

    #[test]
    fn model_ids_are_stable_per_kind() {
        assert_ne!(
            model_id(NoteTypeKind::Basic),
            model_id(NoteTypeKind::BasicAndReversed)
        );
        let reversed = model_json(NoteTypeKind::BasicAndReversed, 0);
        assert_eq!(reversed["tmpls"].as_array().unwrap().len(), 2);
        assert_eq!(reversed["tmpls"][1]["qfmt"], "{{Back}}");
        assert_eq!(
            model_json(NoteTypeKind::Basic, 0)["tmpls"][0]["qfmt"],
            "{{Front}}"
        );
    }
}
