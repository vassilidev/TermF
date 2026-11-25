---
title: Laravel
order: 8
---

Give an SSH connection the folder of a Laravel site, and Ops reads the app for you: `php artisan about`, the scheduler, Horizon, the failed jobs, and `laravel.log` live.

![Ops on a Laravel server: the failed jobs of the site, each with its queue and date, and the buttons to retry or forget it](shot:laravel "The failed jobs of a Laravel site. Sample data.")

## Set it up {#setup}

In **Ops**, add or edit an SSH connection and fill **Site folder**: the application's folder on the server, such as `/home/forge/acme.com`, where its recipes run `artisan`. A site imported from [Forge](forge.md) gets it on its own when it is the only site imported on its server.

## Read the app {#read}

Pick the connection, then in the **Laravel** panel:

- **About**: what `php artisan about` says;
- **Scheduler**: the scheduled commands, from `schedule:list`;
- **Failed jobs**: each job with its class, queue and date;
- **Horizon**: its status.

## Follow laravel.log {#log}

**Live laravel.log** opens the **Live** tab on the site's `storage/logs/laravel.log`. An exception folds with its stack trace, so the log stays readable. See [Live logs](ops.md#live-logs).

## Retry or forget a job {#jobs}

**Retry** puts a failed job back on its queue. **Forget** erases it and its error for good. Each one shows a card that says what changes, then asks you to type the environment's name.

An agent can ask for the same through its own card, see [Agents that act for you](agent-actions.md).

> [!NOTE]
> Only `laravel.log` is followed, not the daily `laravel-YYYY-MM-DD.log` files.
>
> **Tried so far.** Never against a real Laravel site: built from known outputs and checked with automated tests and a simulated backend.
