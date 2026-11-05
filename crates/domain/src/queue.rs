//! Study queue: daily limits over the deck tree, sibling dedupe, interleaving and the choice of
//! the next card (plan Phase 9 steps 9–13, ADR-15). Pure: the application loads candidates and
//! today's committed counts, this module decides.
//!
//! Rules:
//! - A deck's daily limit caps the cards of the deck and its subdecks. A card is admitted when
//!   every deck from the card's deck up to the studied deck still has room; admitting it uses
//!   room in all of them. With "all decks" the path ends at the top-level deck.
//! - "Done today" is computed from committed, valid review logs: introduced new cards count
//!   against the new limit, reviews of review cards against the review limit. Learning steps are
//!   never limited.
//! - Each card appears once (it has one deck), so "all decks" needs no further dedupe.
//! - With sibling burying on, at most one card of a note is gathered per day.
//! - Order: intraday learning cards due now; then reviews (by due) mixed evenly with new cards (by
//!   position, or a per-day shuffle); then learning cards due within the learn-ahead window.
//! - Suspended, buried, archived and deleted cards are never candidates.

use std::collections::{HashMap, HashSet};

use crate::{CardId, DeckId, NoteId, TimestampMs};

/// Learning cards due this soon are shown when nothing else is left (Anki's learn-ahead).
pub const LEARN_AHEAD_MS: i64 = 20 * 60_000;

/// What the queue needs to know about a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub card_id: CardId,
    pub note_id: NoteId,
    pub deck_id: DeckId,
    pub due_at: Option<TimestampMs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DailyLimits {
    pub new: u32,
    pub review: u32,
}

/// One deck of the tree with its limits for today (preset limits plus any extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckNode {
    pub id: DeckId,
    pub parent: Option<DeckId>,
    pub limits: DailyLimits,
    /// The deck's preset buries siblings.
    pub bury_siblings: bool,
}

/// Committed ratings today of cards in one deck (not its subdecks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoneToday {
    pub new: u32,
    pub review: u32,
}

/// Remaining room per deck for today.
#[derive(Debug, Clone)]
pub struct LimitTree {
    parent: HashMap<DeckId, Option<DeckId>>,
    remaining: HashMap<DeckId, DailyLimits>,
    bury: HashMap<DeckId, bool>,
    scope: Option<DeckId>,
}

impl LimitTree {
    /// `scope` is the studied deck (`None` = all decks). Decks missing from `decks` are ignored.
    pub fn new(
        decks: &[DeckNode],
        done: &HashMap<DeckId, DoneToday>,
        scope: Option<DeckId>,
    ) -> Self {
        let parent: HashMap<DeckId, Option<DeckId>> =
            decks.iter().map(|deck| (deck.id, deck.parent)).collect();
        let mut subtree_done: HashMap<DeckId, DoneToday> = HashMap::new();
        for (deck, counts) in done {
            let mut cursor = Some(*deck);
            let mut guard = 0;
            while let Some(id) = cursor {
                if !parent.contains_key(&id) || guard > decks.len() {
                    break;
                }
                let entry = subtree_done.entry(id).or_default();
                entry.new += counts.new;
                entry.review += counts.review;
                cursor = parent[&id];
                guard += 1;
            }
        }
        let remaining = decks
            .iter()
            .map(|deck| {
                let done = subtree_done.get(&deck.id).copied().unwrap_or_default();
                (
                    deck.id,
                    DailyLimits {
                        new: deck.limits.new.saturating_sub(done.new),
                        review: deck.limits.review.saturating_sub(done.review),
                    },
                )
            })
            .collect();
        Self {
            parent,
            remaining,
            bury: decks
                .iter()
                .map(|deck| (deck.id, deck.bury_siblings))
                .collect(),
            scope,
        }
    }

    /// Decks whose cards belong to the scope (the studied deck and its subdecks, or all).
    pub fn scope_decks(&self) -> Vec<DeckId> {
        let mut decks: Vec<DeckId> = self
            .parent
            .keys()
            .copied()
            .filter(|deck| self.path(*deck).is_some())
            .collect();
        decks.sort();
        decks
    }

    /// Decks from `deck` up to the scope root; `None` when `deck` is outside the scope.
    fn path(&self, deck: DeckId) -> Option<Vec<DeckId>> {
        let mut path = Vec::new();
        let mut cursor = Some(deck);
        while let Some(id) = cursor {
            if !self.parent.contains_key(&id) || path.len() > self.parent.len() {
                return None;
            }
            path.push(id);
            if Some(id) == self.scope {
                return Some(path);
            }
            cursor = self.parent[&id];
        }
        self.scope.is_none().then_some(path)
    }

    /// Room left for `deck`'s cards: the smallest remaining limit on its path.
    pub fn room(&self, deck: DeckId) -> DailyLimits {
        let Some(path) = self.path(deck) else {
            return DailyLimits::default();
        };
        path.iter().map(|id| self.remaining[id]).fold(
            DailyLimits {
                new: u32::MAX,
                review: u32::MAX,
            },
            |room, limits| DailyLimits {
                new: room.new.min(limits.new),
                review: room.review.min(limits.review),
            },
        )
    }

    fn take(&mut self, deck: DeckId, new: bool) -> bool {
        let room = self.room(deck);
        if (new && room.new == 0) || (!new && room.review == 0) {
            return false;
        }
        for id in self.path(deck).unwrap_or_default() {
            let limits = self.remaining.get_mut(&id).expect("deck on path");
            if new {
                limits.new -= 1;
            } else {
                limits.review -= 1;
            }
        }
        true
    }

    pub fn buries_siblings(&self, deck: DeckId) -> bool {
        self.bury.get(&deck).copied().unwrap_or(false)
    }

    /// Whether any deck on the path of the scope has hit a limit (for "nothing due" hints).
    pub fn scope_room(&self) -> DailyLimits {
        match self.scope {
            Some(scope) => self.room(scope),
            None => self
                .parent
                .iter()
                .filter(|(_, parent)| parent.is_none())
                .map(|(id, _)| self.remaining[id])
                .fold(DailyLimits::default(), |sum, limits| DailyLimits {
                    new: sum.new.saturating_add(limits.new),
                    review: sum.review.saturating_add(limits.review),
                }),
        }
    }
}

/// Which list a chosen card came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    Learning,
    Review,
    New,
}

/// Counts shown during study.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueCounts {
    pub new: u32,
    /// Intraday learning cards due today.
    pub learning: u32,
    /// Admitted review and day-learning cards.
    pub review: u32,
    /// Admitted reviews that were due before today.
    pub overdue: u32,
}

/// Candidates of the scope, each list in its natural order.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    /// Intraday learning cards due before the end of today, by due time.
    pub learning: Vec<Candidate>,
    /// Review and day-learning cards due before the end of today, by due time.
    pub reviews: Vec<Candidate>,
    /// New cards in introduction order.
    pub news: Vec<Candidate>,
}

/// Today's queue after limits and sibling rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueuePlan {
    pub learning: Vec<Candidate>,
    pub reviews: Vec<Candidate>,
    pub news: Vec<Candidate>,
    pub day_start: TimestampMs,
}

impl QueuePlan {
    /// Applies the limits of `tree` to the candidates. `seen_notes` are notes that already had a
    /// card answered today (their siblings were buried or are being learned).
    pub fn build(
        candidates: Candidates,
        mut tree: LimitTree,
        seen_notes: &HashSet<NoteId>,
        day_start: TimestampMs,
    ) -> Self {
        let mut gathered: HashSet<NoteId> = HashSet::new();
        let learning_notes: HashSet<NoteId> = candidates
            .learning
            .iter()
            .map(|card| card.note_id)
            .collect();
        let mut admit = |card: &Candidate, new: bool, tree: &mut LimitTree| {
            let bury = tree.buries_siblings(card.deck_id);
            if bury
                && (gathered.contains(&card.note_id)
                    || learning_notes.contains(&card.note_id)
                    || seen_notes.contains(&card.note_id))
            {
                return false;
            }
            if !tree.take(card.deck_id, new) {
                return false;
            }
            gathered.insert(card.note_id);
            true
        };
        let reviews = candidates
            .reviews
            .iter()
            .filter(|card| admit(card, false, &mut tree))
            .copied()
            .collect();
        let news = candidates
            .news
            .iter()
            .filter(|card| admit(card, true, &mut tree))
            .copied()
            .collect();
        Self {
            learning: candidates.learning,
            reviews,
            news,
            day_start,
        }
    }

    /// A queue over a fixed list of cards (studying chosen cards again): no limits.
    pub fn fixed(learning: Vec<Candidate>, cards: Vec<Candidate>, day_start: TimestampMs) -> Self {
        let learning_ids: HashSet<CardId> = learning.iter().map(|card| card.card_id).collect();
        Self {
            learning,
            reviews: cards
                .into_iter()
                .filter(|card| !learning_ids.contains(&card.card_id))
                .collect(),
            news: Vec::new(),
            day_start,
        }
    }

    pub fn counts(&self) -> QueueCounts {
        QueueCounts {
            new: self.news.len() as u32,
            learning: self.learning.len() as u32,
            review: self.reviews.len() as u32,
            overdue: self
                .reviews
                .iter()
                .filter(|card| card.due_at.is_some_and(|due| due < self.day_start))
                .count() as u32,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.learning.is_empty() && self.reviews.is_empty() && self.news.is_empty()
    }

    /// The card to show at `now`. `answered` is the number of cards answered in the session so
    /// far; it spreads new cards evenly among the reviews.
    pub fn next(&self, now: TimestampMs, answered: u32) -> Option<(Candidate, QueueKind)> {
        let due_now = |card: &&Candidate| card.due_at.is_some_and(|due| due <= now);
        if let Some(card) = self.learning.iter().find(due_now) {
            return Some((*card, QueueKind::Learning));
        }
        let review = self.reviews.first();
        let new = self.news.first();
        match (review, new) {
            (Some(review), Some(new)) => {
                let reviews = self.reviews.len() as u32;
                let news = self.news.len() as u32;
                // Show a new card after every `ratio - 1` reviews.
                let ratio = (reviews + news) / news;
                if (answered + 1).is_multiple_of(ratio.max(1)) {
                    Some((*new, QueueKind::New))
                } else {
                    Some((*review, QueueKind::Review))
                }
            }
            (Some(review), None) => Some((*review, QueueKind::Review)),
            (None, Some(new)) => Some((*new, QueueKind::New)),
            (None, None) => {
                let ahead = TimestampMs(now.0 + LEARN_AHEAD_MS);
                self.learning
                    .iter()
                    .filter(|card| card.due_at.is_some_and(|due| due <= ahead))
                    .min_by_key(|card| card.due_at)
                    .map(|card| (*card, QueueKind::Learning))
            }
        }
    }

    /// Earliest due time among learning cards not yet due at `now` (to tell the user when to
    /// come back).
    pub fn next_learning_due(&self, now: TimestampMs) -> Option<TimestampMs> {
        self.learning
            .iter()
            .filter_map(|card| card.due_at)
            .filter(|due| *due > now)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: TimestampMs = TimestampMs(1_000_000);

    fn node(id: i64, parent: Option<i64>, new: u32, review: u32) -> DeckNode {
        DeckNode {
            id: DeckId(id),
            parent: parent.map(DeckId),
            limits: DailyLimits { new, review },
            bury_siblings: false,
        }
    }

    fn card(id: i64, deck: i64, due: Option<i64>) -> Candidate {
        Candidate {
            card_id: CardId(id),
            note_id: NoteId(id),
            deck_id: DeckId(deck),
            due_at: due.map(TimestampMs),
        }
    }

    fn plan(
        decks: &[DeckNode],
        done: &[(i64, u32, u32)],
        scope: Option<i64>,
        c: Candidates,
    ) -> QueuePlan {
        let done = done
            .iter()
            .map(|(deck, new, review)| {
                (
                    DeckId(*deck),
                    DoneToday {
                        new: *new,
                        review: *review,
                    },
                )
            })
            .collect();
        let tree = LimitTree::new(decks, &done, scope.map(DeckId));
        QueuePlan::build(c, tree, &HashSet::new(), TimestampMs(500_000))
    }

    #[test]
    fn a_parent_limit_caps_its_children() {
        // Parent 1 allows 3 reviews; children allow 5 each.
        let decks = [
            node(1, None, 10, 3),
            node(2, Some(1), 10, 5),
            node(3, Some(1), 10, 5),
        ];
        let reviews = (1..=8)
            .map(|i| card(i, if i % 2 == 0 { 2 } else { 3 }, Some(0)))
            .collect();
        let result = plan(
            &decks,
            &[],
            Some(1),
            Candidates {
                reviews,
                ..Default::default()
            },
        );
        assert_eq!(result.counts().review, 3);
    }

    #[test]
    fn a_child_limit_holds_when_studying_the_parent() {
        let decks = [node(1, None, 10, 100), node(2, Some(1), 10, 1)];
        let reviews = vec![
            card(1, 2, Some(0)),
            card(2, 2, Some(1)),
            card(3, 1, Some(2)),
        ];
        let result = plan(
            &decks,
            &[],
            Some(1),
            Candidates {
                reviews,
                ..Default::default()
            },
        );
        let ids: Vec<i64> = result.reviews.iter().map(|c| c.card_id.0).collect();
        assert_eq!(ids, [1, 3]);
    }

    #[test]
    fn studying_a_child_ignores_the_parent_limit_but_counts_done_cards_upwards() {
        let decks = [node(1, None, 2, 1), node(2, Some(1), 5, 5)];
        let news = (1..=6).map(|i| card(i, 2, None)).collect();
        // Child studied alone: only its own limit (5) applies.
        let child = plan(
            &decks,
            &[],
            Some(2),
            Candidates {
                news,
                ..Default::default()
            },
        );
        assert_eq!(child.counts().new, 5);
        // Two new cards already introduced in the child exhaust the parent's limit of 2.
        let news = (1..=6).map(|i| card(i, 2, None)).collect();
        let parent = plan(
            &decks,
            &[(2, 2, 0)],
            Some(1),
            Candidates {
                news,
                ..Default::default()
            },
        );
        assert_eq!(parent.counts().new, 0);
    }

    #[test]
    fn all_decks_scope_covers_every_tree() {
        let decks = [
            node(1, None, 1, 10),
            node(2, None, 1, 10),
            node(3, Some(2), 1, 10),
        ];
        let news = vec![
            card(1, 1, None),
            card(2, 1, None),
            card(3, 3, None),
            card(4, 2, None),
        ];
        let result = plan(
            &decks,
            &[],
            None,
            Candidates {
                news,
                ..Default::default()
            },
        );
        let ids: Vec<i64> = result.news.iter().map(|c| c.card_id.0).collect();
        assert_eq!(
            ids,
            [1, 3],
            "deck 2's single slot goes to its subdeck card first"
        );
        assert_eq!(
            LimitTree::new(&decks, &HashMap::new(), None)
                .scope_decks()
                .len(),
            3
        );
        assert_eq!(
            LimitTree::new(&decks, &HashMap::new(), Some(DeckId(2))).scope_decks(),
            [DeckId(2), DeckId(3)]
        );
    }

    #[test]
    fn cards_outside_the_scope_are_never_admitted() {
        let decks = [node(1, None, 10, 10), node(2, None, 10, 10)];
        let reviews = vec![card(1, 2, Some(0))];
        let result = plan(
            &decks,
            &[],
            Some(1),
            Candidates {
                reviews,
                ..Default::default()
            },
        );
        assert!(result.is_empty());
    }

    #[test]
    fn siblings_are_gathered_once_when_burying() {
        let mut decks = [node(1, None, 10, 10)];
        decks[0].bury_siblings = true;
        let mut sibling = card(2, 1, None);
        sibling.note_id = NoteId(1);
        let candidates = Candidates {
            reviews: vec![card(1, 1, Some(0))],
            news: vec![sibling, card(3, 1, None)],
            ..Default::default()
        };
        let result = plan(&decks, &[], Some(1), candidates.clone());
        assert_eq!(
            result.news.iter().map(|c| c.card_id.0).collect::<Vec<_>>(),
            [3]
        );
        decks[0].bury_siblings = false;
        let result = plan(&decks, &[], Some(1), candidates);
        assert_eq!(result.news.len(), 2);
    }

    #[test]
    fn learning_due_now_comes_first_then_reviews_mix_with_new_cards() {
        let decks = [node(1, None, 10, 10)];
        let candidates = Candidates {
            learning: vec![
                card(10, 1, Some(NOW.0 - 1)),
                card(11, 1, Some(NOW.0 + 60_000)),
            ],
            reviews: (1..=4).map(|i| card(i, 1, Some(0))).collect(),
            news: vec![card(20, 1, None), card(21, 1, None)],
        };
        let result = plan(&decks, &[], Some(1), candidates);
        assert_eq!(
            result.next(NOW, 0),
            Some((card(10, 1, Some(NOW.0 - 1)), QueueKind::Learning))
        );
        let mut without_learning = result.clone();
        without_learning.learning.clear();
        // 4 reviews + 2 new: a new card every third card.
        let kinds: Vec<QueueKind> = (0..3)
            .map(|i| without_learning.next(NOW, i).unwrap().1)
            .collect();
        assert_eq!(
            kinds,
            [QueueKind::Review, QueueKind::Review, QueueKind::New]
        );
    }

    #[test]
    fn learning_cards_are_shown_early_only_when_nothing_else_is_left() {
        let soon = card(11, 1, Some(NOW.0 + LEARN_AHEAD_MS - 1));
        let later = card(12, 1, Some(NOW.0 + LEARN_AHEAD_MS + 1));
        let queue = QueuePlan {
            learning: vec![later, soon],
            ..Default::default()
        };
        assert_eq!(queue.next(NOW, 0), Some((soon, QueueKind::Learning)));
        let queue = QueuePlan {
            learning: vec![later],
            ..Default::default()
        };
        assert_eq!(queue.next(NOW, 0), None);
        assert_eq!(queue.next_learning_due(NOW), later.due_at);
    }

    #[test]
    fn overdue_counts_reviews_due_before_today() {
        let decks = [node(1, None, 10, 10)];
        let reviews = vec![card(1, 1, Some(100)), card(2, 1, Some(600_000))];
        let result = plan(
            &decks,
            &[],
            Some(1),
            Candidates {
                reviews,
                ..Default::default()
            },
        );
        assert_eq!(
            result.counts(),
            QueueCounts {
                new: 0,
                learning: 0,
                review: 2,
                overdue: 1
            }
        );
    }

    #[test]
    fn fixed_queues_ignore_limits_and_keep_learning_separate() {
        let learning = vec![card(1, 1, Some(NOW.0))];
        let cards = vec![card(1, 1, Some(NOW.0)), card(2, 1, None)];
        let queue = QueuePlan::fixed(learning, cards, TimestampMs(0));
        assert_eq!(queue.counts().review, 1);
        assert_eq!(queue.counts().learning, 1);
    }
}
