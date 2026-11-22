---
title: Visual review
order: 3
---

The visual review captures each changed screen before and after a run, so you approve what the interface looks like, not only the diff.

## How it works

TermF's capture engine starts the project's dev server for the run and for its base, captures each changed screen with the system's Chrome, and compares the pixels and the texts. The review page then puts each screen in front of you: side by side, with a slider, as an overlay or as a heatmap, opened on what changed.

![The visual review page: the before and after slider on a changed screen, a pin, the comment for the agent and the decision buttons](shot:review "The review page, in slider mode. Sample data.")

## Turn it on

The review is a project rule, `visual-review`, off until you switch it on. With the rule on, a run whose interface changed stops once more before the delivery (see [the visual review step](runs.md#visual)).

## Review a run

1. When the run stops, its card opens the review page.
2. Each screen (route, device, theme) opens on its changed areas. Switch between side by side, slider, overlay and heatmap, zoomed and panned together.
3. Pin a comment on the image. Read why the screen changed and which texts changed, with their reason.
4. Approve, ask for changes or refuse each screen. With several screens, the swipe mode goes through them one card at a time, from the keyboard or the trackpad.

Each decision is saved at once; earlier versions stay readable.

## Settings

The `[visual]` table of the project sets the routes, the viewports (desktop, tablet, mobile), the themes, the threshold, the login and whether images go on the pull request. By default, only desktop in the light theme is captured.

```sh
# reviews from a terminal
termf review start | show | decide | pr
```

> [!NOTE]
> Decisions are taken screen by screen: there is no approve-all button. Chrome on Linux and Windows has not been tried; the page was checked with a simulated backend.
>
> **Tried so far.** The engine ran with a real Chrome on macOS through automated tests.
