//! Study use cases (plan Phase 9 step 3, ADR-15): start, resume and change a session, due
//! counts, rating preview, rating, undo, today's limit extension and session statistics.
//!
//! Every function runs inside the caller's transaction. Write use cases must run in a write
//! transaction; so must [`due_counts`], because it first returns expired buried cards to their
//! queues. [`study_view`], [`preview`] and [`session_stats`] only read.
//!
//! Policies:
//! - Ratings use the deck's current preset. The preview token covers the card version, the preset
//!   version and parameters hash, the session and the study day, so a rating made against an
//!   outdated preview fails with `PREVIEW_STALE` and the UI fetches a new preview.
//! - A retried `operationId` returns the first result instead of rating twice.
//! - Undo reverses only the latest rating of the session, and only while neither the card, its
//!   buried siblings nor a leech-tagged note changed since.
//! - Counters come from committed, valid review logs; an undone rating stops counting at once.

use std::collections::{HashMap, HashSet};

use langualinker_domain::calendar::StudyCalendar;
use langualinker_domain::card::{Card, CardState, QueueState};
use langualinker_domain::deck::{Deck, DeckConfig, LeechAction, NewCardOrder};
use langualinker_domain::fsrs::{FSRS_VERSION, SchedulerConfig, is_leech};
use langualinker_domain::queue::{
    Candidate, Candidates, DailyLimits, DeckNode, LimitTree, QueueCounts, QueuePlan,
};
use langualinker_domain::review::{CardSnapshot, Rating, ReviewLog};
use langualinker_domain::scheduling::{self, StudyMoment};
use langualinker_domain::{
    CardId, DeckConfigId, DeckId, NoteId, ReviewLogId, SessionId, TimestampMs, ValidationError,
    validation,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::repositories::{
    NewStudySession, RecordedOperation, Repositories, SessionItemStatus, SessionStatus,
    SiblingChange, StudySession, UndoChanges,
};
use crate::{AppError, AppResult};

/// Longest answer time counted per rating (Anki's default cap).
pub const MAX_ANSWER_MS: i64 = 60_000;
/// Tag added to a note whose card became a leech (preset action `Tag`).
pub const LEECH_TAG: &str = "leech";
/// Most cards a session over chosen cards may contain.
pub const MAX_CHOSEN_CARDS: usize = 1_000;
/// Setting with today's limit extensions: `{"day": n, "decks": {"<deckId>": [new, review]}}`.
pub const LIMIT_EXTENSIONS_KEY: &str = "study.limitExtensions";
/// Largest extension of one limit in one request.
pub const MAX_LIMIT_EXTENSION: u32 = 9_999;

const OP_START: &str = "startStudy";
const OP_UNDO: &str = "undoReview";
const OP_EXTEND: &str = "extendTodayLimits";

// ---------------------------------------------------------------- inputs and results

#[derive(Debug, Clone, PartialEq)]
pub struct StartStudy {
    pub operation_id: String,
    /// `None` studies all decks.
    pub deck_id: Option<DeckId>,
    /// Studies only these cards, in this order and without daily limits.
    pub card_ids: Option<Vec<CardId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Pause,
    Resume,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateCard {
    pub operation_id: String,
    pub session_id: SessionId,
    pub card_id: CardId,
    pub card_version: i64,
    pub preview_token: String,
    pub rating: Rating,
    /// Active answer time measured by the UI; capped at [`MAX_ANSWER_MS`].
    pub answer_ms: i64,
}

/// The queue is empty because a daily limit was reached while more cards exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LimitReached {
    pub new: bool,
    pub review: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StudyView {
    pub session: StudySession,
    /// Card to show now; `None` when nothing is due now.
    pub current: Option<Card>,
    pub counts: QueueCounts,
    /// Valid ratings in this session.
    pub reviewed_count: u32,
    pub undo_available: bool,
    /// The next learning card due later today, when nothing is due now.
    pub next_learning_due_at: Option<TimestampMs>,
    pub limit_reached: LimitReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewedCard {
    pub card_id: CardId,
    pub version: i64,
    pub state: CardState,
    pub due_at: Option<TimestampMs>,
}

impl ReviewedCard {
    fn of(card: &Card) -> Self {
        Self {
            card_id: card.id,
            version: card.version,
            state: card.state,
            due_at: card.due_at,
        }
    }

    fn of_snapshot(card_id: CardId, snapshot: &CardSnapshot) -> Self {
        Self {
            card_id,
            version: snapshot.version,
            state: snapshot.state,
            due_at: snapshot.due_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateOutcome {
    pub operation_id: String,
    pub reviewed: ReviewedCard,
    pub view: StudyView,
    /// The operation had already been committed; nothing was rated again.
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoOutcome {
    pub operation_id: String,
    pub restored: ReviewedCard,
    pub view: StudyView,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalPreview {
    pub rating: Rating,
    pub due_at: TimestampMs,
    pub interval_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RatingPreview {
    pub card_id: CardId,
    pub card_version: i64,
    pub token: String,
    /// Again, Hard, Good, Easy.
    pub intervals: Vec<IntervalPreview>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStats {
    pub session: StudySession,
    /// Valid ratings by button: Again, Hard, Good, Easy.
    pub ratings: [u32; 4],
    pub reviewed_count: u32,
    /// Cards rated Again at least once, in first-failure order.
    pub again_cards: Vec<CardId>,
    pub next_due_at: Option<TimestampMs>,
    /// Cards of the session's scope still due today.
    pub remaining: QueueCounts,
}

// ---------------------------------------------------------------- helpers

/// "Now" in the collection calendar. An unknown stored zone falls back to UTC.
pub fn study_moment(tx: &mut dyn Repositories, now: TimestampMs) -> AppResult<StudyMoment> {
    let (zone, cutoff) = tx.collection_calendar()?;
    let calendar = StudyCalendar::new(&zone, cutoff).unwrap_or_else(|_| {
        tracing::warn!("unknown collection time zone, using UTC");
        StudyCalendar::utc(cutoff)
    });
    Ok(StudyMoment::new(calendar, now))
}

fn check_operation_id(operation_id: &str) -> AppResult<()> {
    if operation_id.is_empty() || operation_id.len() > 128 {
        return Err(ValidationError::new("operationId", "format").into());
    }
    Ok(())
}

fn reused() -> AppError {
    ValidationError::new("operationId", "reused").into()
}

/// Fails when a rating already used this operation id (ratings are keyed by their review log).
fn not_a_rating(tx: &mut dyn Repositories, operation_id: &str) -> AppResult<()> {
    if tx.review_log_by_operation(operation_id)?.is_some() {
        return Err(reused());
    }
    Ok(())
}

fn studyable(card: &Card) -> bool {
    card.deleted_at.is_none()
        && card.archived_at.is_none()
        && !matches!(
            card.queue,
            QueueState::Suspended | QueueState::SchedBuried | QueueState::UserBuried
        )
}

fn candidate(card: &Card) -> Candidate {
    Candidate {
        card_id: card.id,
        note_id: card.note_id,
        deck_id: card.deck_id,
        due_at: card.due_at,
    }
}

fn load_session(tx: &mut dyn Repositories, id: SessionId) -> AppResult<StudySession> {
    tx.session(id)?
        .ok_or(AppError::NotFound { entity: "session" })
}

/// The card a rating or preview targets, at the version the UI showed.
fn load_studyable_card(tx: &mut dyn Repositories, id: CardId, version: i64) -> AppResult<Card> {
    let card = tx.card(id)?.ok_or(AppError::NotFound { entity: "card" })?;
    if card.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "card" });
    }
    if card.version != version {
        return Err(AppError::VersionConflict {
            entity: "card",
            current: card.version,
        });
    }
    if !studyable(&card) {
        return Err(ValidationError::new("cardId", "notStudyable").into());
    }
    Ok(card)
}

/// Preset of the card's deck and the scheduler built from it.
fn scheduler_for(
    tx: &mut dyn Repositories,
    card: &Card,
) -> AppResult<(DeckConfig, SchedulerConfig, String)> {
    let deck = tx
        .deck(card.deck_id)?
        .ok_or(AppError::NotFound { entity: "deck" })?;
    let config = tx
        .config(deck.config_id)?
        .ok_or(AppError::NotFound { entity: "preset" })?;
    let scheduler = SchedulerConfig::from_options(&config.options)?;
    let hash = scheduler.parameters_hash();
    Ok((config, scheduler, hash))
}

/// Token binding a preview to everything that could change its intervals.
fn preview_token(
    card: &Card,
    config: &DeckConfig,
    parameters_hash: &str,
    session: SessionId,
    moment: &StudyMoment,
) -> String {
    let text = format!(
        "preview-v1|{}|{}|{}|{}|{}|{}|{}",
        card.id.0,
        card.version,
        config.id.0,
        config.version,
        parameters_hash,
        session.0,
        moment.today.0
    );
    Sha256::digest(text.as_bytes())[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Active decks with their presets.
struct DeckTree {
    decks: Vec<Deck>,
    configs: HashMap<DeckConfigId, DeckConfig>,
}

impl DeckTree {
    fn load(tx: &mut dyn Repositories) -> AppResult<Self> {
        let decks = tx.active_decks()?;
        let mut configs = HashMap::new();
        for deck in &decks {
            if let std::collections::hash_map::Entry::Vacant(entry) = configs.entry(deck.config_id)
            {
                let config = tx
                    .config(deck.config_id)?
                    .ok_or(AppError::NotFound { entity: "preset" })?;
                entry.insert(config);
            }
        }
        Ok(Self { decks, configs })
    }

    fn config_of(&self, deck: DeckId) -> Option<&DeckConfig> {
        self.decks
            .iter()
            .find(|candidate| candidate.id == deck)
            .and_then(|deck| self.configs.get(&deck.config_id))
    }

    /// Decks of the scope in tree order (siblings by name); all trees when `scope` is `None`.
    fn preorder(&self, scope: Option<DeckId>) -> Vec<DeckId> {
        let mut children: HashMap<Option<DeckId>, Vec<&Deck>> = HashMap::new();
        let ids: HashSet<DeckId> = self.decks.iter().map(|deck| deck.id).collect();
        for deck in &self.decks {
            // A deck whose parent is not active is treated as a root.
            let parent = deck.parent_id.filter(|parent| ids.contains(parent));
            children.entry(parent).or_default().push(deck);
        }
        for list in children.values_mut() {
            list.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        }
        let mut order = Vec::new();
        let mut stack: Vec<DeckId> = match scope {
            Some(deck) if ids.contains(&deck) => vec![deck],
            Some(_) => Vec::new(),
            None => children
                .get(&None)
                .map(|roots| roots.iter().rev().map(|deck| deck.id).collect())
                .unwrap_or_default(),
        };
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            if let Some(list) = children.get(&Some(id)) {
                stack.extend(list.iter().rev().map(|deck| deck.id));
            }
        }
        order
    }

    fn limit_tree(
        &self,
        tx: &mut dyn Repositories,
        scope: Option<DeckId>,
        moment: &StudyMoment,
    ) -> AppResult<LimitTree> {
        let extensions = load_extensions(tx, moment)?;
        let ids: HashSet<DeckId> = self.decks.iter().map(|deck| deck.id).collect();
        let nodes: Vec<DeckNode> = self
            .decks
            .iter()
            .map(|deck| {
                let options = &self.configs[&deck.config_id].options;
                let extra = extensions.get(&deck.id).copied().unwrap_or_default();
                DeckNode {
                    id: deck.id,
                    parent: deck.parent_id.filter(|parent| ids.contains(parent)),
                    limits: DailyLimits {
                        new: options.new_per_day.saturating_add(extra.new),
                        review: options.reviews_per_day.saturating_add(extra.review),
                    },
                    bury_siblings: options.bury_siblings,
                }
            })
            .collect();
        let done = tx.done_since(moment.day_start)?.into_iter().collect();
        Ok(LimitTree::new(&nodes, &done, scope))
    }
}

fn load_extensions(
    tx: &mut dyn Repositories,
    moment: &StudyMoment,
) -> AppResult<HashMap<DeckId, DailyLimits>> {
    let Some(json) = tx.setting(LIMIT_EXTENSIONS_KEY)? else {
        return Ok(HashMap::new());
    };
    let value: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    if value["day"].as_i64() != Some(moment.today.0) {
        return Ok(HashMap::new());
    }
    let mut result = HashMap::new();
    if let Some(decks) = value["decks"].as_object() {
        for (id, pair) in decks {
            let (Ok(id), Some(new), Some(review)) =
                (id.parse::<i64>(), pair[0].as_u64(), pair[1].as_u64())
            else {
                continue;
            };
            result.insert(
                DeckId(id),
                DailyLimits {
                    new: u32::try_from(new).unwrap_or(u32::MAX),
                    review: u32::try_from(review).unwrap_or(u32::MAX),
                },
            );
        }
    }
    Ok(result)
}

/// Today's queue of a scope, plus whether limits hide more cards.
fn plan_for_scope(
    tx: &mut dyn Repositories,
    scope: Option<DeckId>,
    moment: &StudyMoment,
) -> AppResult<(QueuePlan, LimitReached)> {
    let tree = DeckTree::load(tx)?;
    let limits = tree.limit_tree(tx, scope, moment)?;
    let scope_decks = tree.preorder(scope);
    let learning = tx.learning_candidates(&scope_decks, moment.day_end)?;
    let mut reviews = Vec::new();
    for deck in &scope_decks {
        let room = limits.room(*deck).review;
        if room > 0 {
            reviews.extend(tx.review_candidates(*deck, moment.day_end, room)?);
        }
    }
    reviews.sort_by_key(|card| (card.due_at, card.card_id));
    let mut news = Vec::new();
    for deck in &scope_decks {
        let room = limits.room(*deck).new;
        if room > 0 {
            let order = tree
                .config_of(*deck)
                .map_or(NewCardOrder::Position, |config| {
                    config.options.new_card_order
                });
            news.extend(tx.new_candidates(*deck, order, moment.today.0, room)?);
        }
    }
    let seen: HashSet<NoteId> = tx
        .notes_reviewed_since(moment.day_start)?
        .into_iter()
        .collect();
    let plan = QueuePlan::build(
        Candidates {
            learning,
            reviews,
            news,
        },
        limits,
        &seen,
        moment.day_start,
    );
    // Limits only explain an empty list when cards would otherwise be there.
    let mut reached = LimitReached::default();
    if plan.news.is_empty() {
        for deck in &scope_decks {
            if !tx
                .new_candidates(*deck, NewCardOrder::Position, moment.today.0, 1)?
                .is_empty()
            {
                reached.new = true;
                break;
            }
        }
    }
    if plan.reviews.is_empty() {
        for deck in &scope_decks {
            if !tx.review_candidates(*deck, moment.day_end, 1)?.is_empty() {
                reached.review = true;
                break;
            }
        }
    }
    Ok((plan, reached))
}

/// Queue of a session over chosen cards: its pending items plus learning re-entries.
fn plan_for_chosen(
    tx: &mut dyn Repositories,
    session: &StudySession,
    moment: &StudyMoment,
) -> AppResult<QueuePlan> {
    let items = tx.session_items(session.id)?;
    let members: HashSet<CardId> = items.iter().map(|item| item.card_id).collect();
    let decks: Vec<DeckId> = tx.active_decks()?.iter().map(|deck| deck.id).collect();
    let learning: Vec<Candidate> = tx
        .learning_candidates(&decks, moment.day_end)?
        .into_iter()
        .filter(|card| members.contains(&card.card_id))
        .collect();
    let mut pending = Vec::new();
    for item in items
        .iter()
        .filter(|item| item.status == SessionItemStatus::Pending)
    {
        if let Some(card) = tx.card(item.card_id)?
            && studyable(&card)
        {
            pending.push(candidate(&card));
        }
    }
    Ok(QueuePlan::fixed(learning, pending, moment.day_start))
}

fn plan_for_session(
    tx: &mut dyn Repositories,
    session: &StudySession,
    moment: &StudyMoment,
) -> AppResult<(QueuePlan, LimitReached)> {
    if session.restricted {
        Ok((
            plan_for_chosen(tx, session, moment)?,
            LimitReached::default(),
        ))
    } else {
        plan_for_scope(tx, session.deck_id, moment)
    }
}

fn plan_contains(plan: &QueuePlan, card: CardId) -> bool {
    plan.learning
        .iter()
        .chain(&plan.reviews)
        .chain(&plan.news)
        .any(|candidate| candidate.card_id == card)
}

/// Keeps the session's current card when it is still in the queue; otherwise picks the next one.
fn choose_current(
    tx: &mut dyn Repositories,
    session: &StudySession,
    plan: &QueuePlan,
    moment: &StudyMoment,
    keep: bool,
) -> AppResult<Option<Card>> {
    if session.status == SessionStatus::Finished {
        return Ok(None);
    }
    if keep
        && let Some(id) = session.current_card_id
        && let Some(card) = tx.card(id)?
        && studyable(&card)
        && plan_contains(plan, id)
    {
        return Ok(Some(card));
    }
    let answered = tx.session_review_count(session.id)?;
    match plan.next(moment.now, answered) {
        Some((next, _)) => tx.card(next.card_id),
        None => Ok(None),
    }
}

fn build_view(
    tx: &mut dyn Repositories,
    session: StudySession,
    current: Option<Card>,
    plan: &QueuePlan,
    limit_reached: LimitReached,
    moment: &StudyMoment,
) -> AppResult<StudyView> {
    let reviewed_count = tx.session_review_count(session.id)?;
    let undo_available = tx
        .last_undo_entry(session.id)?
        .is_some_and(|entry| entry.consumed_at.is_none());
    let next_learning_due_at = if current.is_none() {
        plan.next_learning_due(moment.now)
    } else {
        None
    };
    Ok(StudyView {
        session,
        current,
        counts: plan.counts(),
        reviewed_count,
        undo_available,
        next_learning_due_at,
        limit_reached,
    })
}

/// Picks the current card, persists it with the session and returns the view. A session with
/// nothing left today (no learning card either) is finished.
fn refresh(
    tx: &mut dyn Repositories,
    session: &mut StudySession,
    moment: &StudyMoment,
    keep_current: bool,
) -> AppResult<StudyView> {
    let (plan, reached) = plan_for_session(tx, session, moment)?;
    let current = choose_current(tx, session, &plan, moment, keep_current)?;
    if current.is_none() && plan.learning.is_empty() && session.status != SessionStatus::Finished {
        session.status = SessionStatus::Finished;
        session.finished_at = Some(moment.now);
        session.paused_at = None;
    }
    session.current_card_id = current.as_ref().map(|card| card.id);
    tx.update_session(session, moment.now)?;
    build_view(tx, session.clone(), current, &plan, reached, moment)
}

/// Returns buried cards whose day has passed to their natural queue. Returns how many changed.
pub fn unbury_expired(tx: &mut dyn Repositories, moment: &StudyMoment) -> AppResult<u32> {
    let mut changed = 0;
    for id in tx.expired_buried_cards(moment.now)? {
        let Some(mut card) = tx.card(id)? else {
            continue;
        };
        if scheduling::unbury_if_expired(&mut card, moment) {
            tx.update_card(&card, moment.now)?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Adds the leech tag to a note. Returns the note version it left, or `None` when the note
/// already had the tag or cannot take another one.
fn tag_leech(tx: &mut dyn Repositories, note: NoteId, at: TimestampMs) -> AppResult<Option<i64>> {
    let Some(mut note) = tx.note(note)? else {
        return Ok(None);
    };
    if note.deleted_at.is_some()
        || note
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(LEECH_TAG))
    {
        return Ok(None);
    }
    let mut tags = note.tags.clone();
    tags.push(LEECH_TAG.to_owned());
    if validation::tags(&tags).is_err() {
        tracing::warn!("leech tag skipped: the note has no room for another tag");
        return Ok(None);
    }
    note.tags = tags;
    Ok(Some(tx.update_note(&note, at)?.version))
}

fn session_contains(
    tx: &mut dyn Repositories,
    session: &StudySession,
    card: &Card,
) -> AppResult<bool> {
    if session.restricted {
        return Ok(tx
            .session_items(session.id)?
            .iter()
            .any(|item| item.card_id == card.id));
    }
    let tree = DeckTree::load(tx)?;
    Ok(tree.preorder(session.deck_id).contains(&card.deck_id))
}

// ---------------------------------------------------------------- use cases

/// StartStudy: finishes any open session and starts a new one. Retrying the operation returns
/// the session it started.
pub fn start_study(
    tx: &mut dyn Repositories,
    input: &StartStudy,
    now: TimestampMs,
) -> AppResult<StudyView> {
    check_operation_id(&input.operation_id)?;
    if let Some(recorded) = tx.recorded_operation(&input.operation_id)? {
        if recorded.operation != OP_START {
            return Err(reused());
        }
        let id: i64 = serde_json::from_str(&recorded.result_json)
            .map_err(|_| AppError::Internal("unreadable startStudy result".into()))?;
        return study_view(tx, SessionId(id), now);
    }
    not_a_rating(tx, &input.operation_id)?;
    let (config_id, config_version) = match input.deck_id {
        Some(id) => {
            let deck = tx.deck(id)?.ok_or(AppError::NotFound { entity: "deck" })?;
            if deck.deleted_at.is_some() {
                return Err(AppError::Deleted { entity: "deck" });
            }
            let config = tx
                .config(deck.config_id)?
                .ok_or(AppError::NotFound { entity: "preset" })?;
            (Some(config.id), Some(config.version))
        }
        None => (None, None),
    };
    let chosen = match &input.card_ids {
        None => None,
        Some(ids) => {
            if ids.is_empty() || ids.len() > MAX_CHOSEN_CARDS {
                return Err(ValidationError::new("cardIds", "count").into());
            }
            let mut unique = Vec::with_capacity(ids.len());
            let mut seen = HashSet::new();
            for id in ids {
                if !seen.insert(*id) {
                    continue;
                }
                match tx.card(*id)? {
                    Some(card) if studyable(&card) => unique.push(*id),
                    _ => return Err(ValidationError::new("cardIds", "notStudyable").into()),
                }
            }
            Some(unique)
        }
    };
    let moment = study_moment(tx, now)?;
    if let Some(mut open) = tx.open_session()? {
        open.status = SessionStatus::Finished;
        open.finished_at = Some(now);
        open.paused_at = None;
        open.current_card_id = None;
        tx.update_session(&open, now)?;
    }
    unbury_expired(tx, &moment)?;
    let mut session = tx.insert_session(
        &NewStudySession {
            deck_id: input.deck_id,
            config_id,
            config_version,
            restricted: chosen.is_some(),
        },
        now,
    )?;
    for id in chosen.iter().flatten() {
        tx.put_session_item(session.id, *id, SessionItemStatus::Pending)?;
    }
    let view = refresh(tx, &mut session, &moment, false)?;
    tx.record_operation(
        &input.operation_id,
        &RecordedOperation {
            operation: OP_START.into(),
            result_json: session.id.0.to_string(),
        },
        now,
    )?;
    Ok(view)
}

/// ResumeStudy: the open session, made active again, or `None` when there is none.
pub fn resume_study(tx: &mut dyn Repositories, now: TimestampMs) -> AppResult<Option<StudyView>> {
    let Some(mut session) = tx.open_session()? else {
        return Ok(None);
    };
    let moment = study_moment(tx, now)?;
    unbury_expired(tx, &moment)?;
    if session.status == SessionStatus::Paused {
        session.status = SessionStatus::Active;
        session.paused_at = None;
    }
    refresh(tx, &mut session, &moment, true).map(Some)
}

/// Pauses, resumes or ends a session. Repeating an action changes nothing.
pub fn change_session(
    tx: &mut dyn Repositories,
    operation_id: &str,
    session_id: SessionId,
    action: SessionAction,
    now: TimestampMs,
) -> AppResult<StudyView> {
    check_operation_id(operation_id)?;
    let mut session = load_session(tx, session_id)?;
    let finished = session.status == SessionStatus::Finished;
    match action {
        SessionAction::Pause | SessionAction::Resume if finished => {
            return Err(ValidationError::new("sessionId", "finished").into());
        }
        SessionAction::Pause => {
            if session.status == SessionStatus::Active {
                session.status = SessionStatus::Paused;
                session.paused_at = Some(now);
            }
        }
        SessionAction::Resume => {
            session.status = SessionStatus::Active;
            session.paused_at = None;
        }
        SessionAction::End => {
            if !finished {
                session.status = SessionStatus::Finished;
                session.finished_at = Some(now);
                session.paused_at = None;
            }
        }
    }
    let moment = study_moment(tx, now)?;
    refresh(tx, &mut session, &moment, true)
}

/// The session as the study screen shows it. Read-only.
pub fn study_view(
    tx: &mut dyn Repositories,
    session_id: SessionId,
    now: TimestampMs,
) -> AppResult<StudyView> {
    let session = load_session(tx, session_id)?;
    let moment = study_moment(tx, now)?;
    let (plan, reached) = plan_for_session(tx, &session, &moment)?;
    let current = choose_current(tx, &session, &plan, &moment, true)?;
    build_view(tx, session, current, &plan, reached, &moment)
}

/// GetDue: today's counts of a deck (with subdecks) or of all decks, within the daily limits.
/// Needs a write transaction (expired burials are lifted first).
pub fn due_counts(
    tx: &mut dyn Repositories,
    deck: Option<DeckId>,
    now: TimestampMs,
) -> AppResult<QueueCounts> {
    let moment = study_moment(tx, now)?;
    unbury_expired(tx, &moment)?;
    Ok(plan_for_scope(tx, deck, &moment)?.0.counts())
}

/// Preview of the four ratings of a card, with the token the rating must present.
pub fn preview(
    tx: &mut dyn Repositories,
    session_id: SessionId,
    card_id: CardId,
    card_version: i64,
    now: TimestampMs,
) -> AppResult<RatingPreview> {
    let session = load_session(tx, session_id)?;
    if session.status == SessionStatus::Finished {
        return Err(ValidationError::new("sessionId", "finished").into());
    }
    let card = load_studyable_card(tx, card_id, card_version)?;
    let moment = study_moment(tx, now)?;
    let (config, scheduler, hash) = scheduler_for(tx, &card)?;
    let intervals = Rating::ALL
        .iter()
        .map(|rating| {
            let scheduled = scheduling::schedule(&card, &scheduler, &hash, *rating, &moment);
            IntervalPreview {
                rating: *rating,
                due_at: scheduled.due_at(),
                interval_ms: scheduled.due_at().0 - now.0,
            }
        })
        .collect();
    Ok(RatingPreview {
        card_id,
        card_version,
        token: preview_token(&card, &config, &hash, session.id, &moment),
        intervals,
    })
}

/// ScheduleReview: rates a card. Card, log, sibling burials, leech action, session item and
/// session state are written in the caller's transaction, so they commit or roll back together.
pub fn rate_card(
    tx: &mut dyn Repositories,
    input: &RateCard,
    now: TimestampMs,
) -> AppResult<RateOutcome> {
    check_operation_id(&input.operation_id)?;
    if let Some(log) = tx.review_log_by_operation(&input.operation_id)? {
        if log.card_id != input.card_id || log.session_id != Some(input.session_id) {
            return Err(reused());
        }
        return Ok(RateOutcome {
            operation_id: input.operation_id.clone(),
            reviewed: ReviewedCard::of_snapshot(log.card_id, &log.card_after),
            view: study_view(tx, input.session_id, now)?,
            replayed: true,
        });
    }
    if tx.recorded_operation(&input.operation_id)?.is_some() {
        return Err(reused());
    }
    if input.answer_ms < 0 {
        return Err(ValidationError::new("answerMs", "outOfRange").into());
    }
    let mut session = load_session(tx, input.session_id)?;
    match session.status {
        SessionStatus::Active => {}
        SessionStatus::Paused => return Err(ValidationError::new("sessionId", "paused").into()),
        SessionStatus::Finished => {
            return Err(ValidationError::new("sessionId", "finished").into());
        }
    }
    let card = load_studyable_card(tx, input.card_id, input.card_version)?;
    if !session_contains(tx, &session, &card)? {
        return Err(ValidationError::new("cardId", "outsideSession").into());
    }
    let moment = study_moment(tx, now)?;
    let (config, scheduler, hash) = scheduler_for(tx, &card)?;
    if input.preview_token != preview_token(&card, &config, &hash, session.id, &moment) {
        return Err(AppError::PreviewStale);
    }

    let scheduled = scheduling::schedule(&card, &scheduler, &hash, input.rating, &moment);
    let mut after = scheduled.card.clone();
    let mut changes = UndoChanges::default();
    if scheduled.lapse && is_leech(after.lapses, config.options.leech_threshold) {
        match config.options.leech_action {
            LeechAction::Suspend => after.queue = QueueState::Suspended,
            LeechAction::Tag => {
                if let Some(version) = tag_leech(tx, card.note_id, now)? {
                    changes.leech_tagged = Some((card.note_id, version));
                }
            }
        }
    }
    let persisted = tx.update_card(&after, now)?;
    if config.options.bury_siblings {
        for sibling in tx.cards_of_note(card.note_id)? {
            if sibling.id == card.id || !scheduling::buries_sibling(&sibling, &moment) {
                continue;
            }
            let before = CardSnapshot::of(&sibling);
            let mut buried = sibling;
            scheduling::bury(&mut buried, QueueState::SchedBuried, &moment);
            let saved = tx.update_card(&buried, now)?;
            changes.siblings.push(SiblingChange {
                card_id: saved.id,
                before,
                after_version: saved.version,
            });
        }
    }
    let duration_ms = input.answer_ms.min(MAX_ANSWER_MS);
    let log = tx.insert_review_log(&ReviewLog {
        id: ReviewLogId(0),
        card_id: card.id,
        session_id: Some(session.id),
        operation_id: input.operation_id.clone(),
        rating: input.rating,
        kind: scheduled.kind,
        reviewed_at: now,
        duration_ms,
        card_before: CardSnapshot::of(&card),
        card_after: CardSnapshot::of(&persisted),
        fsrs_version: FSRS_VERSION.into(),
        parameters_hash: hash,
        invalidated_at: None,
    })?;
    tx.insert_undo_entry(session.id, log.id, &changes, now)?;
    tx.put_session_item(session.id, card.id, SessionItemStatus::Answered)?;
    session.active_ms += duration_ms;
    let view = refresh(tx, &mut session, &moment, false)?;
    Ok(RateOutcome {
        operation_id: input.operation_id.clone(),
        reviewed: ReviewedCard::of(&persisted),
        view,
        replayed: false,
    })
}

/// Undo of the latest rating of the session: restores the card's full snapshot, the buried
/// siblings and the leech tag, invalidates the log (kept for audit, excluded from counts) and
/// makes the card current again.
pub fn undo_review(
    tx: &mut dyn Repositories,
    operation_id: &str,
    session_id: SessionId,
    now: TimestampMs,
) -> AppResult<UndoOutcome> {
    check_operation_id(operation_id)?;
    if let Some(recorded) = tx.recorded_operation(operation_id)? {
        if recorded.operation != OP_UNDO {
            return Err(reused());
        }
        let id: i64 = serde_json::from_str(&recorded.result_json)
            .map_err(|_| AppError::Internal("unreadable undo result".into()))?;
        let card = tx
            .card(CardId(id))?
            .ok_or(AppError::NotFound { entity: "card" })?;
        return Ok(UndoOutcome {
            operation_id: operation_id.to_owned(),
            restored: ReviewedCard::of(&card),
            view: study_view(tx, session_id, now)?,
            replayed: true,
        });
    }
    not_a_rating(tx, operation_id)?;
    let mut session = load_session(tx, session_id)?;
    let nothing = || -> AppError { ValidationError::new("sessionId", "nothingToUndo").into() };
    let entry = tx
        .last_undo_entry(session.id)?
        .filter(|entry| entry.consumed_at.is_none())
        .ok_or_else(nothing)?;
    let log = tx
        .review_log_by_operation(&entry.operation_id)?
        .filter(|log| log.invalidated_at.is_none())
        .ok_or_else(nothing)?;

    // Refuse when anything the rating wrote changed afterwards.
    let mut card = tx
        .card(log.card_id)?
        .ok_or(AppError::NotFound { entity: "card" })?;
    if card.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "card" });
    }
    if card.version != log.card_after.version {
        return Err(AppError::VersionConflict {
            entity: "card",
            current: card.version,
        });
    }
    let mut siblings = Vec::with_capacity(entry.changes.siblings.len());
    for change in &entry.changes.siblings {
        let sibling = tx
            .card(change.card_id)?
            .ok_or(AppError::NotFound { entity: "card" })?;
        if sibling.deleted_at.is_some() || sibling.version != change.after_version {
            return Err(AppError::VersionConflict {
                entity: "card",
                current: sibling.version,
            });
        }
        siblings.push((sibling, change));
    }
    let leech_note = match entry.changes.leech_tagged {
        Some((note_id, version)) => {
            let note = tx
                .note(note_id)?
                .ok_or(AppError::NotFound { entity: "note" })?;
            if note.version != version {
                return Err(AppError::VersionConflict {
                    entity: "note",
                    current: note.version,
                });
            }
            Some(note)
        }
        None => None,
    };

    log.card_before.apply_to(&mut card);
    let restored = tx.update_card(&card, now)?;
    for (mut sibling, change) in siblings {
        change.before.apply_to(&mut sibling);
        tx.update_card(&sibling, now)?;
    }
    if let Some(mut note) = leech_note {
        note.tags.retain(|tag| !tag.eq_ignore_ascii_case(LEECH_TAG));
        tx.update_note(&note, now)?;
    }
    tx.invalidate_review_log(&log.operation_id, now)?;
    tx.consume_undo_entry(entry.id, now)?;
    tx.put_session_item(session.id, restored.id, SessionItemStatus::Pending)?;
    session.active_ms = (session.active_ms - log.duration_ms).max(0);
    if session.status == SessionStatus::Finished {
        session.status = SessionStatus::Active;
        session.finished_at = None;
    }
    session.current_card_id = Some(restored.id);
    let moment = study_moment(tx, now)?;
    let (plan, reached) = plan_for_session(tx, &session, &moment)?;
    tx.update_session(&session, now)?;
    let view = build_view(
        tx,
        session.clone(),
        Some(restored.clone()),
        &plan,
        reached,
        &moment,
    )?;
    tx.record_operation(
        operation_id,
        &RecordedOperation {
            operation: OP_UNDO.into(),
            result_json: restored.id.0.to_string(),
        },
        now,
    )?;
    Ok(UndoOutcome {
        operation_id: operation_id.to_owned(),
        restored: ReviewedCard::of(&restored),
        view,
        replayed: false,
    })
}

/// "Study more today" (plan step 13): raises today's limits of one deck. Repeating the operation
/// changes nothing. Returns the deck's new counts.
pub fn extend_today_limits(
    tx: &mut dyn Repositories,
    operation_id: &str,
    deck_id: DeckId,
    extra_new: u32,
    extra_review: u32,
    now: TimestampMs,
) -> AppResult<QueueCounts> {
    check_operation_id(operation_id)?;
    if let Some(recorded) = tx.recorded_operation(operation_id)? {
        if recorded.operation != OP_EXTEND {
            return Err(reused());
        }
        return due_counts(tx, Some(deck_id), now);
    }
    not_a_rating(tx, operation_id)?;
    if extra_new > MAX_LIMIT_EXTENSION {
        return Err(ValidationError::new("extraNew", "outOfRange").into());
    }
    if extra_review > MAX_LIMIT_EXTENSION {
        return Err(ValidationError::new("extraReview", "outOfRange").into());
    }
    let deck = tx
        .deck(deck_id)?
        .ok_or(AppError::NotFound { entity: "deck" })?;
    if deck.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "deck" });
    }
    let moment = study_moment(tx, now)?;
    let mut extensions = load_extensions(tx, &moment)?;
    let entry = extensions.entry(deck_id).or_default();
    entry.new = entry.new.saturating_add(extra_new);
    entry.review = entry.review.saturating_add(extra_review);
    let decks: Map<String, Value> = extensions
        .iter()
        .map(|(id, limits)| (id.0.to_string(), json!([limits.new, limits.review])))
        .collect();
    let value = json!({ "day": moment.today.0, "decks": decks }).to_string();
    tx.put_setting(LIMIT_EXTENSIONS_KEY, &value, now)?;
    tx.record_operation(
        operation_id,
        &RecordedOperation {
            operation: OP_EXTEND.into(),
            result_json: deck_id.0.to_string(),
        },
        now,
    )?;
    due_counts(tx, Some(deck_id), now)
}

/// GetStudyStats: what the session summary shows. Read-only.
pub fn session_stats(
    tx: &mut dyn Repositories,
    session_id: SessionId,
    now: TimestampMs,
) -> AppResult<SessionStats> {
    let session = load_session(tx, session_id)?;
    let logs = tx.session_reviews(session.id)?;
    let mut ratings = [0u32; 4];
    let mut again_cards = Vec::new();
    for log in &logs {
        let index = match log.rating {
            Rating::Again => 0,
            Rating::Hard => 1,
            Rating::Good => 2,
            Rating::Easy => 3,
        };
        ratings[index] += 1;
        if log.rating == Rating::Again && !again_cards.contains(&log.card_id) {
            again_cards.push(log.card_id);
        }
    }
    let moment = study_moment(tx, now)?;
    let remaining = plan_for_session(tx, &session, &moment)?.0.counts();
    Ok(SessionStats {
        reviewed_count: logs.len() as u32,
        session,
        ratings,
        again_cards,
        next_due_at: tx.next_due_after(now)?,
        remaining,
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests of the pure helpers. The use cases need a repository and are covered end to end
    //! on SQLite by `crates/infrastructure/tests/study.rs`.

    use langualinker_domain::Guid;
    use langualinker_domain::card::{CardFlag, CardFsrsState};
    use langualinker_domain::deck::{DeckConfigOptions, DeckType, NativeLanguage, TargetLanguage};

    use super::*;

    const NOON: i64 = 1_772_452_800_000;

    fn moment(now: i64) -> StudyMoment {
        StudyMoment::new(StudyCalendar::utc(240), TimestampMs(now))
    }

    fn card() -> Card {
        Card {
            id: CardId(1),
            guid: Guid::from_bytes([2; 16]),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            template_ordinal: 0,
            position: 0,
            flag: CardFlag::None,
            state: CardState::Review,
            queue: QueueState::Review,
            due_at: Some(TimestampMs(NOON)),
            fsrs: CardFsrsState::NEW,
            legacy_ease: None,
            reps: 3,
            lapses: 0,
            learning_step: None,
            buried_until: None,
            archived_at: None,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 7,
        }
    }

    fn config(id: i64, version: i64) -> DeckConfig {
        DeckConfig {
            id: DeckConfigId(id),
            guid: Guid::from_bytes([3; 16]),
            name: "Preset".into(),
            built_in: false,
            options: DeckConfigOptions::default(),
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version,
        }
    }

    fn deck(id: i64, name: &str, parent: Option<i64>) -> Deck {
        Deck {
            id: DeckId(id),
            guid: Guid::from_bytes([id as u8; 16]),
            name: name.into(),
            parent_id: parent.map(DeckId),
            description: String::new(),
            native_language: NativeLanguage::Pl,
            target_language: TargetLanguage::En,
            deck_type: DeckType::Simple,
            config_id: DeckConfigId(1),
            content_version: None,
            bundled: false,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 1,
        }
    }

    fn tree(decks: Vec<Deck>) -> DeckTree {
        DeckTree {
            decks,
            configs: HashMap::new(),
        }
    }

    fn ids(values: &[i64]) -> Vec<DeckId> {
        values.iter().map(|id| DeckId(*id)).collect()
    }

    #[test]
    fn operation_ids_must_be_1_to_128_characters() {
        assert!(check_operation_id("a").is_ok());
        assert!(check_operation_id(&"x".repeat(128)).is_ok());
        assert!(check_operation_id("").is_err());
        assert!(check_operation_id(&"x".repeat(129)).is_err());
    }

    #[test]
    fn only_active_unsuspended_unburied_cards_are_studyable() {
        let mut studyable_card = card();
        for queue in [QueueState::New, QueueState::Learning, QueueState::Review] {
            studyable_card.queue = queue;
            assert!(studyable(&studyable_card), "{queue:?} should be studyable");
        }
        for queue in [
            QueueState::Suspended,
            QueueState::SchedBuried,
            QueueState::UserBuried,
        ] {
            studyable_card.queue = queue;
            assert!(
                !studyable(&studyable_card),
                "{queue:?} should not be studyable"
            );
        }

        let mut deleted = card();
        deleted.deleted_at = Some(TimestampMs(1));
        assert!(!studyable(&deleted));

        let mut archived = card();
        archived.archived_at = Some(TimestampMs(1));
        assert!(!studyable(&archived));
    }

    #[test]
    fn candidate_copies_the_identifying_fields_of_a_card() {
        let source = card();
        let picked = candidate(&source);
        assert_eq!(picked.card_id, source.id);
        assert_eq!(picked.note_id, source.note_id);
        assert_eq!(picked.deck_id, source.deck_id);
        assert_eq!(picked.due_at, source.due_at);
    }

    #[test]
    fn reviewed_card_takes_version_state_and_due_from_the_card() {
        let source = card();
        let reviewed = ReviewedCard::of(&source);
        assert_eq!(reviewed.card_id, source.id);
        assert_eq!(reviewed.version, source.version);
        assert_eq!(reviewed.state, source.state);
        assert_eq!(reviewed.due_at, source.due_at);

        let snapshot = CardSnapshot::of(&source);
        assert_eq!(ReviewedCard::of_snapshot(source.id, &snapshot), reviewed);
    }

    #[test]
    fn preview_token_is_32_hex_characters_and_stable_for_the_same_inputs() {
        let source = card();
        let preset = config(5, 2);
        let now = moment(NOON);
        let first = preview_token(&source, &preset, "hash", SessionId(9), &now);
        let second = preview_token(&source, &preset, "hash", SessionId(9), &now);
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(first, second);
    }

    #[test]
    fn preview_token_changes_when_anything_that_moves_intervals_changes() {
        let source = card();
        let preset = config(5, 2);
        let now = moment(NOON);
        let base = preview_token(&source, &preset, "hash", SessionId(9), &now);

        let mut newer_card = source.clone();
        newer_card.version += 1;
        assert_ne!(
            base,
            preview_token(&newer_card, &preset, "hash", SessionId(9), &now)
        );

        assert_ne!(
            base,
            preview_token(&source, &config(5, 3), "hash", SessionId(9), &now)
        );
        assert_ne!(
            base,
            preview_token(&source, &config(6, 2), "hash", SessionId(9), &now)
        );
        assert_ne!(
            base,
            preview_token(&source, &preset, "other-hash", SessionId(9), &now)
        );
        assert_ne!(
            base,
            preview_token(&source, &preset, "hash", SessionId(10), &now)
        );

        let next_day = moment(NOON + 2 * 86_400_000);
        assert_ne!(
            base,
            preview_token(&source, &preset, "hash", SessionId(9), &next_day)
        );
    }

    #[test]
    fn preorder_of_all_decks_walks_roots_by_name_then_children_depth_first() {
        // Roots: Beta, Alpha. Alpha has children Zeta and Gamma (sorted by name).
        let decks = vec![
            deck(1, "Beta", None),
            deck(2, "Alpha", None),
            deck(3, "Zeta", Some(2)),
            deck(4, "Gamma", Some(2)),
            deck(5, "Gamma-sub", Some(4)),
        ];
        let order = tree(decks).preorder(None);
        assert_eq!(order, ids(&[2, 4, 5, 3, 1]));
    }

    #[test]
    fn preorder_of_a_scope_contains_only_that_subtree() {
        let decks = vec![
            deck(1, "Root", None),
            deck(2, "Child", Some(1)),
            deck(3, "Grandchild", Some(2)),
            deck(4, "Other", None),
        ];
        assert_eq!(tree(decks.clone()).preorder(Some(DeckId(2))), ids(&[2, 3]));
        assert_eq!(tree(decks).preorder(Some(DeckId(1))), ids(&[1, 2, 3]));
    }

    #[test]
    fn preorder_of_an_unknown_scope_is_empty() {
        let decks = vec![deck(1, "Root", None)];
        assert!(tree(decks).preorder(Some(DeckId(99))).is_empty());
    }

    #[test]
    fn a_deck_whose_parent_is_not_active_is_treated_as_a_root() {
        // Parent 42 is not in the active list, so "Orphan" (2) becomes a root beside "Root" (1);
        // roots are visited by name.
        let decks = vec![deck(1, "Root", None), deck(2, "Orphan", Some(42))];
        assert_eq!(tree(decks).preorder(None), ids(&[2, 1]));
    }

    #[test]
    fn a_parent_cycle_terminates_and_visits_each_deck_once() {
        // 1 and 2 are each other's parent, so neither is a root and the walk from roots is empty.
        let decks = vec![deck(1, "A", Some(2)), deck(2, "B", Some(1))];
        assert!(tree(decks.clone()).preorder(None).is_empty());
        assert_eq!(tree(decks).preorder(Some(DeckId(1))), ids(&[1, 2]));
    }

    #[test]
    fn preorder_of_an_empty_tree_is_empty() {
        assert!(tree(Vec::new()).preorder(None).is_empty());
    }
}
