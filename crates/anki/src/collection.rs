//! Reading an Anki collection out of an `.apkg`: the ZIP, the (zstd-compressed) SQLite file and
//! both schema layouts — schema 11 (note types and decks as JSON in `col`) and schema 15+
//! (`notetypes`, `fields`, `templates`, `decks` tables; Anki 2.1.28 and later).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use rusqlite::{Connection, OpenFlags};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a readable .apkg (ZIP): {0}")]
    Zip(String),
    #[error("no Anki collection inside the .apkg")]
    NoCollection,
    #[error("the .apkg holds only the legacy compatibility stub; export again from Anki")]
    StubOnly,
    #[error("the Anki collection cannot be read: {0}")]
    Sqlite(rusqlite::Error),
    #[error("unsupported Anki collection: {0}")]
    Schema(String),
}

impl From<rusqlite::Error> for ReadError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteType {
    pub id: i64,
    pub name: String,
    /// Field names by ordinal.
    pub fields: Vec<String>,
    /// Template names by ordinal.
    pub templates: Vec<String>,
    pub cloze: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Deck {
    pub id: i64,
    /// Full name with `::` between levels.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: i64,
    pub guid: String,
    pub notetype_id: i64,
    pub tags: Vec<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: i64,
    pub note_id: i64,
    /// Home deck (the original deck when the card sits in a filtered deck).
    pub deck_id: i64,
    pub ord: u32,
    pub kind: i64,
    pub queue: i64,
    /// Original due when the card sits in a filtered deck.
    pub due: i64,
    pub ivl: i64,
    pub factor: i64,
    pub reps: i64,
    pub lapses: i64,
    pub flags: i64,
    /// JSON with FSRS memory state (`s`, `d`) on Anki 23.10+; empty otherwise.
    pub data: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    /// Review time in epoch milliseconds.
    pub id: i64,
    pub card_id: i64,
    pub ease: i64,
    pub ivl: i64,
    pub factor: i64,
    pub time_ms: i64,
    pub kind: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// Schema version (`col.ver`).
    pub schema: i64,
    /// Collection creation in epoch seconds (`col.crt`), the origin of day-based due values.
    pub created_secs: i64,
    pub note_types: HashMap<i64, NoteType>,
    pub decks: HashMap<i64, Deck>,
    pub notes: Vec<Note>,
    pub cards: Vec<Card>,
    pub reviews: Vec<Review>,
    /// Media files listed in the archive (they are not converted).
    pub media_files: usize,
}

const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Opens an `.apkg` file and reads its collection.
pub fn read_apkg(path: &Path) -> Result<Collection, ReadError> {
    let file = std::fs::File::open(path).map_err(ReadError::Io)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| ReadError::Zip(error.to_string()))?;
    let mut entry = |name: &str| -> Result<Option<Vec<u8>>, ReadError> {
        match archive.by_name(name) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).map_err(ReadError::Io)?;
                Ok(Some(bytes))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(error) => Err(ReadError::Zip(error.to_string())),
        }
    };
    let media = entry("media")?;
    // Newest first: anki21b (zstd, schema 18), anki21 (schema 11), anki2 (legacy or stub).
    let collection = match entry("collection.anki21b")? {
        Some(bytes) => decompress(&bytes)?,
        None => match entry("collection.anki21")? {
            Some(bytes) => bytes,
            None => entry("collection.anki2")?.ok_or(ReadError::NoCollection)?,
        },
    };
    let mut collection = read_collection_bytes(&collection)?;
    collection.media_files = media.as_deref().map_or(0, count_media);
    Ok(collection)
}

fn decompress(bytes: &[u8]) -> Result<Vec<u8>, ReadError> {
    if bytes.starts_with(&ZSTD_MAGIC) {
        zstd::decode_all(bytes).map_err(ReadError::Io)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Number of media entries: a JSON object (legacy) or a zstd protobuf list (newer; counted
/// roughly by its entries' field tags).
fn count_media(bytes: &[u8]) -> usize {
    if let Ok(map) = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(bytes) {
        return map.len();
    }
    let Ok(raw) = decompress(bytes) else { return 0 };
    // MediaEntries { repeated MediaEntry entries = 1; } — each entry starts with tag 0x0a.
    let mut count = 0;
    let mut index = 0;
    while index < raw.len() {
        if raw[index] != 0x0a {
            break;
        }
        let (length, used) = varint(&raw[index + 1..]);
        count += 1;
        index += 1 + used + length as usize;
    }
    count
}

fn varint(bytes: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return (value, index + 1);
        }
    }
    (value, bytes.len())
}

/// Reads a collection from SQLite bytes (written to a temporary file; SQLite needs a file).
pub fn read_collection_bytes(bytes: &[u8]) -> Result<Collection, ReadError> {
    let mut temp = tempfile::NamedTempFile::new().map_err(ReadError::Io)?;
    temp.write_all(bytes).map_err(ReadError::Io)?;
    temp.flush().map_err(ReadError::Io)?;
    let conn = Connection::open_with_flags(temp.path(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // Anki (2.1.28+) declares names with `COLLATE unicase`; without it SQLite cannot plan queries.
    conn.create_collation("unicase", |a: &str, b: &str| {
        a.to_lowercase().cmp(&b.to_lowercase())
    })?;
    let collection = read_collection(&conn)?;
    drop(conn);
    Ok(collection)
}

fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )
}

pub fn read_collection(conn: &Connection) -> Result<Collection, ReadError> {
    if !has_table(conn, "col")? || !has_table(conn, "notes")? || !has_table(conn, "cards")? {
        return Err(ReadError::Schema("missing col/notes/cards tables".into()));
    }
    let (schema, created_secs): (i64, i64) =
        conn.query_row("SELECT ver, crt FROM col", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    let (note_types, decks) = if has_table(conn, "notetypes")? {
        (note_types_v15(conn)?, decks_v15(conn)?)
    } else {
        let (models, decks): (String, String) =
            conn.query_row("SELECT models, decks FROM col", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        (note_types_v11(&models)?, decks_v11(&decks)?)
    };

    let mut notes = Vec::new();
    let mut statement = conn.prepare("SELECT id, guid, mid, tags, flds FROM notes ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        let tags: String = row.get(3)?;
        let fields: String = row.get(4)?;
        Ok(Note {
            id: row.get(0)?,
            guid: row.get(1)?,
            notetype_id: row.get(2)?,
            tags: tags.split_whitespace().map(str::to_owned).collect(),
            fields: fields.split('\u{1f}').map(str::to_owned).collect(),
        })
    })?;
    for note in rows {
        notes.push(note?);
    }
    if notes.len() == 1
        && notes[0]
            .fields
            .iter()
            .any(|field| field.contains("Please update to the latest Anki version"))
    {
        return Err(ReadError::StubOnly);
    }

    let has_data = conn.prepare("SELECT data FROM cards LIMIT 0").is_ok();
    let sql = format!(
        "SELECT id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, flags, odue, odid, {}
         FROM cards ORDER BY nid, ord",
        if has_data { "data" } else { "''" }
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        let odid: i64 = row.get(13)?;
        let odue: i64 = row.get(12)?;
        Ok(Card {
            id: row.get(0)?,
            note_id: row.get(1)?,
            deck_id: if odid != 0 { odid } else { row.get(2)? },
            ord: row.get(3)?,
            kind: row.get(4)?,
            queue: row.get(5)?,
            due: if odid != 0 && odue != 0 {
                odue
            } else {
                row.get(6)?
            },
            ivl: row.get(7)?,
            factor: row.get(8)?,
            reps: row.get(9)?,
            lapses: row.get(10)?,
            flags: row.get(11)?,
            data: row.get::<_, Option<String>>(14)?.unwrap_or_default(),
        })
    })?;
    let mut cards = Vec::new();
    for card in rows {
        cards.push(card?);
    }

    let mut reviews = Vec::new();
    if has_table(conn, "revlog")? {
        let mut statement = conn.prepare(
            "SELECT id, cid, ease, ivl, factor, time, type FROM revlog ORDER BY cid, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(Review {
                id: row.get(0)?,
                card_id: row.get(1)?,
                ease: row.get(2)?,
                ivl: row.get(3)?,
                factor: row.get(4)?,
                time_ms: row.get(5)?,
                kind: row.get(6)?,
            })
        })?;
        for review in rows {
            reviews.push(review?);
        }
    }
    Ok(Collection {
        schema,
        created_secs,
        note_types,
        decks,
        notes,
        cards,
        reviews,
        media_files: 0,
    })
}

fn note_types_v15(conn: &Connection) -> Result<HashMap<i64, NoteType>, ReadError> {
    let mut types: HashMap<i64, NoteType> = HashMap::new();
    let mut statement = conn.prepare("SELECT id, name, config FROM notetypes")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<Vec<u8>>>(2)?,
        ))
    })?;
    for row in rows {
        let (id, name, config) = row?;
        // NotetypeConfig.kind is protobuf field 1 (varint): `08 01` = CLOZE; NORMAL (0) is
        // omitted by proto3.
        let cloze = config
            .as_deref()
            .is_some_and(|config| config.starts_with(&[0x08, 0x01]));
        types.insert(
            id,
            NoteType {
                id,
                name,
                fields: Vec::new(),
                templates: Vec::new(),
                cloze,
            },
        );
    }
    let mut statement = conn.prepare("SELECT ntid, name FROM fields ORDER BY ntid, ord")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, name) = row?;
        if let Some(note_type) = types.get_mut(&id) {
            note_type.fields.push(name);
        }
    }
    let mut statement = conn.prepare("SELECT ntid, name FROM templates ORDER BY ntid, ord")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, name) = row?;
        if let Some(note_type) = types.get_mut(&id) {
            note_type.templates.push(name);
        }
    }
    Ok(types)
}

fn decks_v15(conn: &Connection) -> Result<HashMap<i64, Deck>, ReadError> {
    let mut decks = HashMap::new();
    let mut statement = conn.prepare("SELECT id, name FROM decks")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, name) = row?;
        decks.insert(
            id,
            Deck {
                id,
                name: name.replace('\u{1f}', "::"),
            },
        );
    }
    Ok(decks)
}

fn note_types_v11(json: &str) -> Result<HashMap<i64, NoteType>, ReadError> {
    let models: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json)
        .map_err(|error| ReadError::Schema(format!("col.models: {error}")))?;
    let mut types = HashMap::new();
    for (key, model) in models {
        let id = key
            .parse::<i64>()
            .map_err(|_| ReadError::Schema("note type id".into()))?;
        let names = |list: &str| -> Vec<String> {
            let mut items: Vec<(i64, String)> = model[list]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            (
                                item["ord"].as_i64().unwrap_or(0),
                                item["name"].as_str().unwrap_or_default().to_owned(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            items.sort();
            items.into_iter().map(|(_, name)| name).collect()
        };
        types.insert(
            id,
            NoteType {
                id,
                name: model["name"].as_str().unwrap_or_default().to_owned(),
                fields: names("flds"),
                templates: names("tmpls"),
                cloze: model["type"].as_i64() == Some(1),
            },
        );
    }
    Ok(types)
}

fn decks_v11(json: &str) -> Result<HashMap<i64, Deck>, ReadError> {
    let decks: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json)
        .map_err(|error| ReadError::Schema(format!("col.decks: {error}")))?;
    Ok(decks
        .into_iter()
        .filter_map(|(key, deck)| {
            let id = key.parse::<i64>().ok()?;
            Some((
                id,
                Deck {
                    id,
                    name: deck["name"].as_str().unwrap_or_default().to_owned(),
                },
            ))
        })
        .collect())
}

impl Collection {
    /// Ids of the deck named `name` and its subdecks (case-insensitive, `::` levels).
    pub fn deck_tree(&self, name: &str) -> Vec<i64> {
        let wanted = name.to_lowercase();
        self.decks
            .values()
            .filter(|deck| {
                let deck_name = deck.name.to_lowercase();
                deck_name == wanted || deck_name.starts_with(&format!("{wanted}::"))
            })
            .map(|deck| deck.id)
            .collect()
    }

    /// Cards per home deck, for `--list`.
    pub fn cards_per_deck(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for card in &self.cards {
            *counts.entry(card.deck_id).or_default() += 1;
        }
        let mut list: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(id, count)| {
                let name = self
                    .decks
                    .get(&id)
                    .map_or_else(|| format!("#{id}"), |deck| deck.name.clone());
                (name, count)
            })
            .collect();
        list.sort();
        list
    }
}
