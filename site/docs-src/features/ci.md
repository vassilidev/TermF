---
title: CI/CD
order: 6
---

The CI/CD view shows a repository's GitHub Actions runs and logs inside the app, and sends a failure back to the agent that caused it.

![CI/CD: the runs of a repository, and a failed job's log stopped on its first error](shot:ci "A failed run, its log opened on the first error. Sample data.")

## Read the runs

1. Open **CI/CD** in the project. Runs, jobs and steps come from the GitHub CLI, by repository, branch or pull request.
2. Open a job: its log is cleaned and opens on its errors.
3. **Run again** (every job, or the failed ones only), **Cancel**, or start a workflow with its inputs.

## A failure in a run

On a run's page, the pull request's checks show beside the work. **Send the failure to the agent** types a framed excerpt of the failed log into the agent's session.

The guard keeps a run's agents from starting or cancelling CI themselves (see [the run guard](security.md#guard)).

## Settings

Notifications cover the runs that concern you. `[ci] notify = "mine" | "all" | "off"` changes that.

```sh
# runs and logs
termf ci runs | view | logs --errors

# act on them
termf ci rerun [--failed] | cancel | dispatch
```

> [!NOTE]
> Needs the GitHub CLI signed in. Only GitHub Actions is read.
>
> **Tried so far.** Tried with the real GitHub CLI 2.97 on TermF's own repository: runs, filters, jobs, logs and their errors, offline and signed out; a failed run is driven in the real app on macOS by an end-to-end test.
