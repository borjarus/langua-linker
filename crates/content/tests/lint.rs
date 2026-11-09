//! Linter against the real bundled content and against broken copies of it (plan step 16:
//! "include broken fixtures"). Each case breaks one rule in a temporary copy of `content/decks`
//! and expects exactly that rule's code.

use std::path::{Path, PathBuf};

use langualinker_content::format::DeckManifest;
use langualinker_content::reader::sha256_hex;
use langualinker_content::{DirSource, Mode, Severity, lint};
use serde_json::Value;

fn content_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/decks")
}

fn copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(content_dir()).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    dir
}

fn error_codes(dir: &Path, mode: Mode) -> Vec<&'static str> {
    let mut codes: Vec<_> = lint(&DirSource::new(dir), mode)
        .0
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .map(|issue| issue.code)
        .collect();
    codes.sort_unstable();
    codes.dedup();
    codes
}

/// Rewrites one record of a notes file and fixes the manifest checksum, so only the record rule
/// under test fails.
fn edit_record(dir: &Path, stem: &str, index: usize, change: impl FnOnce(&mut Value)) {
    let notes = dir.join(format!("{stem}.ndjson"));
    let text = std::fs::read_to_string(&notes).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut record: Value = serde_json::from_str(&lines[index]).unwrap();
    change(&mut record);
    lines[index] = serde_json::to_string(&record).unwrap();
    write_notes(dir, stem, &(lines.join("\n") + "\n"));
}

fn write_notes(dir: &Path, stem: &str, text: &str) {
    std::fs::write(dir.join(format!("{stem}.ndjson")), text).unwrap();
    edit_manifest(dir, stem, |manifest| {
        manifest["sha256"] = sha256_hex(text.as_bytes()).into()
    });
}

fn edit_manifest(dir: &Path, stem: &str, change: impl FnOnce(&mut Value)) {
    let path = dir.join(format!("{stem}.json"));
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    change(&mut manifest);
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

#[test]
fn bundled_content_passes_the_dev_gate() {
    let issues = lint(&DirSource::new(content_dir()), Mode::Dev);
    assert!(issues.0.is_empty(), "{:#?}", issues.0);
}

#[test]
fn bundled_content_is_blocked_for_release_until_reviewed() {
    // Candidates are pending native review and the rights audit (D-04); the release gate says so.
    assert_eq!(
        error_codes(&content_dir(), Mode::Release),
        ["release.notReviewed", "release.rightsUnconfirmed"]
    );
}

#[test]
fn approved_and_confirmed_content_passes_the_release_gate() {
    let dir = copy();
    for stem in [
        "de_words_a1",
        "de_sentences_a1",
        "en_words_a1",
        "en_sentences_a1",
        "es_words_a1",
        "es_sentences_a1",
    ] {
        edit_manifest(dir.path(), stem, |manifest| {
            manifest["review"] = serde_json::json!({"status": "approved", "reviewer": "R", "reviewedAt": "2026-11-04"});
            manifest["license"]["rightsConfirmed"] = true.into();
        });
    }
    assert_eq!(error_codes(dir.path(), Mode::Release), Vec::<&str>::new());
}

#[test]
fn manifests_follow_the_schema() {
    let text = std::fs::read_to_string(content_dir().join("de_words_a1.json")).unwrap();
    let manifest: DeckManifest = serde_json::from_str(&text).unwrap();
    assert_eq!(
        manifest.expected_cards,
        manifest.expected_notes * 2,
        "BasicAndReversed"
    );
}

macro_rules! broken {
    ($name:ident, $expected:expr, |$dir:ident| $body:block) => {
        #[test]
        fn $name() {
            let tmp = copy();
            let $dir = tmp.path();
            $body
            assert_eq!(error_codes($dir, Mode::Dev), $expected);
        }
    };
}

// A line that cannot be read also breaks the counts and the sentence links pointing at it.
broken!(
    malformed_line,
    [
        "counts.cards",
        "counts.notes",
        "link.unresolved",
        "record.malformed"
    ],
    |dir| {
        let text = std::fs::read_to_string(dir.join("en_words_a1.ndjson")).unwrap();
        write_notes(dir, "en_words_a1", &text.replacen("{", "{{", 1));
    }
);

broken!(
    unknown_record_field,
    [
        "counts.cards",
        "counts.notes",
        "link.unresolved",
        "record.unknownField"
    ],
    |dir| {
        edit_record(dir, "en_words_a1", 0, |record| {
            record["audio"] = "x.mp3".into()
        });
    }
);

broken!(unknown_schema_version, ["schema.unsupported"], |dir| {
    edit_manifest(dir, "en_words_a1", |manifest| {
        manifest["schemaVersion"] = 3.into()
    });
});

// The rejected manifest is skipped, so the sentence deck depending on it fails too.
broken!(
    unknown_manifest_field,
    ["deck.dependency", "manifest.unknownField"],
    |dir| {
        edit_manifest(dir, "en_words_a1", |manifest| {
            manifest["audioDir"] = "a".into()
        });
    }
);

broken!(
    missing_language,
    ["deck.dependency", "language.target"],
    |dir| {
        edit_manifest(dir, "en_words_a1", |manifest| {
            manifest["targetLanguage"] = "".into()
        });
    }
);

broken!(missing_license, ["license.missing"], |dir| {
    edit_manifest(dir, "en_words_a1", |manifest| {
        manifest["license"]["name"] = " ".into()
    });
});

broken!(
    duplicate_guid_across_decks,
    ["guid.duplicate", "link.unresolved"],
    |dir| {
        let first = std::fs::read_to_string(dir.join("de_words_a1.ndjson")).unwrap();
        let guid: Value = serde_json::from_str(first.lines().next().unwrap()).unwrap();
        edit_record(dir, "en_words_a1", 0, |record| {
            record["guid"] = guid["guid"].clone()
        });
    }
);

broken!(duplicate_front, ["front.duplicate"], |dir| {
    let text = std::fs::read_to_string(dir.join("en_words_a1.ndjson")).unwrap();
    let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    edit_record(dir, "en_words_a1", 1, |record| {
        record["front"] = first["front"].clone();
        record["back"] = first["back"].clone();
    });
});

broken!(empty_required_field, ["text.required"], |dir| {
    edit_record(dir, "de_words_a1", 2, |record| record["back"] = "".into());
});

broken!(missing_association, ["associations.required"], |dir| {
    edit_record(dir, "de_words_a1", 2, |record| {
        record["associations"] = Value::Array(vec![])
    });
});

broken!(
    association_longer_than_the_gui_allows,
    ["text.tooLong"],
    |dir| {
        edit_record(dir, "de_words_a1", 2, |record| {
            record["associations"][0]["text"] = "a".repeat(501).into()
        });
    }
);

broken!(not_nfc, ["text.notNfc"], |dir| {
    edit_record(dir, "de_words_a1", 2, |record| {
        record["front"] = "z\u{0307}".into()
    });
});

broken!(range_out_of_bounds, ["range.invalid"], |dir| {
    edit_record(dir, "de_sentences_a1", 0, |record| {
        record["wordLinks"][0]["end"] = 400.into()
    });
});

broken!(range_does_not_match_surface, ["range.mismatch"], |dir| {
    edit_record(dir, "de_sentences_a1", 0, |record| {
        record["wordLinks"][0]["surfaceForm"] = "Du".into()
    });
});

broken!(overlapping_ranges, ["range.overlap"], |dir| {
    edit_record(dir, "de_sentences_a1", 0, |record| {
        let first = record["wordLinks"][0].clone();
        record["wordLinks"].as_array_mut().unwrap().insert(1, first);
    });
});

broken!(unresolved_word_reference, ["link.unresolved"], |dir| {
    edit_record(dir, "de_sentences_a1", 0, |record| {
        record["wordLinks"][0]["wordGuid"] = "dew-missing".into()
    });
});

broken!(
    word_reference_into_another_language,
    ["link.unresolved"],
    |dir| {
        let english = std::fs::read_to_string(dir.join("en_words_a1.ndjson")).unwrap();
        let other: Value = serde_json::from_str(english.lines().next().unwrap()).unwrap();
        edit_record(dir, "de_sentences_a1", 0, |record| {
            record["wordLinks"][0]["wordGuid"] = other["guid"].clone()
        });
    }
);

broken!(sentence_without_links, ["wordLinks.required"], |dir| {
    edit_record(dir, "de_sentences_a1", 0, |record| {
        record.as_object_mut().unwrap().remove("wordLinks");
    });
});

broken!(
    note_count_differs,
    ["counts.cards", "counts.notes", "link.unresolved"],
    |dir| {
        edit_manifest(dir, "en_words_a1", |manifest| {
            manifest["expectedNotes"] = 1.into()
        });
        edit_manifest(dir, "en_words_a1", |manifest| {
            manifest["expectedCards"] = 1.into()
        });
        let text = std::fs::read_to_string(dir.join("en_words_a1.ndjson")).unwrap();
        let kept: Vec<&str> = text.lines().take(2).collect();
        write_notes(dir, "en_words_a1", &(kept.join("\n") + "\n"));
    }
);

broken!(card_count_ignores_reverse_cards, ["counts.cards"], |dir| {
    edit_manifest(dir, "en_words_a1", |manifest| {
        manifest["expectedCards"] = manifest["expectedNotes"].clone();
    });
});

broken!(checksum_differs, ["checksum.mismatch"], |dir| {
    edit_manifest(dir, "en_words_a1", |manifest| {
        manifest["sha256"] = "0".repeat(64).into()
    });
});

broken!(
    dependency_listed_after_its_dependent,
    ["deck.dependency"],
    |dir| {
        let path = dir.join("catalog.json");
        let mut catalog: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        catalog["decks"] = serde_json::json!([
            "de_sentences_a1",
            "de_words_a1",
            "en_words_a1",
            "en_sentences_a1"
        ]);
        std::fs::write(&path, catalog.to_string()).unwrap();
    }
);
