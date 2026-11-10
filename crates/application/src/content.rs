//! Bundled content: onboarding state, the `prepareContent` job and the streaming importer (plan
//! Phase 8, ADR-14).
//!
//! - **Pending list.** Decks to install or update live in the `content.pendingDecks` setting.
//!   Onboarding adds the chosen decks; startup adds installed decks whose bundled version is newer.
//!   A deck leaves the list in the same transaction that installs it, so a killed process resumes
//!   exactly where it stopped ([`ContentService::resume`]).
//! - **One transaction per deck.** Every record is validated with the linter's rules before the
//!   transaction opens; the transaction streams the file again and writes it. A kill or an error
//!   rolls the deck back as a whole — there is no staging table to clean up.
//! - **Idempotent.** Notes, associations and tips are matched by GUID, cards by (note, template).
//!   A second run creates nothing; scheduling columns are never written.
//! - **User data wins.** Fields listed in `content_overrides` are kept; a differing bundled value
//!   is reported as an `overrideConflict` warning. Deleted notes stay deleted. Notes a newer
//!   version no longer ships are kept when they have progress or edits, archived otherwise.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex};

use langualinker_content::check::{
    DeckChecker, Issues, Mode, parse_deck_type, parse_note_type, parse_target_language,
};
use langualinker_content::format::{
    CardRecord, CardStateName, ContentKind, DeckManifest, NoteRecord, QueueName, ReviewKindName,
    SourceKind,
};
use langualinker_content::{Bundle, ContentSource, NotAPackage, Package};
use langualinker_domain::card::{Card, CardFlag, CardFsrsState, CardState, NewCard, QueueState};
use langualinker_domain::content::{
    AssociationProvenance, ModerationStatus, NewAssociation, SentenceWordLink, TipProvenance,
    WordLinkResolution,
};
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::{CardSnapshot, Rating, ReviewKind, ReviewLog};
use langualinker_domain::text::nfc;
use langualinker_domain::{
    DeckId, Guid, NoteId, ReviewLogId, TimestampMs, ValidationError, WordLinkId, validation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::jobs::{JobContext, JobId, JobKind, JobRecord, JobService, NewJob};
use crate::ports::{Clock, CollectionScope, EventSink, Rng};
use crate::repositories::{BundledNote, Repositories, UnitOfWork};
use crate::{AppError, AppResult};

/// Settings key of the onboarding answers.
pub const ONBOARDING_KEY: &str = "onboarding";
/// Settings key of the daily goal (shared with the settings screen, Phase 12).
pub const DAILY_GOAL_KEY: &str = "dailyGoal";
/// Settings key of the deck GUIDs still to install or update.
pub const PENDING_KEY: &str = "content.pendingDecks";

pub const DEFAULT_DAILY_GOAL: u32 = 20;
/// Records validated between two persisted progress updates.
const PROGRESS_BATCH: u32 = 250;
/// Warnings kept in a job result; the rest are only counted.
const MAX_WARNINGS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOnboarding {
    completed: bool,
    native_language: String,
    target_languages: Vec<String>,
    daily_goal: u32,
}

/// A deck the catalog offers.
#[derive(Debug, Clone, PartialEq)]
pub struct BundledDeckInfo {
    pub guid: String,
    pub name: String,
    pub description: String,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
    pub note_count: u32,
    pub card_count: u32,
    pub level: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OnboardingState {
    pub completed: bool,
    pub available: Vec<BundledDeckInfo>,
    /// Bundled decks present (not deleted) in the collection.
    pub installed: Vec<String>,
    pub native_language: NativeLanguage,
    pub target_languages: Vec<TargetLanguage>,
    pub daily_goal: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OnboardingInput {
    pub operation_id: String,
    pub native_language: NativeLanguage,
    pub target_languages: Vec<TargetLanguage>,
    pub daily_goal: u32,
    pub deck_guids: Vec<String>,
}

/// A notable outcome for one deck or note. `code` is stable: `overrideConflict`, `deckDeleted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentWarning {
    pub code: &'static str,
    pub deck_guid: String,
    pub note_guid: Option<String>,
    /// The field kept for the user (`front`, `association:<guid>`, …).
    pub field: Option<String>,
}

/// Result of a `prepareContent` job.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrepareOutcome {
    /// Decks created or changed, in install order.
    pub deck_ids: Vec<DeckId>,
    pub created: u32,
    pub updated: u32,
    pub unchanged: u32,
    /// Removed from the content but kept because the user has progress or edits.
    pub kept: u32,
    /// Removed from the content without progress: cards archived, history intact.
    pub archived: u32,
    pub warnings: Vec<ContentWarning>,
    /// Warnings beyond the first [`MAX_WARNINGS`].
    pub warnings_dropped: u32,
}

impl PrepareOutcome {
    fn add(&mut self, deck: DeckOutcome) {
        if let Some(id) = deck.deck_id
            && (deck.created + deck.updated + deck.kept + deck.archived > 0 || deck.deck_created)
            && !self.deck_ids.contains(&id)
        {
            self.deck_ids.push(id);
        }
        self.created += deck.created;
        self.updated += deck.updated;
        self.unchanged += deck.unchanged;
        self.kept += deck.kept;
        self.archived += deck.archived;
        for warning in deck.warnings {
            if self.warnings.len() < MAX_WARNINGS {
                self.warnings.push(warning);
            } else {
                self.warnings_dropped += 1;
            }
        }
    }
}

#[derive(Debug, Default)]
struct DeckOutcome {
    deck_id: Option<DeckId>,
    deck_created: bool,
    created: u32,
    updated: u32,
    unchanged: u32,
    kept: u32,
    archived: u32,
    /// Cards whose progress came from an import package.
    scheduled: u32,
    /// Review log rows written from an import package.
    reviews: u32,
    warnings: Vec<ContentWarning>,
}

/// Result of importing a package (job `import`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub deck_id: DeckId,
    pub deck_created: bool,
    pub created: u32,
    pub updated: u32,
    pub unchanged: u32,
    pub kept: u32,
    pub archived: u32,
    /// Cards created with the progress they had in the source.
    pub scheduled: u32,
    pub reviews: u32,
    pub warnings: Vec<ContentWarning>,
}

/// What `previewDeckImport` shows before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct PackagePreview {
    pub token: String,
    pub file_name: String,
    pub manifest: DeckManifest,
    pub notes: u32,
    pub cards: u32,
    pub scheduled: u32,
    pub suspended: u32,
    pub reviews: u32,
    /// The deck this package updates (same deck GUID), if it was imported before.
    pub existing_deck: Option<DeckId>,
    /// Notes the import creates and notes already present (kept or updated, progress untouched).
    pub new_notes: u32,
    pub existing_notes: u32,
    /// Warning codes of the content rules (e.g. `front.homograph`), with the file line.
    pub warnings: Vec<(&'static str, Option<u32>)>,
}

struct StoredPreview {
    token: String,
    package: Arc<Package>,
    fingerprint: String,
}

/// Renders job results as contract JSON (`JobResultDto`); supplied by the composition root,
/// which owns the contract types.
#[derive(Clone, Copy)]
pub struct ResultEncoders {
    pub prepare: fn(&PrepareOutcome) -> String,
    pub import: fn(&ImportOutcome) -> String,
}

pub struct ContentService<U: UnitOfWork> {
    db: Arc<U>,
    source: Arc<dyn ContentSource>,
    bundle: Result<Arc<Bundle>, String>,
    jobs: JobService<U>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
    encode: ResultEncoders,
    /// One content job writes at a time; a second one waits and then finds less to do.
    run_lock: Arc<Mutex<()>>,
    /// The last package preview; `commitDeckImport` must name its token.
    preview: Arc<Mutex<Option<StoredPreview>>>,
}

impl<U: UnitOfWork> Clone for ContentService<U> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            source: self.source.clone(),
            bundle: self.bundle.clone(),
            jobs: self.jobs.clone(),
            events: self.events.clone(),
            clock: self.clock.clone(),
            rng: self.rng.clone(),
            encode: self.encode,
            run_lock: self.run_lock.clone(),
            preview: self.preview.clone(),
        }
    }
}

fn read_pending(tx: &mut dyn Repositories) -> AppResult<Vec<String>> {
    let Some(json) = tx.setting(PENDING_KEY)? else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&json)
        .map_err(|_| AppError::Internal("pending decks setting is not a list".into()))
}

fn write_pending(tx: &mut dyn Repositories, pending: &[String], at: TimestampMs) -> AppResult<()> {
    let json =
        serde_json::to_string(pending).map_err(|error| AppError::Internal(error.to_string()))?;
    tx.put_setting(PENDING_KEY, &json, at)
}

fn read_onboarding(tx: &mut dyn Repositories) -> AppResult<Option<StoredOnboarding>> {
    tx.setting(ONBOARDING_KEY)?
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|_| AppError::Internal("onboarding setting is malformed".into()))
        })
        .transpose()
}

/// Target language codes chosen at onboarding (empty before it).
pub fn onboarding_target_languages(tx: &mut dyn Repositories) -> AppResult<Vec<String>> {
    Ok(read_onboarding(tx)?
        .map(|stored| stored.target_languages)
        .unwrap_or_default())
}

fn content_error(issues: &Issues) -> AppError {
    let issue = issues.first_error();
    if let Some(issue) = issue {
        // File, line and code only: the message may quote card content.
        tracing::error!(code = issue.code, file = %issue.file, line = issue.line, "bundled content rejected");
    }
    AppError::Validation(ValidationError::new(
        "content",
        issue.map_or("invalid", |issue| issue.code),
    ))
}

impl<U: UnitOfWork> ContentService<U> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<U>,
        source: Arc<dyn ContentSource>,
        jobs: JobService<U>,
        events: Arc<dyn EventSink>,
        clock: Arc<dyn Clock>,
        rng: Arc<dyn Rng>,
        encode: ResultEncoders,
    ) -> Self {
        let bundle = Bundle::load(source.as_ref(), Mode::Dev)
            .map(|(bundle, _)| Arc::new(bundle))
            .map_err(|issues| {
                let error = content_error(&issues);
                error.to_string()
            });
        Self {
            db,
            source,
            bundle,
            jobs,
            events,
            clock,
            rng,
            encode,
            run_lock: Arc::default(),
            preview: Arc::default(),
        }
    }

    fn bundle(&self) -> AppResult<Arc<Bundle>> {
        self.bundle.clone().map_err(AppError::Internal)
    }

    /// Catalog entries in install order.
    pub fn available_decks(&self) -> AppResult<Vec<BundledDeckInfo>> {
        let bundle = self.bundle()?;
        Ok(bundle
            .decks
            .iter()
            .filter_map(|deck| {
                Some(BundledDeckInfo {
                    guid: deck.deck_guid.clone(),
                    name: deck.name.clone(),
                    description: deck.description.clone(),
                    target_language: parse_target_language(&deck.target_language)?,
                    deck_type: parse_deck_type(&deck.deck_type)?,
                    note_count: deck.expected_notes,
                    card_count: deck.expected_cards,
                    level: deck.level.clone(),
                })
            })
            .collect())
    }

    /// Licences of the bundled decks for the Licences screen: `(deck name, licence, url)`.
    pub fn content_licences(&self) -> AppResult<Vec<(String, String, Option<String>)>> {
        let bundle = self.bundle()?;
        Ok(bundle
            .decks
            .iter()
            .map(|deck| {
                let licence = match &deck.license.attribution {
                    Some(attribution) => format!("{} — {attribution}", deck.license.name),
                    None => deck.license.name.clone(),
                };
                (deck.name.clone(), licence, deck.license.url.clone())
            })
            .collect())
    }

    /// Reads an import package, checks it with the content rules and remembers it for
    /// [`ContentService::commit_package`]. Nothing is written.
    pub fn preview_package(&self, file_name: &str, bytes: &[u8]) -> AppResult<PackagePreview> {
        let package = match Package::parse(bytes) {
            Err(NotAPackage::Zip) => return Err(AppError::UnsupportedFormat { format: "apkg" }),
            Err(NotAPackage::TooLarge) => {
                return Err(ValidationError::new("file", "tooLarge").into());
            }
            Err(NotAPackage::Unknown) => {
                return Err(AppError::UnsupportedFormat { format: "unknown" });
            }
            Ok(Err(issues)) => return Err(content_error(&issues)),
            Ok(Ok(package)) => package,
        };
        let check = package.check();
        if check.issues.has_errors() {
            return Err(content_error(&check.issues));
        }
        let warnings = check
            .issues
            .0
            .iter()
            .map(|issue| (issue.code, issue.line))
            .collect();
        let deck_guid = guid(&package.manifest.deck_guid)?;
        let note_guids: Vec<String> = check.note_guids.iter().cloned().collect();
        let (existing_deck, existing_notes, fingerprint) = self.db.read(move |tx| {
            let deck = tx.deck_by_guid(&deck_guid)?;
            if deck.as_ref().is_some_and(|deck| deck.deleted_at.is_some()) {
                return Err(AppError::Deleted { entity: "deck" });
            }
            let mut existing = 0u32;
            for note in &note_guids {
                if let Ok(parsed) = Guid::parse(note)
                    && tx.note_by_guid(&parsed)?.is_some()
                {
                    existing += 1;
                }
            }
            Ok((
                deck.map(|deck| deck.id),
                existing,
                tx.collection_fingerprint()?,
            ))
        })?;
        let digest = Sha256::digest(format!(
            "{}|{fingerprint}|{}",
            package.body_sha256(),
            self.clock.now().0
        ));
        let token: String = digest
            .iter()
            .take(12)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let preview = PackagePreview {
            token: token.clone(),
            file_name: file_name.to_owned(),
            manifest: package.manifest.clone(),
            notes: check.notes,
            cards: check.cards,
            scheduled: check.scheduled,
            suspended: check.suspended,
            reviews: check.reviews,
            existing_deck,
            new_notes: check.notes - existing_notes,
            existing_notes,
            warnings,
        };
        *self
            .preview
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(StoredPreview {
            token,
            package: Arc::new(package),
            fingerprint,
        });
        Ok(preview)
    }

    /// Imports the previewed package in one transaction (job `import`). A repeated
    /// `operation_id` returns the first job; a changed collection or another preview since then
    /// is `PREVIEW_STALE`.
    pub fn commit_package(&self, operation_id: String, token: &str) -> AppResult<JobRecord> {
        let recorded = {
            let operation = operation_id.clone();
            self.db.read(move |tx| tx.recorded_operation(&operation))?
        };
        if let Some(recorded) = recorded {
            return self.jobs.get(&JobId(recorded.result_json));
        }
        let (package, fingerprint) = {
            let stored = self
                .preview
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match stored.as_ref() {
                Some(stored) if stored.token == token => {
                    (stored.package.clone(), stored.fingerprint.clone())
                }
                _ => return Err(AppError::PreviewStale),
            }
        };
        if self.db.read(|tx| tx.collection_fingerprint())? != fingerprint {
            return Err(AppError::PreviewStale);
        }
        let total = u64::from(package.manifest.expected_notes.max(1));
        let service = self.clone();
        self.jobs.start(
            NewJob {
                kind: JobKind::Import,
                operation_id: Some(operation_id),
                total: Some(total),
            },
            Box::new(move |context| {
                let _running = service
                    .run_lock
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                context.begin_commit()?;
                let outcome = service.install_package(&package)?;
                context.progress(total, total)?;
                Ok(Some((service.encode.import)(&outcome)))
            }),
        )
    }

    fn install_package(&self, package: &Package) -> AppResult<ImportOutcome> {
        let source = package.source();
        let manifest = package.manifest.clone();
        let rng = self.rng.clone();
        let at = self.clock.now();
        let outcome = self.db.write(move |tx| {
            Installer {
                tx,
                manifest: &manifest,
                rng: rng.as_ref(),
                at,
                base_position: 0,
            }
            .run(&source)
        })?;
        self.events.collection_changed(
            &[
                CollectionScope::Decks,
                CollectionScope::Notes,
                CollectionScope::Cards,
                CollectionScope::Stats,
            ],
            at,
        );
        let deck_id = outcome
            .deck_id
            .ok_or(AppError::Deleted { entity: "deck" })?;
        Ok(ImportOutcome {
            deck_id,
            deck_created: outcome.deck_created,
            created: outcome.created,
            updated: outcome.updated,
            unchanged: outcome.unchanged,
            kept: outcome.kept,
            archived: outcome.archived,
            scheduled: outcome.scheduled,
            reviews: outcome.reviews,
            warnings: outcome.warnings,
        })
    }

    /// GUIDs of decks waiting to be installed or updated.
    pub fn pending(&self) -> AppResult<Vec<String>> {
        self.db.read(read_pending)
    }

    pub fn onboarding_state(&self) -> AppResult<OnboardingState> {
        let available = self.available_decks()?;
        let catalog: Vec<String> = available.iter().map(|deck| deck.guid.clone()).collect();
        let (stored, decks, goal) = self.db.read(|tx| {
            let stored = read_onboarding(tx)?;
            let goal = tx.setting(DAILY_GOAL_KEY)?;
            Ok((stored, tx.active_decks()?, goal))
        })?;
        let installed: Vec<String> = catalog
            .into_iter()
            .filter(|guid| {
                decks
                    .iter()
                    .any(|deck| deck.bundled && deck.guid.as_str() == guid)
            })
            .collect();
        let mut targets: Vec<TargetLanguage> = match &stored {
            Some(stored) => stored
                .target_languages
                .iter()
                .filter_map(|code| parse_target_language(code))
                .collect(),
            None => Vec::new(),
        };
        if targets.is_empty() {
            for deck in &decks {
                if !targets.contains(&deck.target_language) {
                    targets.push(deck.target_language);
                }
            }
        }
        if targets.is_empty() {
            targets.push(TargetLanguage::De);
        }
        let daily_goal = goal
            .and_then(|json| serde_json::from_str::<u32>(&json).ok())
            .or(stored.as_ref().map(|stored| stored.daily_goal))
            .unwrap_or(DEFAULT_DAILY_GOAL);
        Ok(OnboardingState {
            completed: stored.as_ref().is_some_and(|stored| stored.completed),
            available,
            installed,
            native_language: NativeLanguage::Pl,
            target_languages: targets,
            daily_goal,
        })
    }

    /// Saves the answers, queues the chosen decks that are not installed yet and starts the
    /// prepare job. A repeated `operation_id` returns the first job.
    pub fn complete_onboarding(&self, input: OnboardingInput) -> AppResult<JobRecord> {
        validation::in_range("dailyGoal", input.daily_goal.into(), 1, 500)?;
        if input.target_languages.is_empty() {
            return Err(ValidationError::new("targetLanguages", "required").into());
        }
        let unique: HashSet<_> = input.target_languages.iter().collect();
        if unique.len() != input.target_languages.len() {
            return Err(ValidationError::new("targetLanguages", "duplicate").into());
        }
        let available = self.available_decks()?;
        let mut chosen = Vec::new();
        for guid in &input.deck_guids {
            let deck = available
                .iter()
                .find(|deck| &deck.guid == guid)
                .ok_or(ValidationError::new("deckGuids", "unknown"))?;
            if !input.target_languages.contains(&deck.target_language) {
                return Err(ValidationError::new("deckGuids", "language").into());
            }
            if !chosen.contains(guid) {
                chosen.push(guid.clone());
            }
        }
        let stored = StoredOnboarding {
            completed: true,
            native_language: input.native_language.code().to_owned(),
            target_languages: input
                .target_languages
                .iter()
                .map(|language| language.code().to_owned())
                .collect(),
            daily_goal: input.daily_goal,
        };
        let at = self.clock.now();
        self.db.write(move |tx| {
            let json = serde_json::to_string(&stored)
                .map_err(|error| AppError::Internal(error.to_string()))?;
            tx.put_setting(ONBOARDING_KEY, &json, at)?;
            tx.put_setting(DAILY_GOAL_KEY, &input.daily_goal.to_string(), at)?;
            let mut pending = read_pending(tx)?;
            for guid in chosen {
                let installed = Guid::parse(&guid)
                    .ok()
                    .map(|parsed| tx.deck_by_guid(&parsed))
                    .transpose()?
                    .flatten()
                    .is_some();
                if !installed && !pending.contains(&guid) {
                    pending.push(guid);
                }
            }
            write_pending(tx, &pending, at)
        })?;
        self.events
            .collection_changed(&[CollectionScope::Settings], at);
        self.start(Some(input.operation_id))
    }

    /// At startup: queues installed bundled decks with a newer bundled version and starts a job
    /// when anything is pending (also after a kill during preparation).
    pub fn resume(&self) -> AppResult<Option<JobRecord>> {
        let bundle = self.bundle()?;
        let at = self.clock.now();
        let pending = self.db.write(move |tx| {
            let mut pending = read_pending(tx)?;
            for deck in tx.active_decks()? {
                let Some(manifest) = bundle.deck(deck.guid.as_str()) else {
                    continue;
                };
                let outdated = deck.bundled
                    && deck
                        .content_version
                        .as_deref()
                        .is_none_or(|installed| installed < manifest.content_version.as_str());
                if outdated && !pending.iter().any(|guid| guid == &manifest.deck_guid) {
                    pending.push(manifest.deck_guid.clone());
                }
            }
            write_pending(tx, &pending, at)?;
            Ok(pending)
        })?;
        if pending.is_empty() {
            return Ok(None);
        }
        self.start(None).map(Some)
    }

    fn start(&self, operation_id: Option<String>) -> AppResult<JobRecord> {
        let total = self.pending_notes()? * 2;
        let service = self.clone();
        self.jobs.start(
            NewJob {
                kind: JobKind::PrepareContent,
                operation_id,
                total: Some(u64::from(total.max(1))),
            },
            Box::new(move |context| {
                let outcome = service.run(context)?;
                Ok(Some((service.encode.prepare)(&outcome)))
            }),
        )
    }

    fn pending_notes(&self) -> AppResult<u32> {
        let bundle = self.bundle()?;
        let pending = self.pending()?;
        Ok(pending
            .iter()
            .filter_map(|guid| bundle.deck(guid))
            .map(|deck| deck.expected_notes)
            .sum())
    }

    /// Pending decks in catalog order (word decks before the sentence decks linking to them).
    fn ordered_pending(&self, bundle: &Bundle) -> AppResult<Vec<DeckManifest>> {
        let pending = self.pending()?;
        let mut unknown = Vec::new();
        let ordered: Vec<DeckManifest> = bundle
            .decks
            .iter()
            .filter(|deck| pending.contains(&deck.deck_guid))
            .cloned()
            .collect();
        for guid in &pending {
            if bundle.deck(guid).is_none() {
                unknown.push(guid.clone());
            }
        }
        if !unknown.is_empty() {
            // A deck dropped from the catalog cannot be installed; forget it.
            tracing::warn!(
                count = unknown.len(),
                "pending decks are not in the catalog"
            );
            let at = self.clock.now();
            self.db.write(move |tx| {
                let pending: Vec<String> = read_pending(tx)?
                    .into_iter()
                    .filter(|guid| !unknown.contains(guid))
                    .collect();
                write_pending(tx, &pending, at)
            })?;
        }
        Ok(ordered)
    }

    fn run(&self, context: &JobContext<U>) -> AppResult<PrepareOutcome> {
        let _running = self
            .run_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let bundle = self.bundle()?;
        let mut outcome = PrepareOutcome::default();
        let mut done: u64 = 0;
        let mut total: u64 = 0;
        loop {
            let decks = self.ordered_pending(&bundle)?;
            if decks.is_empty() {
                break;
            }
            total = total.max(
                done + decks
                    .iter()
                    .map(|deck| u64::from(deck.expected_notes) * 2)
                    .sum::<u64>(),
            );
            // Phase 1: validate every pending deck before anything is written (cancellable).
            for deck in &decks {
                self.validate(&bundle, deck, context, &mut done, total)?;
            }
            // Phase 2: write, one transaction per deck. A cancel no longer applies.
            context.begin_commit()?;
            for deck in decks {
                let notes = u64::from(deck.expected_notes);
                let result = self.install(&bundle, deck)?;
                outcome.add(result);
                done += notes;
                context.progress(done.min(total), total)?;
            }
        }
        context.progress(total.max(1), total.max(1))?;
        Ok(outcome)
    }

    fn validate(
        &self,
        bundle: &Bundle,
        deck: &DeckManifest,
        context: &JobContext<U>,
        done: &mut u64,
        total: u64,
    ) -> AppResult<()> {
        let source = self.source.as_ref();
        let dependencies = bundle
            .dependency_guids(source, deck)
            .map_err(|issues| content_error(&issues))?;
        let mut issues = Issues::default();
        let mut seen = HashSet::new();
        let mut reader = bundle
            .notes(source, deck)
            .map_err(|error| AppError::Internal(format!("bundled notes unreadable: {error}")))?;
        let mut checker = DeckChecker::new(deck, &mut seen, &dependencies);
        let mut batch = 0;
        for (line, record) in &mut reader {
            match record {
                Ok(record) => checker.record(line, &record, &mut issues),
                Err(error) => issues.error(error.code, &deck.notes_file, Some(line), error.message),
            }
            batch += 1;
            if batch == PROGRESS_BATCH {
                *done += u64::from(batch);
                batch = 0;
                context.progress((*done).min(total), total)?;
            }
        }
        *done += u64::from(batch);
        checker.finish(&reader.sha256_hex(), &mut issues);
        if issues.has_errors() {
            return Err(content_error(&issues));
        }
        context.check_cancelled()
    }

    /// Writes one deck in one transaction and takes it off the pending list.
    fn install(&self, bundle: &Bundle, manifest: DeckManifest) -> AppResult<DeckOutcome> {
        let source = self.source.clone();
        let rng = self.rng.clone();
        let at = self.clock.now();
        let dependents: Vec<String> = if manifest.content_kind == ContentKind::Words {
            bundle
                .decks
                .iter()
                .filter(|deck| deck.depends_on.contains(&manifest.deck_guid))
                .map(|deck| deck.deck_guid.clone())
                .collect()
        } else {
            Vec::new()
        };
        let guid = manifest.deck_guid.clone();
        let outcome = self.db.write(move |tx| {
            let outcome = Installer {
                tx,
                manifest: &manifest,
                rng: rng.as_ref(),
                at,
                base_position: 0,
            }
            .run(source.as_ref())?;
            let mut pending: Vec<String> = read_pending(tx)?
                .into_iter()
                .filter(|pending| pending != &guid)
                .collect();
            // Sentence decks already installed relink against the new or changed words.
            if outcome.created + outcome.updated > 0 || outcome.deck_created {
                for dependent in dependents {
                    let installed = Guid::parse(&dependent)
                        .ok()
                        .map(|parsed| tx.deck_by_guid(&parsed))
                        .transpose()?
                        .flatten()
                        .is_some_and(|deck| deck.deleted_at.is_none());
                    if installed && !pending.contains(&dependent) {
                        pending.push(dependent);
                    }
                }
            }
            write_pending(tx, &pending, at)?;
            Ok(outcome)
        })?;
        self.events.collection_changed(
            &[
                CollectionScope::Decks,
                CollectionScope::Notes,
                CollectionScope::Cards,
                CollectionScope::Stats,
            ],
            at,
        );
        Ok(outcome)
    }
}

/// `fsrs_version` of review logs that came from an import package rather than this scheduler.
pub const IMPORTED_SCHEDULER: &str = "import";

const DAY_MS: i64 = 86_400_000;

fn card_state(name: CardStateName) -> CardState {
    match name {
        CardStateName::New => CardState::New,
        CardStateName::Learning => CardState::Learning,
        CardStateName::Review => CardState::Review,
        CardStateName::Relearning => CardState::Relearning,
    }
}

fn queue_state(name: QueueName) -> QueueState {
    match name {
        QueueName::New => QueueState::New,
        QueueName::Learning => QueueState::Learning,
        QueueName::Review => QueueState::Review,
        QueueName::DayLearn => QueueState::DayLearn,
        QueueName::Suspended => QueueState::Suspended,
        QueueName::SchedBuried => QueueState::SchedBuried,
        QueueName::UserBuried => QueueState::UserBuried,
    }
}

/// Copies a validated card record onto a freshly created card.
fn apply_card_record(card: &mut Card, record: &CardRecord, base_position: i64) {
    card.state = card_state(record.state);
    card.queue = queue_state(record.queue);
    card.due_at = record.due_at_ms.map(TimestampMs);
    if card.state == CardState::New {
        card.position = base_position + record.position.unwrap_or(0).max(0);
    }
    card.fsrs = CardFsrsState {
        stability: record.stability,
        difficulty: record.difficulty,
        last_review_at: record.last_review_at_ms.map(TimestampMs),
        scheduled_days: record.scheduled_days,
        elapsed_days: 0,
    };
    card.legacy_ease = record.ease;
    card.reps = record.reps;
    card.lapses = record.lapses;
    card.flag = CardFlag::from_db_code(i64::from(record.flag)).unwrap_or(CardFlag::None);
    // The source's position inside the learning steps is not portable: the step list restarts.
    card.learning_step = None;
}

/// The card state after one imported review, derived from the review row alone.
fn replay(
    before: &CardSnapshot,
    review: &langualinker_content::format::ReviewRecord,
) -> CardSnapshot {
    let lapse = review.kind == ReviewKindName::Review && review.rating == 1;
    let state = if review.scheduled_days > 0 {
        CardState::Review
    } else if lapse || review.kind == ReviewKindName::Relearning {
        CardState::Relearning
    } else {
        CardState::Learning
    };
    let at = TimestampMs(review.reviewed_at_ms);
    CardSnapshot {
        state,
        queue: QueueState::natural(state, false),
        due_at: Some(TimestampMs(
            review.reviewed_at_ms + review.scheduled_days * DAY_MS,
        )),
        position: before.position,
        fsrs: CardFsrsState {
            stability: None,
            difficulty: None,
            last_review_at: Some(at),
            scheduled_days: review.scheduled_days,
            elapsed_days: before
                .fsrs
                .last_review_at
                .map_or(0, |last| (at.0 - last.0).max(0) / DAY_MS),
        },
        legacy_ease: review.ease.or(before.legacy_ease),
        reps: before.reps + 1,
        lapses: before.lapses + i64::from(lapse),
        learning_step: None,
        buried_until: None,
        flag: before.flag,
        version: before.version,
    }
}

/// Writes one validated deck inside an open transaction.
struct Installer<'a> {
    tx: &'a mut dyn Repositories,
    manifest: &'a DeckManifest,
    rng: &'a dyn Rng,
    at: TimestampMs,
    /// Position of the deck's first new card; imported positions are relative to it.
    base_position: i64,
}

fn guid(value: &str) -> AppResult<Guid> {
    Guid::parse(value).map_err(AppError::Validation)
}

impl Installer<'_> {
    fn run(mut self, source: &dyn ContentSource) -> AppResult<DeckOutcome> {
        let manifest = self.manifest;
        let mut outcome = DeckOutcome::default();
        let deck_guid = guid(&manifest.deck_guid)?;
        let kind = parse_note_type(&manifest.note_type)
            .ok_or(ValidationError::new("content", "deck.noteType"))?;
        let deck = match self.tx.deck_by_guid(&deck_guid)? {
            Some(deck) if deck.deleted_at.is_some() => {
                // The user deleted the deck; installing it again would need a restore.
                outcome.warnings.push(ContentWarning {
                    code: "deckDeleted",
                    deck_guid: manifest.deck_guid.clone(),
                    note_guid: None,
                    field: None,
                });
                return Ok(outcome);
            }
            Some(mut deck) => {
                // Name, description and type belong to the user once installed.
                if deck.content_version.as_deref() != Some(manifest.content_version.as_str()) {
                    deck.content_version = Some(manifest.content_version.clone());
                    deck = self.tx.update_deck(&deck, self.at)?;
                }
                deck
            }
            None => {
                let config = self.tx.default_config()?;
                outcome.deck_created = true;
                self.tx.insert_deck(
                    &NewDeck {
                        guid: deck_guid,
                        name: manifest.name.clone(),
                        parent_id: None,
                        description: manifest.description.clone(),
                        native_language: NativeLanguage::Pl,
                        target_language: parse_target_language(&manifest.target_language)
                            .ok_or(ValidationError::new("content", "language.target"))?,
                        deck_type: parse_deck_type(&manifest.deck_type)
                            .ok_or(ValidationError::new("content", "deck.type"))?,
                        config_id: config.id,
                        content_version: Some(manifest.content_version.clone()),
                        bundled: manifest.source == SourceKind::Bundled,
                    },
                    self.at,
                )?
            }
        };
        outcome.deck_id = Some(deck.id);
        let note_type = self.tx.note_type(kind)?;
        let mut position = self.tx.next_position()?;
        self.base_position = position;
        if manifest.source == SourceKind::Import {
            // Imported new cards keep their source order: base + their relative position.
            position += 1_000_000_000;
        }
        let mut shipped = HashSet::new();
        let reader = source
            .open(&manifest.notes_file)
            .map(langualinker_content::reader::NoteReader::new)
            .map_err(|error| AppError::Internal(format!("bundled notes unreadable: {error}")))?;
        for (_, record) in reader {
            // Validated before the transaction; a difference now means the source changed.
            let record = record.map_err(|error| {
                AppError::Validation(ValidationError::new("content", error.code))
            })?;
            shipped.insert(record.guid.clone());
            self.note(
                &record,
                kind,
                note_type.id,
                deck.id,
                &mut position,
                &mut outcome,
            )?;
        }
        self.removed(&shipped, &mut outcome)?;
        Ok(outcome)
    }

    fn note(
        &mut self,
        record: &NoteRecord,
        kind: NoteTypeKind,
        note_type_id: langualinker_domain::NoteTypeId,
        deck_id: DeckId,
        position: &mut i64,
        outcome: &mut DeckOutcome,
    ) -> AppResult<()> {
        let note_guid = guid(&record.guid)?;
        let fields = NoteFields {
            front: nfc(record.front.trim()),
            back: nfc(record.back.trim()),
        };
        let tags: Vec<String> = record.tags.iter().map(|tag| nfc(tag)).collect();
        let existing = self.tx.note_by_guid(&note_guid)?;
        let created = existing.is_none();
        let (note, overrides, mut changed) = match existing {
            Some(note) if note.deleted_at.is_some() => {
                outcome.unchanged += 1;
                return Ok(());
            }
            None => {
                let note = self.tx.insert_note(
                    &NewNote {
                        guid: note_guid,
                        note_type_id,
                        fields,
                        tags,
                    },
                    self.at,
                )?;
                (note, Vec::new(), true)
            }
            Some(mut note) => {
                let overrides = self.tx.content_overrides(note.id)?;
                let kept = |field: &str| overrides.iter().any(|item| item == field);
                let mut conflicts = Vec::new();
                let mut changed = false;
                for (field, current, bundled) in [
                    ("front", &mut note.fields.front, &fields.front),
                    ("back", &mut note.fields.back, &fields.back),
                ] {
                    if current != bundled {
                        if kept(field) {
                            conflicts.push(field.to_owned());
                        } else {
                            *current = bundled.clone();
                            changed = true;
                        }
                    }
                }
                if note.tags != tags {
                    if kept("tags") {
                        conflicts.push("tags".to_owned());
                    } else {
                        note.tags = tags;
                        changed = true;
                    }
                }
                if note.note_type_id != note_type_id && !kept("noteType") {
                    note.note_type_id = note_type_id;
                    changed = true;
                }
                for field in conflicts {
                    self.conflict(record, field, outcome);
                }
                let note = if changed {
                    self.tx.update_note(&note, self.at)?
                } else {
                    note
                };
                (note, overrides, changed)
            }
        };
        // One live card per template; scheduling of existing cards is never touched.
        let cards = self.tx.cards_of_note(note.id)?;
        for (ordinal, _) in kind.templates().iter().enumerate() {
            let ordinal = ordinal as u32;
            if !cards.iter().any(|card| card.template_ordinal == ordinal) {
                let card = self.tx.insert_card(
                    &NewCard {
                        guid: self.rng.guid(),
                        note_id: note.id,
                        deck_id,
                        template_ordinal: ordinal,
                        position: *position,
                    },
                    self.at,
                )?;
                *position += 1;
                changed = true;
                // Progress from an import package applies to cards created now only; a card
                // that already exists keeps the progress made in this app.
                if self.manifest.source == SourceKind::Import {
                    self.progress(record, card, outcome)?;
                }
            }
        }

        changed |= self.snippets(record, note.id, &overrides, outcome)?;
        if !overrides.iter().any(|field| field == "wordLinks") {
            changed |= self.links(record, note.id)?;
        }

        let previous = self.tx.bundled_note(note.id)?;
        if previous
            .as_ref()
            .is_some_and(|previous| previous.archived_by_import)
        {
            self.tx.set_note_cards_archived(note.id, None, self.at)?;
            changed = true;
        }
        let provenance = BundledNote {
            note_id: note.id,
            deck_guid: self.manifest.deck_guid.clone(),
            content_version: self.manifest.content_version.clone(),
            removed_in_version: None,
            archived_by_import: false,
        };
        if previous.as_ref() != Some(&provenance) {
            self.tx.put_bundled_note(&provenance)?;
        }
        if created {
            outcome.created += 1;
        } else if changed {
            outcome.updated += 1;
        } else {
            outcome.unchanged += 1;
        }
        Ok(())
    }

    /// Applies the source's scheduling state to a card just created and writes its history.
    fn progress(
        &mut self,
        record: &NoteRecord,
        card: Card,
        outcome: &mut DeckOutcome,
    ) -> AppResult<()> {
        let ordinal = card.template_ordinal;
        let initial = CardSnapshot::of(&card);
        let mut card = card;
        if let Some(state) = record.cards.iter().find(|item| item.ordinal == ordinal) {
            apply_card_record(&mut card, state, self.base_position);
            card = self.tx.update_card(&card, self.at)?;
            if card.state != CardState::New {
                outcome.scheduled += 1;
            }
        }
        // History, oldest first: before/after snapshots are reconstructed from the source rows
        // (the source keeps no snapshots); imported rows are never undoable.
        let mut before = initial;
        for review in record
            .reviews
            .iter()
            .filter(|review| review.ordinal == ordinal)
        {
            let after = replay(&before, review);
            self.tx.insert_review_log(&ReviewLog {
                id: ReviewLogId(0),
                card_id: card.id,
                session_id: None,
                operation_id: format!("import:{}:{}", card.guid.as_str(), review.reviewed_at_ms),
                rating: Rating::from_db_code(i64::from(review.rating))?,
                kind: match review.kind {
                    ReviewKindName::Learning => ReviewKind::Learning,
                    ReviewKindName::Review => ReviewKind::Review,
                    ReviewKindName::Relearning => ReviewKind::Relearning,
                    ReviewKindName::Filtered => ReviewKind::Filtered,
                    ReviewKindName::Manual => ReviewKind::Manual,
                },
                reviewed_at: TimestampMs(review.reviewed_at_ms),
                duration_ms: review.duration_ms,
                card_before: before.clone(),
                card_after: after.clone(),
                fsrs_version: IMPORTED_SCHEDULER.into(),
                parameters_hash: "none".into(),
                invalidated_at: None,
            })?;
            outcome.reviews += 1;
            before = after;
        }
        Ok(())
    }

    fn conflict(&self, record: &NoteRecord, field: String, outcome: &mut DeckOutcome) {
        outcome.warnings.push(ContentWarning {
            code: "overrideConflict",
            deck_guid: self.manifest.deck_guid.clone(),
            note_guid: Some(record.guid.clone()),
            field: Some(field),
        });
    }

    /// Associations and tips by GUID. Returns whether anything changed.
    fn snippets(
        &mut self,
        record: &NoteRecord,
        note: NoteId,
        overrides: &[String],
        outcome: &mut DeckOutcome,
    ) -> AppResult<bool> {
        let mut changed = false;
        for association in &record.associations {
            let association_guid = guid(&association.guid)?;
            let text = nfc(association.text.trim());
            match self.tx.association_by_guid(&association_guid)? {
                None => {
                    self.tx.insert_association(
                        &NewAssociation {
                            guid: association_guid,
                            note_id: note,
                            text,
                            provenance: AssociationProvenance::Bundled,
                            native_language: self.manifest.native_language.clone(),
                            target_language: self.manifest.target_language.clone(),
                            prompt_version: association.prompt_version.clone(),
                            model: association.model.clone(),
                            moderation: ModerationStatus::NotRequired,
                            cache_key: None,
                        },
                        self.at,
                    )?;
                    changed = true;
                }
                Some(existing) if existing.deleted_at.is_some() || existing.text == text => {}
                Some(existing) => {
                    let field = format!("association:{}", association.guid);
                    if overrides.contains(&field) {
                        self.conflict(record, field, outcome);
                    } else {
                        self.tx
                            .update_association_text(existing.id, &text, self.at)?;
                        changed = true;
                    }
                }
            }
        }
        for tip in &record.grammar_tips {
            let tip_guid = guid(&tip.guid)?;
            let text = nfc(tip.text.trim());
            match self.tx.grammar_tip_by_guid(&tip_guid)? {
                None => {
                    self.tx.insert_grammar_tip(
                        note,
                        &tip_guid,
                        &text,
                        TipProvenance::Bundled,
                        self.at,
                    )?;
                    changed = true;
                }
                Some(existing) if existing.deleted_at.is_some() || existing.text == text => {}
                Some(existing) => {
                    let field = format!("tip:{}", tip.guid);
                    if overrides.contains(&field) {
                        self.conflict(record, field, outcome);
                    } else {
                        self.tx
                            .update_grammar_tip_text(existing.id, &text, self.at)?;
                        changed = true;
                    }
                }
            }
        }
        Ok(changed)
    }

    /// Rebuilds the bundled links of a sentence note: resolved when the word note is installed,
    /// unresolved (lemma kept) until its deck is. Returns whether anything changed.
    fn links(&mut self, record: &NoteRecord, note: NoteId) -> AppResult<bool> {
        let mut desired = Vec::with_capacity(record.word_links.len());
        for link in &record.word_links {
            let word = self
                .tx
                .note_by_guid(&guid(&link.word_guid)?)?
                .filter(|word| word.deleted_at.is_none());
            desired.push(SentenceWordLink {
                id: WordLinkId(0),
                sentence_note_id: note,
                word_note_id: word.as_ref().map(|word| word.id),
                preferred_word_card_id: None,
                surface_form: nfc(&link.surface_form),
                lemma: Some(nfc(&link.lemma)),
                start_index: link.start,
                end_index: link.end,
                resolution: if word.is_some() {
                    WordLinkResolution::Resolved
                } else {
                    WordLinkResolution::Unresolved
                },
                created_at: self.at,
                updated_at: self.at,
            });
        }
        let current = self.tx.word_links(note)?;
        let key = |link: &SentenceWordLink| {
            (
                link.start_index,
                link.end_index,
                link.word_note_id,
                link.resolution,
                link.surface_form.clone(),
                link.lemma.clone(),
            )
        };
        let same = current.len() == desired.len()
            && current.iter().zip(&desired).all(|(a, b)| key(a) == key(b));
        if same {
            return Ok(false);
        }
        self.tx.replace_word_links(note, &desired, self.at)?;
        Ok(true)
    }

    /// Notes of earlier versions that this version no longer ships.
    fn removed(&mut self, shipped: &HashSet<String>, outcome: &mut DeckOutcome) -> AppResult<()> {
        let mut by_note = BTreeMap::new();
        for (note_guid, bundled) in self.tx.bundled_notes_of_deck(&self.manifest.deck_guid)? {
            if !shipped.contains(note_guid.as_str()) && bundled.removed_in_version.is_none() {
                by_note.insert(bundled.note_id, bundled);
            }
        }
        for (note, mut bundled) in by_note {
            let keep =
                self.tx.note_has_progress(note)? || !self.tx.content_overrides(note)?.is_empty();
            bundled.removed_in_version = Some(self.manifest.content_version.clone());
            if keep {
                outcome.kept += 1;
            } else {
                self.tx
                    .set_note_cards_archived(note, Some(self.at), self.at)?;
                bundled.archived_by_import = true;
                outcome.archived += 1;
            }
            self.tx.put_bundled_note(&bundled)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use langualinker_content::format::ReviewRecord;
    use langualinker_domain::CardId;

    use super::*;

    fn card(state: CardState) -> Card {
        Card {
            id: CardId(1),
            guid: Guid::parse("card-1").unwrap(),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            template_ordinal: 0,
            position: 42,
            flag: CardFlag::None,
            state,
            queue: QueueState::natural(state, false),
            due_at: None,
            fsrs: CardFsrsState::NEW,
            legacy_ease: None,
            reps: 0,
            lapses: 0,
            learning_step: Some(2),
            buried_until: None,
            archived_at: None,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 1,
        }
    }

    fn snapshot(state: CardState) -> CardSnapshot {
        CardSnapshot {
            state,
            queue: QueueState::natural(state, false),
            due_at: None,
            position: 3,
            fsrs: CardFsrsState::NEW,
            legacy_ease: Some(2500),
            reps: 4,
            lapses: 1,
            learning_step: None,
            buried_until: None,
            flag: CardFlag::None,
            version: 2,
        }
    }

    fn card_record(state: CardStateName, queue: QueueName) -> CardRecord {
        CardRecord {
            ordinal: 0,
            state,
            queue,
            due_at_ms: None,
            position: None,
            scheduled_days: 0,
            stability: None,
            difficulty: None,
            ease: None,
            reps: 0,
            lapses: 0,
            last_review_at_ms: None,
            flag: 0,
        }
    }

    fn review(kind: ReviewKindName, rating: u8, scheduled_days: i64) -> ReviewRecord {
        ReviewRecord {
            ordinal: 0,
            reviewed_at_ms: 10 * DAY_MS,
            rating,
            kind,
            scheduled_days,
            ease: None,
            duration_ms: 5_000,
        }
    }

    fn warning(index: usize) -> ContentWarning {
        ContentWarning {
            code: "overrideConflict",
            deck_guid: format!("d{index}"),
            note_guid: None,
            field: None,
        }
    }

    #[test]
    fn outcome_counts_add_up_across_decks() {
        let mut outcome = PrepareOutcome::default();
        outcome.add(DeckOutcome {
            deck_id: Some(DeckId(1)),
            deck_created: true,
            created: 2,
            unchanged: 1,
            ..Default::default()
        });
        outcome.add(DeckOutcome {
            deck_id: Some(DeckId(2)),
            updated: 3,
            kept: 1,
            archived: 1,
            ..Default::default()
        });
        assert_eq!(
            (
                outcome.created,
                outcome.updated,
                outcome.unchanged,
                outcome.kept,
                outcome.archived
            ),
            (2, 3, 1, 1, 1)
        );
        assert_eq!(outcome.deck_ids, vec![DeckId(1), DeckId(2)]);
    }

    #[test]
    fn deck_without_changes_is_not_listed() {
        let mut outcome = PrepareOutcome::default();
        outcome.add(DeckOutcome {
            deck_id: Some(DeckId(5)),
            unchanged: 9,
            ..Default::default()
        });
        assert!(outcome.deck_ids.is_empty());
        assert_eq!(outcome.unchanged, 9);
    }

    #[test]
    fn newly_created_empty_deck_is_listed() {
        let mut outcome = PrepareOutcome::default();
        outcome.add(DeckOutcome {
            deck_id: Some(DeckId(5)),
            deck_created: true,
            ..Default::default()
        });
        assert_eq!(outcome.deck_ids, vec![DeckId(5)]);
    }

    #[test]
    fn deck_is_listed_once_when_reported_twice() {
        let mut outcome = PrepareOutcome::default();
        for _ in 0..2 {
            outcome.add(DeckOutcome {
                deck_id: Some(DeckId(7)),
                created: 1,
                ..Default::default()
            });
        }
        assert_eq!(outcome.deck_ids, vec![DeckId(7)]);
        assert_eq!(outcome.created, 2);
    }

    #[test]
    fn warnings_are_capped_and_the_rest_counted() {
        let mut outcome = PrepareOutcome::default();
        outcome.add(DeckOutcome {
            warnings: (0..MAX_WARNINGS + 50).map(warning).collect(),
            ..Default::default()
        });
        assert_eq!(outcome.warnings.len(), MAX_WARNINGS);
        assert_eq!(outcome.warnings_dropped, 50);
        assert_eq!(outcome.warnings[0].deck_guid, "d0");
    }

    #[test]
    fn record_states_map_to_domain_states() {
        assert_eq!(card_state(CardStateName::New), CardState::New);
        assert_eq!(card_state(CardStateName::Learning), CardState::Learning);
        assert_eq!(card_state(CardStateName::Review), CardState::Review);
        assert_eq!(card_state(CardStateName::Relearning), CardState::Relearning);
    }

    #[test]
    fn record_queues_map_to_domain_queues() {
        assert_eq!(queue_state(QueueName::New), QueueState::New);
        assert_eq!(queue_state(QueueName::Learning), QueueState::Learning);
        assert_eq!(queue_state(QueueName::Review), QueueState::Review);
        assert_eq!(queue_state(QueueName::DayLearn), QueueState::DayLearn);
        assert_eq!(queue_state(QueueName::Suspended), QueueState::Suspended);
        assert_eq!(queue_state(QueueName::SchedBuried), QueueState::SchedBuried);
        assert_eq!(queue_state(QueueName::UserBuried), QueueState::UserBuried);
    }

    #[test]
    fn new_card_takes_its_position_relative_to_the_deck_base() {
        let mut target = card(CardState::New);
        let mut record = card_record(CardStateName::New, QueueName::New);
        record.position = Some(7);
        apply_card_record(&mut target, &record, 1_000);
        assert_eq!(target.state, CardState::New);
        assert_eq!(target.queue, QueueState::New);
        assert_eq!(target.position, 1_007);
        assert_eq!(target.due_at, None);
    }

    #[test]
    fn scheduled_card_keeps_its_position_and_copies_progress() {
        let mut target = card(CardState::New);
        let record = CardRecord {
            due_at_ms: Some(5_000),
            position: Some(7),
            scheduled_days: 10,
            stability: Some(4.5),
            difficulty: Some(5.0),
            ease: Some(2_300),
            reps: 9,
            lapses: 2,
            last_review_at_ms: Some(4_000),
            flag: 3,
            ..card_record(CardStateName::Review, QueueName::Review)
        };
        apply_card_record(&mut target, &record, 1_000);
        assert_eq!(target.state, CardState::Review);
        assert_eq!(target.queue, QueueState::Review);
        assert_eq!(target.position, 42);
        assert_eq!(target.due_at, Some(TimestampMs(5_000)));
        assert_eq!(target.fsrs.stability, Some(4.5));
        assert_eq!(target.fsrs.difficulty, Some(5.0));
        assert_eq!(target.fsrs.scheduled_days, 10);
        assert_eq!(target.fsrs.last_review_at, Some(TimestampMs(4_000)));
        assert_eq!(target.legacy_ease, Some(2_300));
        assert_eq!(target.reps, 9);
        assert_eq!(target.lapses, 2);
        assert_eq!(target.flag, CardFlag::Green);
    }

    #[test]
    fn learning_step_restarts_and_unknown_flag_falls_back_to_none() {
        let mut target = card(CardState::Learning);
        assert_eq!(target.learning_step, Some(2));
        let record = CardRecord {
            flag: 9,
            ..card_record(CardStateName::Learning, QueueName::Learning)
        };
        apply_card_record(&mut target, &record, 0);
        assert_eq!(target.learning_step, None);
        assert_eq!(target.flag, CardFlag::None);
    }

    #[test]
    fn good_review_schedules_the_card_and_counts_elapsed_days() {
        let mut before = snapshot(CardState::Learning);
        before.fsrs.last_review_at = Some(TimestampMs(2 * DAY_MS));
        let after = replay(&before, &review(ReviewKindName::Learning, 3, 3));
        assert_eq!(after.state, CardState::Review);
        assert_eq!(after.queue, QueueState::Review);
        assert_eq!(after.due_at, Some(TimestampMs(13 * DAY_MS)));
        assert_eq!(after.fsrs.scheduled_days, 3);
        assert_eq!(after.fsrs.elapsed_days, 8);
        assert_eq!(after.fsrs.last_review_at, Some(TimestampMs(10 * DAY_MS)));
        assert_eq!(after.reps, 5);
        assert_eq!(after.lapses, 1);
        assert_eq!(after.position, 3);
        assert_eq!(after.version, 2);
    }

    #[test]
    fn lapse_counts_and_falls_back_to_the_previous_ease() {
        let before = snapshot(CardState::Review);
        let after = replay(&before, &review(ReviewKindName::Review, 1, 0));
        assert_eq!(after.state, CardState::Relearning);
        assert_eq!(after.lapses, 2);
        assert_eq!(after.legacy_ease, Some(2_500));
    }

    #[test]
    fn intraday_learning_step_is_not_a_lapse() {
        let before = snapshot(CardState::Learning);
        let after = replay(&before, &review(ReviewKindName::Learning, 2, 0));
        assert_eq!(after.state, CardState::Learning);
        assert_eq!(after.lapses, 1);
        assert_eq!(after.fsrs.elapsed_days, 0);
    }

    #[test]
    fn failed_relearning_review_is_not_counted_as_a_lapse() {
        let before = snapshot(CardState::Relearning);
        let after = replay(&before, &review(ReviewKindName::Relearning, 1, 0));
        assert_eq!(after.state, CardState::Relearning);
        assert_eq!(after.lapses, 1);
    }

    #[test]
    fn content_error_names_the_first_error_only() {
        let mut issues = Issues::default();
        issues.error("deck.noteType", "deck.json", Some(3), "unknown type");
        issues.error("front.empty", "notes.ndjson", Some(9), "empty");
        assert_eq!(
            content_error(&issues),
            AppError::Validation(ValidationError::new("content", "deck.noteType"))
        );
    }

    #[test]
    fn content_error_without_issues_is_generic() {
        assert_eq!(
            content_error(&Issues::default()),
            AppError::Validation(ValidationError::new("content", "invalid"))
        );
    }

    #[test]
    fn guid_accepts_bundled_ids_and_rejects_others() {
        assert!(guid("de-word_01").is_ok());
        assert_eq!(
            guid("bad id!"),
            Err(AppError::Validation(ValidationError::new("guid", "format")))
        );
        assert!(guid("").is_err());
    }
}
