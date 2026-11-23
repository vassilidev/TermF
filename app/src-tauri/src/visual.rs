//! Visual reviews before / after, for the UI and, through the local API, for the CLI. Every change of a review
//! goes out as `visual-updated` (a `Review`) and the progress of a capture as `visual-progress`. The team's
//! visual gate starts a review version, and a decided version goes back to the gate.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::ipc::ReviewCall;
use termf_visual::{Decision, FileDiff, Pin, PrPreview, Request, Review, Update, Visual};

use crate::CmdResult;
use crate::tickets::blocking;

pub struct VisualState(Visual);

pub fn state(app: &AppHandle, config_dir: PathBuf, data_dir: PathBuf) -> VisualState {
    let handle = app.clone();
    let visual = Visual::new(config_dir, data_dir, move |update| {
        let _ = match update {
            Update::Review(review) => handle.emit("visual-updated", review),
            Update::Progress(progress) => handle.emit("visual-progress", progress),
        };
    });
    let handle = app.clone();
    visual.on_gate(move |run, step, outcome| {
        termf_visual::to_team(crate::team::team(&handle), run, step, outcome)
    });
    let hook = visual.clone();
    crate::team::team(app).on_visual(move |run, step| hook.gate(run, step));
    VisualState(visual)
}

pub(crate) fn visual(app: &AppHandle) -> &Visual {
    &app.state::<VisualState>().inner().0
}

/// The app quits: the dev servers and browsers of a capture are stopped.
pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<VisualState>() {
        state.0.shutdown();
    }
}

fn decide(
    app: &AppHandle,
    review: i64,
    screen: &str,
    decision: &str,
    comment: Option<String>,
    pins: Option<Vec<Pin>>,
) -> Result<Review> {
    visual(app).decide(review, screen, Decision::parse(decision)?, comment, pins)
}

fn pr_comment(app: &AppHandle, review: i64, confirm: Option<&str>) -> Result<PrPreview> {
    match confirm {
        Some(confirm) => visual(app).pr_comment(review, confirm),
        None => visual(app).pr_preview(review),
    }
}

/// What the CLI asks through the local API.
pub fn call(app: &AppHandle, call: ReviewCall) -> Result<Value> {
    Ok(match call {
        ReviewCall::Start { request } => {
            serde_json::to_value(visual(app).start(serde_json::from_value(request)?)?)?
        }
        ReviewCall::Decide {
            review,
            screen,
            decision,
            comment,
            pins,
        } => {
            let pins = pins.map(serde_json::from_value).transpose()?;
            serde_json::to_value(decide(app, review, &screen, &decision, comment, pins)?)?
        }
        ReviewCall::PrComment { review, confirm } => {
            serde_json::to_value(pr_comment(app, review, confirm.as_deref())?)?
        }
    })
}

// Git, the database and the image files: off the UI thread.

#[tauri::command]
pub async fn visual_start(app: AppHandle, request: Request) -> CmdResult<Review> {
    blocking(app, move |app| visual(app).start(request)).await
}

#[tauri::command]
pub async fn visual_reviews(app: AppHandle) -> CmdResult<Vec<Review>> {
    blocking(app, |app| visual(app).reviews()).await
}

#[tauri::command]
pub async fn visual_review(app: AppHandle, id: i64) -> CmdResult<Review> {
    blocking(app, move |app| visual(app).review(id)).await
}

#[tauri::command]
pub async fn visual_decide(
    app: AppHandle,
    id: i64,
    screen: String,
    decision: String,
    comment: Option<String>,
    pins: Option<Vec<Pin>>,
) -> CmdResult<Review> {
    blocking(app, move |app| {
        decide(app, id, &screen, &decision, comment, pins)
    })
    .await
}

/// The PNG bytes of a screen's `before`, `after` or `heatmap` image.
#[tauri::command]
pub async fn visual_image(
    app: AppHandle,
    id: i64,
    version: u32,
    screen: String,
    kind: String,
) -> CmdResult<tauri::ipc::Response> {
    blocking(app, move |app| {
        let png = visual(app).image(id, version, &screen, &kind)?;
        Ok(tauri::ipc::Response::new(png))
    })
    .await
}

/// The files a version's capture changed since its base, with their diff.
#[tauri::command]
pub async fn visual_files(app: AppHandle, id: i64, version: u32) -> CmdResult<Vec<FileDiff>> {
    blocking(app, move |app| visual(app).files(id, version)).await
}

/// Without `confirm`, what posting the images on the pull request would send; with the preview's fingerprint,
/// posts it.
#[tauri::command]
pub async fn visual_pr_comment(
    app: AppHandle,
    id: i64,
    confirm: Option<String>,
) -> CmdResult<PrPreview> {
    blocking(app, move |app| pr_comment(app, id, confirm.as_deref())).await
}
