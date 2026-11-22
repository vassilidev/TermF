//! What the status line of a session TermF started says about usage: its cost, and the account's 5-hour and
//! 7-day limits (code.claude.com/docs/en/statusline). Claude Code gives nothing else outside a session.

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// 0 to 100 (above 100 past a spend limit).
    pub used_percentage: f64,
    /// Seconds since the epoch.
    pub resets_at: Option<u64>,
    /// A spend limit's dollars, when Claude Code has them.
    pub used_usd: Option<f64>,
    pub limit_usd: Option<f64>,
}

/// Each window may be absent: only claude.ai subscribers (or a gateway's spend limit) have them, after the
/// session's first answer.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub spend_limit: Option<Window>,
}

impl Limits {
    pub fn is_empty(&self) -> bool {
        self.five_hour.is_none() && self.seven_day.is_none() && self.spend_limit.is_none()
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Reading {
    /// The session's estimated cost so far, at list price.
    pub cost: Option<f64>,
    pub model: Option<String>,
    pub limits: Limits,
}

fn window(value: &Value) -> Option<Window> {
    Some(Window {
        used_percentage: value["used_percentage"].as_f64()?,
        resets_at: value["resets_at"].as_u64(),
        used_usd: value["used_usd"].as_f64(),
        limit_usd: value["limit_usd"].as_f64(),
    })
}

pub fn read(status_line: &Value) -> Reading {
    let limits = &status_line["rate_limits"];
    Reading {
        cost: status_line["cost"]["total_cost_usd"].as_f64(),
        model: status_line["model"]["display_name"]
            .as_str()
            .or(status_line["model"]["id"].as_str())
            .map(str::to_owned),
        limits: Limits {
            five_hour: window(&limits["five_hour"]),
            seven_day: window(&limits["seven_day"]),
            spend_limit: window(&limits["spend_limit"]),
        },
    }
}

/// From it, a window is nearly used up: TermF warns, and the CTO stays light.
pub const NEARLY_USED: f64 = 80.0;

impl Limits {
    /// The fuller of the 5-hour and 7-day windows, named, when it is at `threshold` percent or more.
    pub fn pressure(&self, threshold: f64) -> Option<(&'static str, &Window)> {
        [
            ("five_hour", &self.five_hour),
            ("seven_day", &self.seven_day),
        ]
        .into_iter()
        .filter_map(|(name, w)| Some((name, w.as_ref()?)))
        .filter(|(_, w)| w.used_percentage >= threshold)
        .max_by(|a, b| a.1.used_percentage.total_cmp(&b.1.used_percentage))
    }
}

/// How the account pays for what Claude Code spends. Its dollars are always list prices: real money only with
/// an API key; a subscription draws on its usage limits instead.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Billing {
    /// `subscription` (claude.ai), `api` (an API key, a key helper or a gateway), `cloud` (Bedrock, Vertex,
    /// Foundry: billed by that cloud), `none` (logged out) or `unknown`.
    pub mode: &'static str,
    /// The subscription's plan as `claude auth status` names it: `pro`, `max`, `team`, `enterprise`…
    pub plan: Option<String>,
}

/// From `claude auth status` (code.claude.com/docs/en/cli-reference: `authMethod` is `none`, `claude.ai`,
/// `oauth_token`, `api_key`, `api_key_helper` or `third_party`; `apiProvider` and `subscriptionType` are not
/// documented but printed by 2.1.330).
pub fn billing(auth: Option<&Value>) -> Billing {
    let Some(auth) = auth else {
        return Billing {
            mode: "unknown",
            plan: None,
        };
    };
    let provider = auth["apiProvider"].as_str().unwrap_or("firstParty");
    let mode = match auth["authMethod"].as_str() {
        _ if provider != "firstParty" => "cloud",
        Some("third_party") => "cloud",
        Some("claude.ai" | "oauth_token") => "subscription",
        Some("api_key" | "api_key_helper") => "api",
        _ if auth["loggedIn"] == false => "none",
        Some("none") => "none",
        _ => "unknown",
    };
    let plan = auth["subscriptionType"]
        .as_str()
        .filter(|p| mode == "subscription" && !p.is_empty())
        .map(str::to_owned);
    Billing { mode, plan }
}
