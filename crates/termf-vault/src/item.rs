//! The password manager's items, kept beside the secrets in the same sealed file: logins (with their TOTP) and
//! secure notes, global or for a project. Each has a stable id and its own `updated`, for an item-by-item merge.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::totp::{Algorithm, Totp};
use crate::{MAX_VALUE, Secret, valid_project};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Login,
    Note,
}

/// What [`crate::Vault::item_value`] gives.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Field {
    Password,
    /// The code valid now.
    Totp,
    /// The TOTP secret itself (base32).
    Seed,
    Notes,
}

impl Field {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Totp => "totp",
            Self::Seed => "seed",
            Self::Notes => "notes",
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Item {
    pub id: String,
    pub kind: Kind,
    pub project: Option<String>,
    pub title: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<Secret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp: Option<Totp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Secret>,
    /// An agent may use its values (never see them): off unless the user turns it on.
    #[serde(default)]
    pub agents: bool,
    pub created: u64,
    pub updated: u64,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TotpInfo {
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u64,
}

/// What the vault says about an item, never a value.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ItemInfo {
    pub id: String,
    pub kind: Kind,
    pub project: Option<String>,
    pub title: String,
    pub urls: Vec<String>,
    pub username: String,
    pub has_password: bool,
    pub totp: Option<TotpInfo>,
    pub has_notes: bool,
    pub agents: bool,
    pub created: u64,
    pub updated: u64,
}

/// A new item (`id: None`) or the changes to one. `password`, `totp` and `notes`: `None` keeps what it has, empty
/// removes it.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ItemDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: Kind,
    #[serde(default)]
    pub project: Option<String>,
    pub title: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Option<Secret>,
    /// A base32 secret or an `otpauth://totp/` link.
    #[serde(default)]
    pub totp: Option<Secret>,
    #[serde(default)]
    pub notes: Option<Secret>,
    #[serde(default)]
    pub agents: bool,
}

impl Item {
    pub fn info(&self) -> ItemInfo {
        ItemInfo {
            id: self.id.clone(),
            kind: self.kind,
            project: self.project.clone(),
            title: self.title.clone(),
            urls: self.urls.clone(),
            username: self.username.clone(),
            has_password: self.password.is_some(),
            totp: self.totp.as_ref().map(|t| TotpInfo {
                algorithm: t.algorithm,
                digits: t.digits,
                period: t.period,
            }),
            has_notes: self.notes.is_some(),
            agents: self.agents,
            created: self.created,
            updated: self.updated,
        }
    }

    /// Applies a draft already checked by [`ItemDraft::check`].
    pub fn apply(&mut self, draft: ItemDraft, totp: Option<Option<Totp>>, now: u64) {
        let keep = |new: Option<Secret>, old: Option<Secret>| match new {
            None => old,
            Some(v) if v.is_empty() => None,
            Some(v) => Some(v),
        };
        self.kind = draft.kind;
        self.project = draft.project;
        self.title = draft.title.trim().into();
        self.agents = draft.agents;
        self.notes = keep(draft.notes, self.notes.take());
        if draft.kind == Kind::Login {
            self.urls = draft
                .urls
                .iter()
                .map(|u| u.trim().to_owned())
                .filter(|u| !u.is_empty())
                .collect();
            self.username = draft.username.trim().into();
            self.password = keep(draft.password, self.password.take());
            if let Some(totp) = totp {
                self.totp = totp;
            }
        } else {
            self.urls.clear();
            self.username.clear();
            self.password = None;
            self.totp = None;
        }
        self.updated = now;
    }
}

impl ItemDraft {
    /// Checks the draft; the TOTP to keep: `None` unchanged, `Some(None)` removed. Errors never quote a value.
    pub fn check(&self) -> Result<Option<Option<Totp>>> {
        let title = self.title.trim();
        ensure!(
            !title.is_empty() && title.chars().count() <= 200,
            "an item needs a title (200 characters at most)"
        );
        if let Some(project) = &self.project {
            ensure!(valid_project(project), "invalid project id");
        }
        ensure!(
            self.urls.len() <= 20 && self.urls.iter().all(|u| u.len() <= 2048),
            "20 addresses at most, of 2,048 characters each"
        );
        ensure!(
            self.username.chars().count() <= 512,
            "the user name is longer than 512 characters"
        );
        for value in [&self.password, &self.notes].into_iter().flatten() {
            ensure!(value.len() <= MAX_VALUE, "a value is longer than 64 KiB");
        }
        Ok(match &self.totp {
            None => None,
            Some(t) if t.expose().trim().is_empty() => Some(None),
            Some(t) => Some(Some(Totp::parse(t.expose())?)),
        })
    }
}
