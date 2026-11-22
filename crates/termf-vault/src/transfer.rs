//! Export and import. An export holds the vault, or one project's part, sealed by a password of its own
//! (Argon2id and XChaCha20-Poly1305, as the sync), and imports as the sync merges: item by item. A plain CSV
//! only when the user asks, with the master password again. Files other managers export come in as new items,
//! after a preview that flags duplicates; they are never written back.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use termf_sync::crypto::{self, Kdf};
use zeroize::Zeroizing;

use crate::foreign::{self, Candidate};
use crate::item::{Item, Kind};
use crate::merge::{Change, Merged, new_rev, summary};
use crate::totp::Totp;
use crate::{
    Action, Actor, Content, Event, MAX_VALUE, MIN_PASSWORD, Secret, SecretId, Vault, kdf,
    valid_project,
};

const KIND: &str = "termf-vault-export";
const EXPORT_FORMAT: u32 = 1;
const AAD: &[u8] = b"termf-vault export";
const DAMAGED: &str = "this vault export is damaged or was not written by TermF";

/// `*.termfvault`: nothing in it is readable without its password.
#[derive(Serialize, Deserialize)]
struct ExportFile {
    termf: String,
    format: u32,
    /// The vault it comes from, bound to the content: another vault's export never replaces nor deletes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vault: Option<String>,
    kdf: Kdf,
    salt: String,
    data: String,
}

fn export_aad(vault: Option<&str>) -> Vec<u8> {
    match vault {
        Some(id) => [AAD, b" ", id.as_bytes()].concat(),
        None => AAD.to_vec(),
    }
}

/// What a file holds is written first; its entry, once written.
pub type ExportWrite<'a> = &'a mut dyn FnMut(&[u8]) -> Result<()>;

/// Whether `bytes` is a TermF vault export (sealed), rather than another manager's file.
pub fn is_export(bytes: &[u8]) -> bool {
    serde_json::from_slice::<ExportFile>(bytes).is_ok_and(|f| f.termf == KIND)
}

/// The vault it comes from, and its content.
fn read_export(bytes: &[u8], password: &str) -> Result<(Option<String>, Content)> {
    let file: ExportFile = serde_json::from_slice::<ExportFile>(bytes)
        .ok()
        .filter(|f| f.termf == KIND)
        .context("this is not a TermF vault export")?;
    ensure!(
        file.format <= EXPORT_FORMAT,
        "this export was written by a newer TermF"
    );
    let salt = crypto::unhex(&file.salt).context(DAMAGED)?;
    let key = crypto::derive(password.as_bytes(), &salt, file.kdf)?;
    let plain = crypto::unhex(&file.data)
        .and_then(|data| crypto::open(&key, &export_aad(file.vault.as_deref()), &data))
        .map(Zeroizing::new)
        .context("wrong export password")?;
    // Not the parser's message: it may quote the content.
    let content = serde_json::from_slice(&plain).map_err(|_| anyhow!(DAMAGED))?;
    Ok((file.vault, content))
}

impl Content {
    /// An export merges as the sync does when it comes from this vault; from another one, it only adds.
    fn merge_export(
        &mut self,
        theirs: Content,
        from: Option<&str>,
        here: &str,
        now: u64,
    ) -> Vec<Merged> {
        if from == Some(here) {
            self.merge(theirs, now)
        } else {
            self.merge_foreign(theirs, now)
        }
    }
}

impl Content {
    /// The elements of one project (`None`: all), with their tombstones; never the other versions in conflict.
    fn part(&self, project: Option<&str>) -> Content {
        let ours = |p: &Option<String>| project.is_none() || p.as_deref() == project;
        Content {
            secrets: self
                .secrets
                .iter()
                .filter(|s| ours(&s.project))
                .cloned()
                .collect(),
            items: self
                .items
                .iter()
                .filter(|i| ours(&i.project))
                .cloned()
                .collect(),
            tombstones: self
                .tombstones
                .iter()
                .filter(|t| ours(&t.project))
                .cloned()
                .collect(),
            conflicts: Vec::new(),
        }
    }
}

fn what(items: usize, secrets: usize, project: Option<&str>) -> String {
    let project = project
        .map(|p| format!(", project {p}"))
        .unwrap_or_default();
    format!("{items} items, {secrets} secrets{project}")
}

/// A field of the CSV: quoted when it must be, and never read as a formula by a spreadsheet (an apostrophe first).
fn cell(out: &mut String, text: &str) {
    let mut text = Zeroizing::new(text.to_owned());
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        text.insert(0, '\'');
    }
    if text.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&Zeroizing::new(text.replace('"', "\"\"")));
        out.push('"');
    } else {
        out.push_str(&text);
    }
}

/// One line of another manager's file, as the preview shows it: never a value.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignRow {
    pub index: usize,
    pub kind: Kind,
    pub title: String,
    pub username: String,
    pub urls: Vec<String>,
    /// Its folder (group, vault, collection) in the other manager: `""` for none.
    pub folder: String,
    pub has_password: bool,
    pub has_totp: bool,
    pub has_notes: bool,
    /// Same address and user name as this item already in the vault, or as a line before it (`""`).
    pub duplicate: Option<String>,
    /// `trash`: in the other manager's trash; `totp`: its TOTP is not one TermF reads, so it goes into the
    /// notes.
    pub problem: Option<&'static str>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForeignPreview {
    /// Which manager and format: `bitwarden-json`, `bitwarden-csv`, `1password-1pux`, `1password-csv`,
    /// `chrome-csv`, `firefox-csv`, `safari-csv`, `keepassxc-csv`, `keepass-xml`.
    pub source: &'static str,
    /// The file previewed: the import refuses another one.
    pub digest: String,
    pub rows: Vec<ForeignRow>,
    /// Its folders, for the user to put each in a project.
    pub folders: Vec<String>,
    /// Elements TermF has no place for (cards, identities, documents…), left out.
    pub unsupported: usize,
}

/// What to import from a previewed file.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForeignChoice {
    pub digest: String,
    /// Lines left out (duplicates are, by default, in the interface).
    #[serde(default)]
    pub skip: Vec<usize>,
    /// A folder's project; a folder missing here, or `None`, goes global.
    #[serde(default)]
    pub projects: BTreeMap<String, Option<String>>,
}

/// A file's fingerprint: what tells a previewed or imported file from another.
pub fn digest(bytes: &[u8]) -> String {
    crypto::hex(&Sha256::digest(bytes)[..16])
}

fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.to_lowercase();
    host.strip_prefix("www.").unwrap_or(&host).to_owned()
}

/// What makes two logins the same account: an address's host and the user name (a title without address).
fn same_account(kind: Kind, title: &str, urls: &[String], username: &str) -> Vec<String> {
    let user = username.trim().to_lowercase();
    let hosts: BTreeSet<String> = urls
        .iter()
        .map(|u| host(u.trim()))
        .filter(|h| !h.is_empty())
        .collect();
    if hosts.is_empty() {
        return vec![format!("{kind:?}\n{}\n{user}", title.trim().to_lowercase())];
    }
    hosts.into_iter().map(|h| format!("{h}\n{user}")).collect()
}

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

impl Vault {
    /// The vault (`project: None`) or one project's elements, sealed by `password`, an export password of its
    /// own: a value never leaves in clear. Asks for the master password again, like changing how it opens.
    pub fn export_sealed(
        &self,
        actor: &Actor,
        project: Option<&str>,
        password: &str,
        write: ExportWrite,
    ) -> Result<()> {
        ensure!(
            password.chars().count() >= MIN_PASSWORD,
            "the export password needs {MIN_PASSWORD} characters or more"
        );
        let (id, _) = self.confirmed()?;
        let part = self.open()?.part(project);
        let salt: [u8; 16] = crypto::random();
        let kdf = kdf();
        let key = crypto::derive(password.as_bytes(), &salt, kdf)?;
        let plain = Zeroizing::new(serde_json::to_vec(&part)?);
        let sealed = crypto::seal(&key, &export_aad(Some(&id)), &plain);
        let file = ExportFile {
            termf: KIND.into(),
            format: EXPORT_FORMAT,
            vault: Some(id),
            kdf,
            salt: crypto::hex(&salt),
            data: crypto::hex(&sealed),
        };
        write(&serde_json::to_vec_pretty(&file)?)?;
        self.record(Event {
            detail: Some(format!(
                "encrypted: {}",
                what(part.items.len(), part.secrets.len(), project)
            )),
            ..Event::new(actor, Action::Export)
        })?;
        Ok(())
    }

    /// The logins and notes (of one project, or all) in clear, as a CSV other managers read (Bitwarden's
    /// columns): only when the user asks, with the master password again, and audited. The secrets (dev,
    /// staging, prod) never go there.
    pub fn export_csv(
        &self,
        actor: &Actor,
        project: Option<&str>,
        write: ExportWrite,
    ) -> Result<()> {
        self.confirmed()?;
        let part = self.open()?.part(project);
        let mut out = Zeroizing::new(String::from(
            "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n",
        ));
        for item in &part.items {
            let kind = match item.kind {
                Kind::Login => "login",
                Kind::Note => "note",
            };
            let totp = item.totp.as_ref().map(|t| t.uri(&item.title));
            let cells: [&str; 11] = [
                item.project.as_deref().unwrap_or_default(),
                "",
                kind,
                &item.title,
                item.notes.as_ref().map_or("", Secret::expose),
                "",
                "0",
                &item.urls.join(","),
                &item.username,
                item.password.as_ref().map_or("", Secret::expose),
                totp.as_ref().map_or("", |t| t.expose()),
            ];
            for (i, text) in cells.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                cell(&mut out, text);
            }
            out.push('\n');
        }
        write(out.as_bytes())?;
        self.record(Event {
            detail: Some(format!("plain CSV: {}", what(part.items.len(), 0, project))),
            ..Event::new(actor, Action::Export)
        })?;
        Ok(())
    }

    /// What importing an export would do, element by element: titles, never a value.
    pub fn preview_import(&self, bytes: &[u8], password: &str) -> Result<Vec<Merged>> {
        let (from, theirs) = read_export(bytes, password)?;
        let (here, _) = self.key()?;
        let mut content = self.open()?;
        Ok(content.merge_export(theirs, from.as_deref(), &here, self.clock.wall()))
    }

    /// Merges an export of this vault as the sync merges another machine's version: both versions kept when
    /// both changed, deletions only of what they saw. Another vault's only adds, or sets its versions beside
    /// ours. Writing a prod secret asks for the master password again, as by hand.
    pub fn import_sealed(
        &self,
        actor: &Actor,
        bytes: &[u8],
        password: &str,
    ) -> Result<Vec<Merged>> {
        let (from, theirs) = read_export(bytes, password)?;
        let (here, _) = self.key()?;
        let from = from.as_deref();
        let now = self.clock.wall();
        let rows = self.open()?.merge_export(theirs.clone(), from, &here, now);
        let prod = rows
            .iter()
            .find(|r| r.scope == Some(crate::Scope::Prod) && r.change != Change::Unchanged);
        if let Some(row) = prod {
            let id = SecretId {
                name: row.title.clone(),
                scope: crate::Scope::Prod,
                project: row.project.clone(),
            };
            self.prod_gate(actor, &id)?;
        }
        let event = Event {
            detail: Some(format!("TermF export: {}", summary(&rows))),
            ..Event::new(actor, Action::Import)
        };
        self.modify(event, |content| {
            Ok(content.merge_export(theirs, from, &here, now))
        })
    }

    /// Another manager's export, read and compared with the vault: what each line would become, which ones
    /// look like accounts already here, never a value.
    pub fn preview_foreign(&self, bytes: &[u8]) -> Result<ForeignPreview> {
        let parsed = foreign::parse(bytes)?;
        let content = self.open()?;
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for item in &content.items {
            for key in same_account(item.kind, &item.title, &item.urls, &item.username) {
                seen.entry(key).or_insert_with(|| item.title.clone());
            }
        }
        let mut rows = Vec::new();
        let mut folders = BTreeSet::new();
        for (index, c) in parsed.candidates.iter().enumerate() {
            let keys = same_account(c.kind, &c.title, &c.urls, &c.username);
            let duplicate = keys.iter().find_map(|k| seen.get(k).cloned());
            for key in keys {
                seen.entry(key).or_default();
            }
            folders.insert(c.folder.clone());
            rows.push(ForeignRow {
                index,
                kind: c.kind,
                title: c.title.clone(),
                username: c.username.clone(),
                urls: c.urls.clone(),
                folder: c.folder.clone(),
                has_password: c.password.is_some(),
                has_totp: c.totp.is_some(),
                has_notes: c.notes.is_some(),
                duplicate,
                problem: if c.trash {
                    Some("trash")
                } else {
                    c.totp
                        .as_ref()
                        .filter(|t| Totp::parse(t.expose()).is_err())
                        .map(|_| "totp")
                },
            });
        }
        Ok(ForeignPreview {
            source: parsed.source,
            digest: digest(bytes),
            rows,
            folders: folders.into_iter().collect(),
            unsupported: parsed.unsupported,
        })
    }

    /// Imports the lines of a previewed file as new items, each folder in the project chosen for it. Returns
    /// how many came in.
    pub fn import_foreign(
        &self,
        actor: &Actor,
        bytes: &[u8],
        choice: &ForeignChoice,
    ) -> Result<usize> {
        ensure!(
            digest(bytes) == choice.digest,
            "the file changed since its preview: preview it again"
        );
        for project in choice.projects.values().flatten() {
            ensure!(valid_project(project), "invalid project id");
        }
        let parsed = foreign::parse(bytes)?;
        let now = self.clock.wall();
        let items: Vec<Item> = parsed
            .candidates
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !choice.skip.contains(i))
            .map(|(_, c)| item(c, choice, now))
            .collect();
        let count = items.len();
        let event = Event {
            detail: Some(format!(
                "{}: {count} items, {} left out",
                parsed.source,
                choice.skip.len()
            )),
            ..Event::new(actor, Action::Import)
        };
        self.modify(event, |content| {
            content.items.extend(items);
            Ok(count)
        })
    }
}

/// A new item from another manager's line, clipped to the vault's limits rather than refused.
fn item(c: Candidate, choice: &ForeignChoice, now: u64) -> Item {
    let mut notes = c.notes.map(|n| Zeroizing::new(n.expose().to_owned()));
    let totp = match c.totp.as_ref().map(|t| Totp::parse(t.expose())) {
        Some(Ok(totp)) => Some(totp),
        Some(Err(_)) => {
            // Not lost: kept where the user can read it and fix it.
            let raw = c.totp.as_ref().map(Secret::expose).unwrap_or_default();
            let text = notes.get_or_insert_with(Default::default);
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str("TOTP: ");
            text.push_str(raw);
            None
        }
        None => None,
    };
    let value = |text: &str| -> Option<Secret> {
        let end = text.floor_char_boundary(MAX_VALUE);
        (!text.is_empty()).then(|| Secret::new(text[..end].to_owned()))
    };
    let mut revs = Vec::new();
    new_rev(&mut revs);
    let login = c.kind == Kind::Login;
    Item {
        id: crypto::hex(&crypto::random::<16>()),
        kind: c.kind,
        project: choice.projects.get(&c.folder).cloned().flatten(),
        title: Some(clip(&c.title, 200))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "?".into()),
        urls: if login {
            c.urls
                .iter()
                .take(20)
                .map(|u| clip(u, 2048))
                .filter(|u| !u.is_empty())
                .collect()
        } else {
            Vec::new()
        },
        username: if login {
            clip(&c.username, 512)
        } else {
            String::new()
        },
        password: c.password.filter(|_| login).and_then(|p| value(p.expose())),
        totp: totp.filter(|_| login),
        notes: notes.and_then(|n| value(&n)),
        agents: false,
        created: now,
        updated: now,
        revs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_same_account_by_host_and_user() {
        let a = same_account(
            Kind::Login,
            "GitHub",
            &["https://www.GitHub.com/login".into()],
            "Alex",
        );
        let b = same_account(Kind::Login, "gh", &["github.com".into()], "alex ");
        assert_eq!(a, b);
        let c = same_account(
            Kind::Login,
            "GitHub",
            &["https://alex@github.com:443/x".into()],
            "bob",
        );
        assert_ne!(a, c);
        assert_eq!(host("https://alex@github.com:443/x"), "github.com:443");
        let note = same_account(Kind::Note, "Wi-Fi", &[], "");
        assert_eq!(note, same_account(Kind::Note, " wi-fi", &[], ""));
    }

    #[test]
    fn quotes_csv_cells_only_when_needed() {
        let mut out = String::new();
        cell(&mut out, "plain");
        out.push(',');
        cell(&mut out, "a, \"b\"\nc");
        assert_eq!(out, "plain,\"a, \"\"b\"\"\nc\"");
        for formula in ["=1+1", "+cmd", "-2", "@SUM(A1)", "\tx"] {
            let mut out = String::new();
            cell(&mut out, formula);
            assert!(out.trim_start_matches('"').starts_with('\''), "{out}");
        }
        let mut out = String::new();
        cell(&mut out, "=HYPERLINK(\"x\", \"y\")");
        assert_eq!(out, "\"'=HYPERLINK(\"\"x\"\", \"\"y\"\")\"");
    }
}
