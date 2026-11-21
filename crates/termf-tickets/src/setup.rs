//! Preparing a worktree again, or tearing its preparation down, from its path (`termf worktree setup`, the
//! "run the setup again" button). A run's worktree gets its run's id and the run's journal records it.

use std::path::Path;

use anyhow::Result;
use termf_worktree::{Report, Setup, Worktree};

use crate::Tickets;
use crate::store::{Event, Run, Store};

impl Tickets {
    pub fn setup(&self) -> Setup {
        let mut setup = Setup::new(self.config_dir.clone(), self.data_dir.clone());
        setup.env = self.env.clone();
        setup
    }

    /// The worktree that holds `path`, and the run it belongs to.
    fn locate(&self, store: &Store, path: &Path) -> Result<(Worktree, Option<Run>)> {
        let setup = self.setup();
        let mut wt = setup.locate(path, None)?;
        let run = store.runs()?.into_iter().find(|r| {
            r.worktree
                .as_deref()
                .is_some_and(|w| same(Path::new(w), &wt.path))
        });
        if let Some(run) = &run {
            wt.id = run.id.to_string();
        }
        Ok((wt, run))
    }

    /// Runs the project's recipe again in the worktree at `path`.
    pub fn prepare(&self, path: &Path) -> Result<(Option<Run>, Report)> {
        let store = Store::open(&self.data_dir)?;
        let (wt, run) = self.locate(&store, path)?;
        let report = self.setup().run(&wt);
        if let Some(run) = &run {
            let event = Event::Setup {
                steps: report.steps.clone(),
                saved: report.saved,
            };
            store.record(run.id, &event)?;
        }
        Ok((run, report))
    }

    /// Drops the worktree's database copy and gives its ports back.
    pub fn unprepare(&self, path: &Path) -> Result<(Option<Run>, Report)> {
        let store = Store::open(&self.data_dir)?;
        let (wt, run) = self.locate(&store, path)?;
        let report = self.setup().teardown(&wt.path, &wt.id);
        if let Some(run) = &run
            && !report.steps.is_empty()
        {
            let steps = report.steps.clone();
            store.record(run.id, &Event::Teardown { steps })?;
        }
        Ok((run, report))
    }
}

fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
