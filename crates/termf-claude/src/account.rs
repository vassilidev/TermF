//! The account and the installation: logging out, updating and checking Claude Code through its own commands.
//! Logging in is interactive (a browser, maybe a code to paste), so it runs in a tile, never here.

use std::time::Duration;

use anyhow::Result;

use crate::Env;
use crate::sessions::{QUICK, claude, printed};

/// `claude update` downloads a build.
const SLOW: Duration = Duration::from_secs(300);

pub fn logout(env: &Env) -> Result<String> {
    printed(claude(env, &["auth", "logout"], QUICK)?)
}

pub fn update(env: &Env) -> Result<String> {
    printed(claude(env, &["update"], SLOW)?)
}

/// Read-only diagnostics: install, settings files, Remote Control.
pub fn doctor(env: &Env) -> Result<String> {
    printed(claude(env, &["doctor"], QUICK)?)
}
