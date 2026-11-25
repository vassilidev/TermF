---
title: CI/CD
order: 6
---

The CI/CD view shows a repository's GitHub Actions runs as a graph of their jobs, opens a failure on its first error, and keeps the deployments and webhooks of the project in the same place.

![CI/CD: the branches and pull requests, the latest runs of a branch, the graph of a failed run's jobs and the first error of its log, with Why?](shot:ci "A failed run, its jobs and its first error. Sample data.")

## Read the runs {#runs}

1. Open **CI/CD** in the project, and pick a branch or a pull request in **Branches and PRs**. Each shows its last ten runs as squares.
2. The latest runs of the branch line up at the top, oldest first, with how many failed in a row. Click one to open it.
3. The **Job graph** draws the jobs with their dependencies and their durations, live while the run goes on. Click a job to see its steps and its log.
4. **First error** opens on the lines around the first error of a failed job, with their numbers. **Full log** shows the rest; **Why?** asks the DevOps engineer, with the run and the line attached (see [Ask from any screen](ask-panel.md)).
5. **Rerun failed jobs**, **Rerun all**, cancel the run, or **Start a workflow** with its inputs.

A repository without a workflow offers **Set up CI with the team**, which hands the work to the DevOps engineer.

## Deployments and webhooks {#deploy}

- **Deployments** lists the deployments of the project's targets on Vercel, Netlify, Fly, Railway or Laravel Forge, through their own command-line tools, with their logs. Each deployment says which pull request and TermF run it comes from. Redeploy one after you approve it. Set the targets up in **Tools**; their tokens stay in the [vault](vault.md).
- **Webhooks** lists the repository's webhooks and their latest deliveries. Send a delivery again, add a webhook or delete one, each after you confirm. GitHub shows webhooks to the repository's admins only.

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
