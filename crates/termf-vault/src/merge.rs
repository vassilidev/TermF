//! The item-by-item merge of two versions of one vault, for the sync and the import. Each item and secret keeps
//! the history of its versions (`revs`): a version that knows the other one's last replaces it, two that do not
//! are a conflict (both kept until the user chooses), and a deletion leaves a tombstone that removes only the
//! versions it saw. Nothing is ever dropped without one side having seen it. Another vault's export shares no
//! history with this one: it only adds, or sets its versions beside ours ([`Content::merge_foreign`]).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use termf_sync::crypto;

use crate::item::{Item, Kind};
use crate::{Content, Scope, Stored};

/// How many versions an element remembers: changed more often than this between two merges, it gets a
/// conflict, never a loss.
const HISTORY: usize = 32;
/// How long a deletion is remembered: a machine away for longer may bring the deleted element back.
pub(crate) const TOMBSTONE_DAYS: u64 = 180;

/// A new version of an element.
pub(crate) fn new_rev(revs: &mut Vec<String>) {
    revs.push(crypto::hex(&crypto::random::<4>()));
    trim(revs);
}

fn trim(revs: &mut Vec<String>) {
    let extra = revs.len().saturating_sub(HISTORY);
    revs.drain(..extra);
}

/// Whether `newer` holds every change of `older`. Two versions from before histories existed know nothing of
/// each other.
fn knows(newer: &[String], older: &[String]) -> bool {
    match older.last() {
        Some(last) => newer.contains(last),
        None => !newer.is_empty(),
    }
}

/// Both histories in `mine`, its last kept last and the other's right before it, so trimming keeps both.
fn union(mine: &mut Vec<String>, theirs: &[String]) {
    let last = mine.pop();
    for rev in theirs {
        if !mine.contains(rev) && Some(rev) != last.as_ref() {
            mine.push(rev.clone());
        }
    }
    if let Some(i) = theirs.last().and_then(|l| mine.iter().position(|r| r == l)) {
        let rev = mine.remove(i);
        mine.push(rev);
    }
    mine.extend(last);
    trim(mine);
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Tombstone {
    /// An item's id, or a secret's [`Stored::key`].
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The history of the version deleted.
    #[serde(default)]
    pub revs: Vec<String>,
    pub deleted: u64,
}

impl Tombstone {
    /// The deletion saw this version: it goes.
    fn buries(&self, revs: &[String]) -> bool {
        revs.last().is_none_or(|last| self.revs.contains(last))
    }
}

/// Another version of an element both sides changed, kept until the user chooses.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Conflicted {
    pub id: String,
    /// When it was found.
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<Stored>,
}

impl Conflicted {
    pub fn key(&self) -> String {
        match (&self.item, &self.secret) {
            (Some(item), _) => item.id.clone(),
            (_, Some(secret)) => secret.key(),
            _ => String::new(),
        }
    }

    fn revs(&self) -> &[String] {
        match (&self.item, &self.secret) {
            (Some(item), _) => &item.revs,
            (_, Some(secret)) => &secret.revs,
            _ => &[],
        }
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Changed,
    Deleted,
    /// Both sides changed it: the other version is kept beside it, for the user to choose.
    Conflict,
    Unchanged,
}

/// What a merge does to one element: its title, never a value.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    pub key: String,
    /// `None`: a secret.
    pub kind: Option<Kind>,
    pub scope: Option<Scope>,
    pub title: String,
    pub project: Option<String>,
    pub change: Change,
}

/// `2 added, 1 changed, 1 conflict`, or `nothing changed`.
pub fn summary(rows: &[Merged]) -> String {
    let count = |change| rows.iter().filter(|r| r.change == change).count();
    let parts: Vec<String> = [
        (Change::Added, "added"),
        (Change::Changed, "changed"),
        (Change::Deleted, "deleted"),
        (Change::Conflict, "conflict"),
    ]
    .into_iter()
    .map(|(change, word)| (count(change), word))
    .filter(|(n, _)| *n > 0)
    .map(|(n, word)| format!("{n} {word}"))
    .collect();
    if parts.is_empty() {
        "nothing changed".into()
    } else {
        parts.join(", ")
    }
}

pub(crate) trait Element: Clone {
    fn key(&self) -> String;
    fn revs(&self) -> &Vec<String>;
    fn revs_mut(&mut self) -> &mut Vec<String>;
    /// The same content, whatever its history and dates.
    fn same(&self, other: &Self) -> bool;
    fn row(&self, change: Change) -> Merged;
    fn of(conflict: &Conflicted) -> Option<&Self>;
    fn into_conflict(self, at: u64) -> Conflicted;
}

impl Element for Item {
    fn key(&self) -> String {
        self.id.clone()
    }
    fn revs(&self) -> &Vec<String> {
        &self.revs
    }
    fn revs_mut(&mut self) -> &mut Vec<String> {
        &mut self.revs
    }
    fn same(&self, o: &Self) -> bool {
        self.kind == o.kind
            && self.project == o.project
            && self.title == o.title
            && self.urls == o.urls
            && self.username == o.username
            && self.password == o.password
            && self.totp == o.totp
            && self.notes == o.notes
            && self.agents == o.agents
    }
    fn row(&self, change: Change) -> Merged {
        Merged {
            key: self.id.clone(),
            kind: Some(self.kind),
            scope: None,
            title: self.title.clone(),
            project: self.project.clone(),
            change,
        }
    }
    fn of(conflict: &Conflicted) -> Option<&Self> {
        conflict.item.as_ref()
    }
    fn into_conflict(self, at: u64) -> Conflicted {
        Conflicted {
            id: crypto::hex(&crypto::random::<8>()),
            at,
            item: Some(self),
            secret: None,
        }
    }
}

impl Element for Stored {
    fn key(&self) -> String {
        Stored::key(self)
    }
    fn revs(&self) -> &Vec<String> {
        &self.revs
    }
    fn revs_mut(&mut self) -> &mut Vec<String> {
        &mut self.revs
    }
    fn same(&self, o: &Self) -> bool {
        self.key() == o.key()
            && self.description == o.description
            && self.value == o.value
            && self.reference == o.reference
    }
    fn row(&self, change: Change) -> Merged {
        Merged {
            key: self.key(),
            kind: None,
            scope: Some(self.scope),
            title: self.name.clone(),
            project: self.project.clone(),
            change,
        }
    }
    fn of(conflict: &Conflicted) -> Option<&Self> {
        conflict.secret.as_ref()
    }
    fn into_conflict(self, at: u64) -> Conflicted {
        Conflicted {
            id: crypto::hex(&crypto::random::<8>()),
            at,
            item: None,
            secret: Some(self),
        }
    }
}

/// Another vault's elements: added when new, set beside ours as a conflict when they differ, never in place of
/// ours. A fresh history for each, so neither side ever reads as knowing the other.
fn add_foreign<T: Element>(
    mine: &mut [T],
    theirs: Vec<T>,
    added: &mut Vec<T>,
    conflicts: &mut Vec<Conflicted>,
    now: u64,
    rows: &mut Vec<Merged>,
) {
    for mut t in theirs {
        let key = t.key();
        *t.revs_mut() = Vec::new();
        new_rev(t.revs_mut());
        match mine.iter_mut().find(|m| m.key() == key) {
            Some(m) if m.same(&t) => rows.push(m.row(Change::Unchanged)),
            Some(m) => {
                rows.push(m.row(Change::Conflict));
                if m.revs().is_empty() {
                    new_rev(m.revs_mut());
                }
                let known = conflicts
                    .iter()
                    .filter_map(T::of)
                    .any(|c| c.key() == key && c.same(&t));
                if !known {
                    conflicts.push(t.into_conflict(now));
                }
            }
            None if added.iter().any(|a| a.key() == key) => {}
            None => {
                rows.push(t.row(Change::Added));
                added.push(t);
            }
        }
    }
}

/// Two versions are the same one: same content, and histories that hold each other.
fn alike<T: Element>(a: &T, b: &T) -> bool {
    fn set(revs: &[String]) -> BTreeSet<&String> {
        revs.iter().collect()
    }
    a.same(b)
        && (set(a.revs()) == set(b.revs())
            || knows(a.revs(), b.revs()) && knows(b.revs(), a.revs()))
}

/// Every version of every element, the current one and the others, by key.
fn versions<'a, T: Element>(
    list: &'a [T],
    conflicts: &'a [Conflicted],
) -> BTreeMap<String, Vec<&'a T>> {
    let mut by_key: BTreeMap<String, Vec<&T>> = BTreeMap::new();
    for v in list.iter().chain(conflicts.iter().filter_map(T::of)) {
        by_key.entry(v.key()).or_default().push(v);
    }
    by_key
}

fn same_versions<T: Element>(a: &BTreeMap<String, Vec<&T>>, b: &BTreeMap<String, Vec<&T>>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, mine)| {
            b.get(key).is_some_and(|theirs| {
                mine.len() == theirs.len()
                    && mine.iter().all(|m| theirs.iter().any(|t| alike(*m, *t)))
                    && theirs.iter().all(|t| mine.iter().any(|m| alike(*m, *t)))
            })
        })
}

fn merge_list<T: Element>(
    mine: &mut Vec<T>,
    theirs: Vec<T>,
    my_tombs: &[Tombstone],
    their_tombs: &[Tombstone],
    conflicts: &mut Vec<Conflicted>,
    now: u64,
    rows: &mut Vec<Merged>,
) {
    let keys: Vec<String> = theirs.iter().map(T::key).collect();
    mine.retain(|m| {
        let key = m.key();
        let gone = !keys.contains(&key)
            && their_tombs
                .iter()
                .any(|t| t.key == key && t.buries(m.revs()));
        if gone {
            rows.push(m.row(Change::Deleted));
        }
        !gone
    });
    for t in theirs {
        let key = t.key();
        match mine.iter_mut().find(|m| m.key() == key) {
            Some(m) if m.same(&t) => {
                union(m.revs_mut(), t.revs());
                rows.push(m.row(Change::Unchanged));
            }
            Some(m) if knows(m.revs(), t.revs()) => rows.push(m.row(Change::Unchanged)),
            Some(m) if knows(t.revs(), m.revs()) => {
                rows.push(t.row(Change::Changed));
                *m = t;
            }
            Some(m) => {
                rows.push(m.row(Change::Conflict));
                let known = conflicts
                    .iter()
                    .filter_map(T::of)
                    .any(|c| c.key() == key && c.same(&t));
                if !known {
                    conflicts.push(t.into_conflict(now));
                }
            }
            None if my_tombs.iter().any(|x| x.key == key && x.buries(t.revs())) => {}
            None => {
                rows.push(t.row(Change::Added));
                mine.push(t);
            }
        }
    }
}

/// Drops the other versions the current one supersedes; brings one back whose element is gone without a
/// tombstone that saw it, and takes one that knows the current version.
fn settle<T: Element>(list: &mut Vec<T>, conflicts: &mut Vec<Conflicted>, tombs: &[Tombstone]) {
    let mut kept: Vec<Conflicted> = Vec::new();
    for c in std::mem::take(conflicts) {
        let Some(v) = T::of(&c).cloned() else {
            kept.push(c);
            continue;
        };
        let key = v.key();
        let buried = tombs.iter().any(|t| t.key == key && t.buries(v.revs()));
        match list.iter().position(|m| m.key() == key) {
            Some(i) if buried || list[i].same(&v) || knows(list[i].revs(), v.revs()) => {}
            Some(i) if knows(v.revs(), list[i].revs()) => list[i] = v,
            Some(_) => {
                if !kept
                    .iter()
                    .filter_map(T::of)
                    .any(|k| k.key() == key && k.same(&v))
                {
                    kept.push(c);
                }
            }
            None if buried => {}
            None => list.push(v),
        }
    }
    *conflicts = kept;
}

impl Content {
    /// Merges another version of the same vault into this one; says what it did, element by element.
    pub(crate) fn merge(&mut self, theirs: Content, now: u64) -> Vec<Merged> {
        let mut rows = Vec::new();
        let mine = std::mem::take(&mut self.tombstones);
        merge_list(
            &mut self.items,
            theirs.items,
            &mine,
            &theirs.tombstones,
            &mut self.conflicts,
            now,
            &mut rows,
        );
        merge_list(
            &mut self.secrets,
            theirs.secrets,
            &mine,
            &theirs.tombstones,
            &mut self.conflicts,
            now,
            &mut rows,
        );
        self.conflicts.extend(theirs.conflicts);
        self.tombstones = mine;
        for t in theirs.tombstones {
            match self.tombstones.iter_mut().find(|x| x.key == t.key) {
                Some(x) => {
                    union(&mut x.revs, &t.revs);
                    x.deleted = x.deleted.max(t.deleted);
                }
                None => self.tombstones.push(t),
            }
        }
        self.settle(now);
        rows
    }

    /// Another vault's export: what it has that this one has not is added, what differs is set beside ours as a
    /// conflict. Never replaces nor deletes anything, whatever its histories and tombstones say.
    pub(crate) fn merge_foreign(&mut self, theirs: Content, now: u64) -> Vec<Merged> {
        let mut rows = Vec::new();
        let mut items = Vec::new();
        add_foreign(
            &mut self.items,
            theirs.items,
            &mut items,
            &mut self.conflicts,
            now,
            &mut rows,
        );
        self.items.extend(items);
        let mut secrets = Vec::new();
        add_foreign(
            &mut self.secrets,
            theirs.secrets,
            &mut secrets,
            &mut self.conflicts,
            now,
            &mut rows,
        );
        self.secrets.extend(secrets);
        self.settle(now);
        rows
    }

    /// Both hold the same versions of every element (current or other, in any order) and the same deletions:
    /// nothing to send either way.
    pub(crate) fn equivalent(&self, other: &Content) -> bool {
        let tombs = |c: &Content| -> BTreeMap<String, BTreeSet<String>> {
            c.tombstones
                .iter()
                .map(|t| (t.key.clone(), t.revs.iter().cloned().collect()))
                .collect()
        };
        same_versions(
            &versions(&self.items, &self.conflicts),
            &versions(&other.items, &other.conflicts),
        ) && same_versions(
            &versions(&self.secrets, &self.conflicts),
            &versions(&other.secrets, &other.conflicts),
        ) && tombs(self) == tombs(other)
    }

    /// After a merge or a change: what supersedes what, and the tombstones still worth keeping.
    pub(crate) fn settle(&mut self, now: u64) {
        settle(&mut self.items, &mut self.conflicts, &self.tombstones);
        settle(&mut self.secrets, &mut self.conflicts, &self.tombstones);
        let present = |key: &str| {
            self.items.iter().any(|i| i.id == key) || self.secrets.iter().any(|s| s.key() == key)
        };
        let kept: Vec<Tombstone> = self
            .tombstones
            .iter()
            .filter(|t| !present(&t.key))
            .filter(|t| now.saturating_sub(t.deleted) < TOMBSTONE_DAYS * 86_400)
            .cloned()
            .collect();
        self.tombstones = kept;
    }

    /// A deletion: the element goes with its other versions, and its tombstone covers every one of them, so a
    /// machine that still shows one of the others deletes it too.
    pub(crate) fn bury(
        &mut self,
        key: String,
        project: Option<String>,
        revs: Vec<String>,
        now: u64,
    ) {
        let others: Vec<Vec<String>> = self
            .conflicts
            .iter()
            .filter(|c| c.key() == key)
            .map(|c| c.revs().to_vec())
            .collect();
        let (mut seen, mut lasts) = (Vec::new(), Vec::new());
        for history in std::iter::once(&revs).chain(&others) {
            if let Some((last, before)) = history.split_last() {
                seen.extend(before.iter().cloned());
                lasts.push(last.clone());
            }
        }
        // The last of each version stays in the trimmed history: what tells the tombstone covers it.
        let mut once = BTreeSet::new();
        lasts.retain(|r| once.insert(r.clone()));
        seen.retain(|r| once.insert(r.clone()));
        seen.extend(lasts);
        trim(&mut seen);
        self.conflicts.retain(|c| c.key() != key);
        self.tombstones.retain(|t| t.key != key);
        self.tombstones.push(Tombstone {
            key,
            project,
            revs: seen,
            deleted: now,
        });
    }

    /// An element made again under a deleted one's key (a secret's name) follows it, rather than conflicting
    /// with a machine that still has the old one.
    pub(crate) fn unbury(&mut self, key: &str) -> Vec<String> {
        let Some(i) = self.tombstones.iter().position(|t| t.key == key) else {
            return Vec::new();
        };
        self.tombstones.remove(i).revs
    }

    /// Settles a conflict with the version kept (`None`: the current one): it then knows every other.
    pub(crate) fn resolve<T: Element>(
        list: &mut [T],
        conflicts: &mut Vec<Conflicted>,
        key: &str,
        keep: Option<&str>,
    ) -> Option<usize> {
        let i = list.iter().position(|m| m.key() == key)?;
        let others: Vec<Conflicted> = conflicts
            .iter()
            .filter(|c| T::of(c).is_some_and(|v| v.key() == key))
            .cloned()
            .collect();
        let chosen = match keep {
            None => list[i].clone(),
            Some(id) => T::of(others.iter().find(|c| c.id == id)?)?.clone(),
        };
        let mut revs = list[i].revs().clone();
        for v in others.iter().filter_map(T::of) {
            union(&mut revs, v.revs());
        }
        union(&mut revs, chosen.revs());
        list[i] = chosen;
        *list[i].revs_mut() = revs;
        new_rev(list[i].revs_mut());
        conflicts.retain(|c| T::of(c).is_none_or(|v| v.key() != key));
        Some(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Secret;

    fn item(id: &str, title: &str, revs: &[&str]) -> Item {
        Item {
            id: id.into(),
            kind: Kind::Login,
            project: None,
            title: title.into(),
            urls: Vec::new(),
            username: "alex".into(),
            password: Some(Secret::new(format!("pw-{title}"))),
            totp: None,
            notes: None,
            agents: false,
            created: 1,
            updated: 1,
            revs: revs.iter().map(|r| r.to_string()).collect(),
        }
    }

    fn content(items: Vec<Item>) -> Content {
        Content {
            items,
            ..Content::default()
        }
    }

    fn titles(c: &Content) -> Vec<&str> {
        let mut t: Vec<&str> = c.items.iter().map(|i| i.title.as_str()).collect();
        t.sort();
        t
    }

    fn changes(rows: &[Merged]) -> Vec<(String, Change)> {
        let mut r: Vec<_> = rows.iter().map(|r| (r.title.clone(), r.change)).collect();
        r.sort_by(|a, b| a.0.cmp(&b.0));
        r
    }

    #[test]
    fn takes_what_one_side_changed_and_keeps_both_when_both_did() {
        let mut mine = content(vec![
            item("a", "same", &["1"]),
            item("b", "b mine", &["1", "2"]),
            item("c", "c old", &["1"]),
            item("d", "d mine", &["1", "2"]),
            item("e", "only mine", &["1"]),
        ]);
        let theirs = content(vec![
            item("a", "same", &["1"]),
            item("b", "b old", &["1"]),
            item("c", "c theirs", &["1", "3"]),
            item("d", "d theirs", &["1", "3"]),
            item("f", "only theirs", &["9"]),
        ]);
        let rows = mine.merge(theirs, 100);
        assert_eq!(
            titles(&mine),
            [
                "b mine",
                "c theirs",
                "d mine",
                "only mine",
                "only theirs",
                "same"
            ]
        );
        assert_eq!(
            changes(&rows),
            [
                ("b mine".into(), Change::Unchanged),
                ("c theirs".into(), Change::Changed),
                ("d mine".into(), Change::Conflict),
                ("only theirs".into(), Change::Added),
                ("same".into(), Change::Unchanged),
            ]
        );
        assert_eq!(mine.conflicts.len(), 1);
        assert_eq!(mine.conflicts[0].item.as_ref().unwrap().title, "d theirs");
        assert_eq!(summary(&rows), "1 added, 1 changed, 1 conflict");

        // Merging the same again finds nothing new and does not pile up conflicts.
        let again = content(vec![item("d", "d theirs", &["1", "3"])]);
        mine.merge(again, 200);
        assert_eq!(mine.conflicts.len(), 1);
    }

    #[test]
    fn a_tombstone_removes_only_the_versions_it_saw() {
        let mut deleted = content(vec![item("b", "b", &["1"])]);
        deleted.bury("a".into(), None, vec!["1".into()], 100);
        deleted.bury("b2".into(), None, vec!["1".into()], 100);
        let mut mine = content(vec![
            item("a", "seen", &["1"]),
            item("b2", "edited after", &["1", "2"]),
        ]);
        let rows = mine.merge(deleted.clone(), 150);
        assert_eq!(titles(&mine), ["b", "edited after"]);
        assert!(
            rows.iter()
                .any(|r| r.title == "seen" && r.change == Change::Deleted)
        );
        assert_eq!(
            mine.tombstones.len(),
            1,
            "the one whose element came back goes"
        );

        // The side that deleted gets the edit back, and keeps the deleted one deleted.
        let rows = deleted.merge(mine, 160);
        assert_eq!(titles(&deleted), ["b", "edited after"]);
        assert!(rows.iter().all(|r| r.title != "seen"));
    }

    #[test]
    fn tombstones_are_forgotten_after_their_retention() {
        let mut c = content(Vec::new());
        c.bury("a".into(), None, vec!["1".into()], 0);
        c.settle(TOMBSTONE_DAYS * 86_400 - 1);
        assert_eq!(c.tombstones.len(), 1);
        c.settle(TOMBSTONE_DAYS * 86_400);
        assert!(c.tombstones.is_empty());
    }

    #[test]
    fn versions_from_before_histories_conflict_rather_than_vanish() {
        let mut mine = content(vec![item("a", "mine", &[])]);
        mine.merge(content(vec![item("a", "theirs", &[])]), 1);
        assert_eq!(titles(&mine), ["mine"]);
        assert_eq!(mine.conflicts.len(), 1);
        // A version with a history knows the one from before.
        mine.merge(content(vec![item("a", "edited", &["7"])]), 2);
        assert_eq!(titles(&mine), ["edited"]);
        assert!(
            mine.conflicts.is_empty(),
            "both older versions are superseded"
        );
    }

    #[test]
    fn a_resolved_conflict_supersedes_both_sides_everywhere() {
        let mut a = content(vec![item("x", "from a", &["1", "2"])]);
        let mut b = content(vec![item("x", "from b", &["1", "3"])]);
        a.merge(b.clone(), 10);
        b.merge(a.clone(), 10);
        assert_eq!((a.conflicts.len(), b.conflicts.len()), (1, 1));

        let other = b.conflicts[0].id.clone();
        Content::resolve(&mut b.items, &mut b.conflicts, "x", Some(&other)).unwrap();
        assert_eq!(titles(&b), ["from a"]);
        assert!(b.conflicts.is_empty());

        let rows = a.merge(b.clone(), 20);
        assert_eq!(titles(&a), ["from a"]);
        assert!(a.conflicts.is_empty(), "{rows:?}");
        assert_eq!(rows[0].change, Change::Unchanged);
        assert!(
            knows(&a.items[0].revs, &b.items[0].revs) && knows(&b.items[0].revs, &a.items[0].revs)
        );
    }

    #[test]
    fn an_other_version_of_an_element_deleted_unseen_comes_back() {
        let mut b = content(vec![item("x", "b", &["1", "2"])]);
        b.conflicts
            .push(item("x", "c", &["1", "3"]).into_conflict(5));
        let mut a = content(Vec::new());
        a.bury("x".into(), None, vec!["1".into(), "2".into()], 6);
        b.merge(a, 7);
        assert_eq!(titles(&b), ["c"], "the deletion never saw this one");
        assert!(b.conflicts.is_empty());
    }

    #[test]
    fn deleting_an_element_buries_its_other_versions_too() {
        let mut a = content(vec![item("x", "a", &["1", "2"])]);
        a.conflicts
            .push(item("x", "b", &["1", "3"]).into_conflict(5));
        let mut b = content(vec![item("x", "b", &["1", "3"])]);
        b.conflicts
            .push(item("x", "a", &["1", "2"]).into_conflict(5));
        a.items.clear();
        a.bury("x".into(), None, vec!["1".into(), "2".into()], 6);
        assert!(a.conflicts.is_empty());
        assert_eq!(a.tombstones[0].revs, ["1", "2", "3"]);
        let rows = b.merge(a.clone(), 7);
        assert!(b.items.is_empty() && b.conflicts.is_empty(), "{rows:?}");
        a.merge(content(vec![item("x", "b", &["1", "3"])]), 8);
        assert!(a.items.is_empty(), "the other version stays deleted");
        // An edit the deletion never saw comes back; an other version it saw does not.
        let mut c = content(vec![item("x", "c", &["1", "3", "4"])]);
        c.conflicts
            .push(item("x", "a", &["1", "2"]).into_conflict(5));
        c.merge(a, 9);
        assert_eq!(titles(&c), ["c"]);
        assert!(c.conflicts.is_empty());
    }

    #[test]
    fn another_vaults_versions_never_replace_nor_delete() {
        let mut mine = content(vec![item("x", "mine", &[]), item("y", "kept", &["1"])]);
        mine.bury("z".into(), None, Vec::new(), 1);
        let mut theirs = content(vec![
            item("x", "theirs", &["9"]),
            item("z", "theirs z", &[]),
        ]);
        theirs.bury("y".into(), None, vec!["1".into()], 1);
        let rows = mine.merge_foreign(theirs, 2);
        assert_eq!(titles(&mine), ["kept", "mine", "theirs z"]);
        assert_eq!(mine.conflicts.len(), 1, "{rows:?}");
        assert_eq!(mine.conflicts[0].item.as_ref().unwrap().title, "theirs");
    }

    #[test]
    fn equivalent_contents_hold_the_same_versions_in_any_order() {
        let mut a = content(vec![item("x", "a", &["1", "2"]), item("y", "y", &["1"])]);
        a.conflicts
            .push(item("x", "b", &["1", "3"]).into_conflict(5));
        let mut b = content(vec![item("y", "y", &["1"]), item("x", "b", &["1", "3"])]);
        b.conflicts
            .push(item("x", "a", &["1", "2"]).into_conflict(6));
        assert!(a.equivalent(&b) && b.equivalent(&a));
        b.items[0].revs.push("4".into());
        assert!(!a.equivalent(&b), "a newer version of y");
    }

    #[test]
    fn histories_stay_bounded_and_keep_both_lasts() {
        let mut revs: Vec<String> = (0..40).map(|i| i.to_string()).collect();
        trim(&mut revs);
        assert_eq!(revs.len(), HISTORY);
        let theirs: Vec<String> = (100..140).map(|i| i.to_string()).collect();
        union(&mut revs, &theirs);
        assert_eq!(revs.len(), HISTORY);
        assert!(knows(&revs, &theirs) && revs.last().unwrap() == "39");
    }
}
