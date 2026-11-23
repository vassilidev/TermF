<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { untrack } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import FileCode from "@lucide/svelte/icons/file-code";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Lightbulb from "@lucide/svelte/icons/lightbulb";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import { initial } from "$lib/ProjectSwitcher.svelte";
  import RepoCard from "$lib/RepoCard.svelte";
  import RepoPicker from "$lib/RepoPicker.svelte";
  import SetupQuestions from "$lib/SetupQuestions.svelte";
  import SetupReady from "$lib/SetupReady.svelte";
  import SetupUnderstood from "$lib/SetupUnderstood.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StreamSteps from "$lib/ui/StreamSteps.svelte";
  import Wizard, { type WizardStep } from "$lib/ui/Wizard.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { lang, t } from "$lib/i18n";
  import { cleanName, nameFromIdea, parentOf, uniqueSlug } from "$lib/greenfield-logic";
  import { ensure } from "$lib/home.svelte";
  import { askAI } from "$lib/ask.svelte";
  import { analyze, cards, closeNewProject, creating, detect, draft, endNewProject, forget, found, keep, readCards, started } from "$lib/newproject.svelte";
  import { analyzing, readText, reads } from "$lib/onboarding.svelte";
  import { ownTeam, type Analysis } from "$lib/onboarding-logic";
  import { editProject } from "$lib/project.svelte";
  import { derive, detectedAll, tagText, unique, type Card } from "$lib/setup-logic";
  import { app, load, setProject, shortPath, ui } from "$lib/state.svelte";

  type StepId = "start" | "repos" | "understood" | "questions" | "ready" | "idea" | "review";
  type Key = Parameters<typeof t>[0];
  type Plan = { path: string; exists: boolean; files: { name: string; text: string }[]; commands: string[]; gh: boolean };
  type Created = { project: string; path: string; remote: string | null; pushed: boolean; githubError: string | null };

  const labels: Record<StepId, Key> = {
    start: "cpStepStart",
    repos: "cpStepRepos",
    understood: "cpStepUnderstood",
    questions: "obQuestionsTitle",
    ready: "cpStepReady",
    idea: "greenfieldStepIdea",
    review: "cpStepReview",
  };
  const examples: [Key, Key][] = [
    ["greenfieldExample1Label", "greenfieldExample1"],
    ["greenfieldExample2Label", "greenfieldExample2"],
    ["greenfieldExample3Label", "greenfieldExample3"],
  ];
  const kickoff: Key[] = ["greenfieldKickoffFrame", "greenfieldKickoffResearch", "greenfieldKickoffDecide", "greenfieldKickoffBuild"];

  let busy = $state(false);
  let ready = $state(false);
  let error = $state("");
  let readyStep = $state<SetupReady>();
  let plan = $state<Plan | null>(null);
  let planError = $state("");
  /** The spec the plan or its error is for: creating waits until it is the one shown. */
  let plannedFor = $state("");

  const existing = $derived(creating.project ? (app.workspace?.projects.find((p) => p.id === creating.project) ?? null) : null);
  const ids = $derived<StepId[]>(
    existing ? ["repos", "understood", "questions", "ready"] : draft.mode === "scratch" ? ["start", "idea", "review"] : ["start", "repos", "understood", "questions", "ready"],
  );
  const at = $derived(Math.min(draft.step, ids.length - 1));
  const here = $derived(ids[at]);
  const home = $derived(app.workspace?.home ?? "");
  const projects = $derived(app.workspace?.projects ?? []);
  const taken = $derived(projects.filter((p) => p.id !== existing?.id).map((p) => p.name));
  const chosenCards = $derived(
    draft.chosen.map((path): Card => {
      const known = [...(app.workspace?.repos ?? []), ...projects.flatMap((p) => p.repos)].find((r) => r.path === path);
      return { ...(known ?? { name: path.split(/[\\/]/).pop() ?? path, path, modified: null }), ...cards[path] };
    }),
  );
  const selection = $derived(draft.chosen.join("\n"));
  const guess = $derived(derive(chosenCards, home));
  const scratch = $derived(draft.mode === "scratch" && !existing);
  // From scratch the name is also a folder's: `name-2` rather than `name 2`.
  const dedupe = (n: string) => (scratch ? uniqueSlug(n, taken) : unique(n, taken));
  const suggested = $derived(scratch ? dedupe(nameFromIdea(draft.idea)) : guess ? unique(guess.name, taken) : "");
  const name = $derived(existing?.name ?? ((draft.name ?? "").trim() || suggested));
  const clash = $derived(!existing && draft.name !== null && !!draft.name.trim() && dedupe(draft.name.trim()) !== draft.name.trim());
  const where = $derived(draft.parent ?? parentOf(app.workspace?.repos ?? [], home));
  const spec = $derived({ name, parent: where, idea: draft.idea.trim(), github: draft.github });
  const scratchReady = $derived(!!name && !!draft.idea.trim() && !!plan && !planError && plannedFor === JSON.stringify(spec) && (!draft.github || plan.gh));
  const analysisKey = $derived(existing?.id ?? "");
  const understood = $derived(draft.analysis !== null && draft.analyzed === selection);
  const questions = $derived(understood ? draft.analysis!.questions.filter((q) => !ownTeam(q)) : []);
  const waiting = $derived(!!analyzing[analysisKey] && !understood);
  const detected = $derived(detectedAll(draft.chosen, found));

  // Each repository is read as soon as it is chosen: what it holds is there when the owner looks.
  $effect(() => detect(draft.chosen));
  $effect(() => {
    readCards(draft.chosen).catch(() => {});
  });
  // Kept at every change: closing the window, or the app, loses nothing.
  $effect(() => {
    void JSON.stringify(draft);
    keep();
  });
  // Every repository removed: back to picking them.
  $effect(() => {
    if (!draft.chosen.length && !scratch && ids.indexOf(here) > ids.indexOf("repos")) draft.step = ids.indexOf("repos");
  });
  // An existing project's analysis is reused, with its answers: running it again is a choice.
  let looked = $state(false);
  $effect(() => {
    const id = existing?.id;
    if (!id || draft.analysis) return void (looked = true);
    invoke<{ analysis: Analysis | null; answers: Record<string, string[]> }>("onboarding_state", { project: id })
      .then((s) => {
        if (!s.analysis || draft.analysis || creating.project !== id) return;
        const same = s.analysis.repos.map((r) => r.path).join("\n") === draft.chosen.join("\n");
        if (same) Object.assign(draft, { analysis: s.analysis, analyzed: selection, answers: { ...s.answers }, failed: "" });
      }, () => {})
      .finally(() => (looked = true));
  });
  // Claude Code reads what was chosen from the moment it is understood, on whatever step the setup resumes; another
  // choice since is read again.
  $effect(() => {
    const after = ["understood", "questions", "ready"].includes(here);
    if (after && looked && !understood && !analyzing[analysisKey] && draft.chosen.length)
      untrack(() => analyze(existing?.id ?? null, [...draft.chosen], lang).catch(() => {}));
  });

  // What creating from the idea would do, read again a moment after the owner stops typing.
  $effect(() => {
    const asked = JSON.stringify(spec);
    plan = null;
    planError = "";
    plannedFor = "";
    if (!scratch || !spec.name || !spec.idea) return;
    const timer = setTimeout(() => {
      invoke<Plan>("greenfield_plan", { spec: JSON.parse(asked) }).then(
        (p) => {
          if (asked === JSON.stringify(spec)) ((plan = p), (planError = ""), (plannedFor = asked));
        },
        (e) => {
          if (asked === JSON.stringify(spec)) ((plan = null), (planError = String(e)), (plannedFor = asked));
        },
      );
    }, 250);
    return () => clearTimeout(timer);
  });

  function go(i: number) {
    draft.step = Math.max(0, Math.min(ids.length - 1, i));
    draft.reached = Math.max(draft.reached, draft.step);
    error = "";
  }

  const canContinue = $derived.by(() => {
    switch (here) {
      case "repos":
        return !!draft.chosen.length && !!name;
      case "understood":
        return detected;
      case "ready":
        return ready;
      case "idea":
        return !!draft.idea.trim() && !!name;
      case "review":
        return scratchReady;
      default:
        return true;
    }
  });

  function next() {
    if (!canContinue || busy) return;
    if (here === "questions" && draft.question < questions.length - 1) draft.question++;
    else if (here === "ready") readyStep?.create();
    else if (here === "review") createScratch();
    else go(at + 1);
  }

  function back() {
    if (here === "questions" && draft.question > 0) draft.question--;
    else go(at - 1);
  }

  function skip() {
    const q = questions[draft.question];
    if (q) draft.answers[q.id] = [...q.default];
    next();
  }

  function restart() {
    const before = $state.snapshot(draft);
    forget();
    toast(t("cpRestarted"), { undo: () => Object.assign(draft, before) });
  }

  async function pickParent() {
    const path = await open({ directory: true, multiple: false, defaultPath: where, title: t("greenfieldPickFolder") });
    if (typeof path === "string") draft.parent = path;
  }

  /** Creates the repository and the project, then opens the Home on the CTO, who gets the idea. */
  async function createScratch() {
    if (busy || !scratchReady) return;
    busy = true;
    error = "";
    const asked = { ...spec };
    let made: Created;
    try {
      made = await invoke<Created>("greenfield_create", { spec: asked });
    } catch (e) {
      error = String(e);
      busy = false;
      return;
    }
    const project = made.project;
    // From here the project exists: a step that fails is reported, never a reason to create it again.
    const failed: string[] = !made.githubError
      ? []
      : made.remote
        ? [t("greenfieldNotPushed", { url: made.remote, error: made.githubError, path: made.path })]
        : [t("greenfieldNoGithub", { error: made.githubError })];
    await load();
    try {
      // There is nothing to read in an empty repository yet: the setup waits for the skeleton.
      await invoke("onboarding_seen", { project });
      await invoke("onboarding_step", { project, step: "done" });
    } catch (e) {
      failed.push(String(e));
    }
    setProject(project);
    endNewProject();
    ui.view = "home";
    busy = false;
    toast(t("cpReady", { name: asked.name }), { kind: "ok" });
    const cto = await ensure(project, "cto");
    if (cto) {
      const text = t("greenfieldSeed", { name: asked.name, path: made.path, idea: asked.idea });
      await invoke("agent_send", { id: cto.id, text }).catch((e) => failed.push(String(e)));
      // Its answer comes in the ⌘I panel.
      void askAI({ role: "cto" });
    }
    if (failed.length) app.error = t("setupPartial", { error: failed.join(" · ") });
  }

  function onkeydown(e: KeyboardEvent) {
    // Enter already taken by the wizard (⌘Enter included) moves one screen, not two.
    if (ui.view !== "newProject" || ui.palette || ui.help || e.isComposing || e.defaultPrevented) return;
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      next();
    } else if (e.key === "Escape" && !e.defaultPrevented && !busy) {
      const target = e.target as HTMLInputElement | null;
      if (target?.closest?.(".xterm") || (target?.matches?.("input, textarea") && target.value)) return;
      e.preventDefault();
      closeNewProject();
    }
  }

  const nameSource = $derived.by(() => {
    if (draft.name !== null && draft.name.trim()) return clash ? t("setupNameTaken", { name: draft.name.trim() }) : t("setupNameTyped");
    if (scratch) return suggested ? t("greenfieldNameFromIdea") : t("greenfieldNameEmpty");
    if (!guess) return t("setupNameEmpty");
    const why = {
      repo: t("setupNameFromRepo"),
      prefix: t("cpNameFromPrefix"),
      owner: t("setupNameFromOwner", { owner: guess.name }),
      folder: t("setupNameFromFolder", { folder: guess.name }),
      first: t("setupNameFromFirst"),
    }[guess.from];
    return suggested !== guess.name ? `${why} ${t("setupNameNumbered", { name: guess.name })}` : why;
  });

  const list = (names: string[]) => (names.length > 4 ? `${names.slice(0, 4).join(", ")} +${names.length - 4}` : names.join(", "));
  const understoodAnswer = $derived.by(() => {
    const done = draft.chosen.map((p) => found[p]).filter((d) => !!d && typeof d !== "string");
    const tags = [...new Set(done.flatMap((d) => (typeof d === "string" ? [] : d.tags.slice(0, 3).map((x) => tagText({ name: x.name })))))];
    const tests = done.flatMap((d) => (typeof d === "string" ? [] : d.scripts.filter((s) => s.kind === "test").map((s) => s.run)));
    const said =
      draft.chosen.length === 1
        ? tests.length
          ? t("cpSumTests", { command: tests[0] })
          : t("cpSumNoTests")
        : `${t("cpTestsIn", { n: tests.length, total: draft.chosen.length })}.`;
    return [tags.slice(0, 4).join(", "), said].filter(Boolean).join(". ");
  });
  const lastRead = $derived(analyzing[analysisKey] ? reads[analysisKey]?.last.at(-1) : undefined);
  const folder = $derived.by(() => {
    const parents = new Set(draft.chosen.map((p) => p.replace(/[\\/][^\\/]*$/, "")));
    return parents.size === 1 ? shortPath([...parents][0]) : "";
  });

  const steps = $derived<WizardStep[]>(
    ids.map((id) => {
      const step: WizardStep = { id, label: t(labels[id]) };
      if (id === "start") step.answer = t(draft.mode === "scratch" ? "greenfieldFromScratch" : "greenfieldFromRepos");
      if (id === "repos") step.answer = list(chosenCards.map((c) => c.name));
      if (id === "understood") {
        step.answer = understoodAnswer;
        if (lastRead) step.live = readText(lastRead);
      }
      if (id === "questions") {
        step.answer = questions.length ? t("cpAnswered", { count: questions.length }) : t("cpNoQuestion");
        step.hint = here === "questions" && questions.length ? t("cpQuestionOf", { n: draft.question + 1, total: questions.length }) : undefined;
        if (!lastRead && waiting) step.live = t("cpPreparing");
      }
      if (id === "ready" || id === "review") step.hint = t("cpNothingBefore");
      if (id === "idea") step.answer = draft.idea.trim().length > 70 ? `${draft.idea.trim().slice(0, 70)}…` : draft.idea.trim();
      return step;
    }),
  );
  const left = $derived(Math.max(1, ids.length - 1 - at));
</script>

<svelte:window {onkeydown} />

{#snippet head()}
  <div class="identity">
    <span class="mark" class:empty={!name} aria-hidden="true">{name ? initial(name) : ""}</span>
    <div>
      <b>{name || t("newProject")}</b>
      <span>
        {#if scratch}{shortPath(where)}/{name || "…"}
        {:else if draft.chosen.length === 1}{folder ? t("cpRepoIn", { folder }) : t("cpRepo")}
        {:else if draft.chosen.length}{folder ? t("cpReposIn", { count: draft.chosen.length, folder }) : t("cpRepos", { count: draft.chosen.length })}
        {:else}{t("setupNoneChosen")}{/if}
      </span>
    </div>
  </div>
{/snippet}

{#snippet foot()}
  {t("cpLeft", { minutes: left })}
{/snippet}

{#snippet secondary()}
  {#if here === "questions" && questions.length}
    <button class="btn quiet" onclick={skip}>{t("cpSkipQuestion")}</button>
  {/if}
{/snippet}

{#snippet title(text: string, lead: string)}
  <p class="kicker">{t("uiF1StepOf", { n: at + 1, total: ids.length })}</p>
  <h2>{text}</h2>
  <p class="lead">{lead}</p>
{/snippet}

<div class="new-project">
  <header class="bar">
    <img src="/favicon.png" alt="" width="22" height="22" />
    <h1>{existing ? t("obPalette", { project: existing.name }) : t("newProject")}</h1>
    <span class="grow"></span>
    <span class="saved"><Check size={14} aria-hidden="true" />{t("cpSaved")}</span>
    {#if started() && !existing}
      <button class="btn quiet" onclick={restart} disabled={busy}>{t("cpRestart")}</button>
    {/if}
    <button class="btn" onclick={closeNewProject} disabled={busy}>{t("obLater")}<kbd>Esc</kbd></button>
  </header>

  <div class="body">
    <Wizard
      title={t("setupSteps")}
      {steps}
      current={at}
      reached={Math.min(draft.reached, ids.length - 1)}
      onstep={go}
      onback={back}
      oncontinue={next}
      canBack={at > 0 || (here === "questions" && draft.question > 0)}
      {canContinue}
      {busy}
      continueLabel={here === "ready" ? t(existing ? "cpApply" : "setupCreate") : here === "review" ? t("setupCreate") : undefined}
      {head}
      {foot}
      {secondary}
    >
      {#if error}<p class="notice error" role="alert">{error}</p>{/if}
      {#key here}
        <div class="step">
          {#if here === "start"}
            {@render title(t("cpStartTitle"), t("cpStartLead"))}
            <div class="modes" role="radiogroup" aria-label={t("greenfieldModes")}>
              {#each [["repos", FolderGit2, "greenfieldFromRepos", "greenfieldFromReposText"], ["scratch", Lightbulb, "greenfieldFromScratch", "greenfieldFromScratchText"]] as const as [id, Icon, label, text] (id)}
                <label class="mode" class:on={draft.mode === id}>
                  <input type="radio" name="mode" value={id} checked={draft.mode === id} onchange={() => (draft.mode = id)} {@attach (el) => { if (draft.mode === id) el.focus(); }} />
                  <span class="icon"><Icon size={20} aria-hidden="true" /></span>
                  <span class="mode-text">
                    <b>{t(label)}</b>
                    <span>{t(text)}</span>
                    {#if id === "repos" && app.workspace?.repos.length}<span class="proof">{t("cpReposFound", { count: app.workspace.repos.length })}</span>{/if}
                  </span>
                </label>
              {/each}
            </div>
          {:else if here === "repos" && existing}
            {@render title(t("cpStepRepos"), t("cpReposExisting"))}
            <ul class="fixed">
              {#each chosenCards as card (card.path)}<li><RepoCard {card} /></li>{/each}
            </ul>
            <button class="btn" onclick={() => editProject(existing.id)}>{t("cpReposChange")}</button>
          {:else if here === "repos"}
            {@render title(t("cpStepRepos"), t("setupReposHelp"))}
            <div class="name">
              <label for="project-name">{t("projectName")}</label>
              <input
                id="project-name"
                class="field"
                value={draft.name ?? suggested}
                placeholder={suggested || t("projectName")}
                aria-describedby="project-name-help"
                aria-invalid={clash || undefined}
                spellcheck="false"
                autocomplete="off"
                oninput={(e) => (draft.name = e.currentTarget.value)}
              />
              <p id="project-name-help" class="help" class:warn={clash}>
                {nameSource}
                {#if clash}
                  <button class="btn link" onclick={() => (draft.name = dedupe(draft.name!.trim()))}>{t("setupNameUse", { name: dedupe(draft.name!.trim()) })}</button>
                {:else if draft.name !== null && draft.name.trim() && suggested}
                  <button class="btn link" onclick={() => (draft.name = null)}>{t("setupNameReset", { name: suggested })}</button>
                {/if}
              </p>
            </div>
            <RepoPicker bind:chosen={draft.chosen} autofocus />
          {:else if here === "understood"}
            {@render title(t("cpStepUnderstood"), t("cpUnderstoodLead"))}
            <SetupUnderstood project={existing?.id ?? null} chosen={chosenCards} />
          {:else if here === "questions"}
            {@render title(t("obQuestionsTitle"), understood ? (questions.length ? t("cpQuestionsLead", { count: questions.length }) : t("obNoQuestions")) : t("cpQuestionsWaiting"))}
            {#if understood && questions.length}
              <SetupQuestions {questions} analysis={draft.analysis!} />
            {:else if waiting}
              {@const live = reads[analysisKey]}
              <div class="waiting">
                <p><Sparkles size={15} aria-hidden="true" />{t("cpPreparing")}</p>
                {#if live?.last.length}
                  <StreamSteps live steps={live.last.map((r, i) => ({ verb: readText(r), state: i === live.last.length - 1 ? "now" : "ok" }))} />
                {:else}
                  <Skeleton rows={2} />
                {/if}
              </div>
            {:else if !understood && draft.failed}
              <p class="notice warn" role="status">{t("cpClaudeFailed", { error: draft.failed })}</p>
            {/if}
          {:else if here === "ready"}
            {@render title(t("cpStepReady"), t(existing ? "cpReadyLeadExisting" : "cpReadyLead"))}
            <SetupReady bind:this={readyStep} project={existing} chosen={chosenCards} {name} bind:busy bind:ready />
          {:else if here === "idea"}
            {@render title(t("greenfieldIdeaTitle"), t("greenfieldIdeaHelp"))}
            <textarea
              class="field idea"
              rows="5"
              aria-label={t("greenfieldIdeaTitle")}
              placeholder={t("greenfieldIdeaPlaceholder")}
              bind:value={draft.idea}
              {@attach (el) => el.focus()}
            ></textarea>
            <div class="examples">
              <span class="muted">{t("greenfieldExamples")}</span>
              {#each examples as [label, text] (label)}
                <button class="btn sm" title={t(text)} onclick={() => (draft.idea = t(text))}>{t(label)}</button>
              {/each}
            </div>
            <div class="settings">
              <div class="name">
                <label for="project-name">{t("projectName")}</label>
                <input
                  id="project-name"
                  class="field"
                  value={draft.name ?? suggested}
                  placeholder={suggested || t("projectName")}
                  aria-describedby="project-name-help"
                  aria-invalid={clash || undefined}
                  spellcheck="false"
                  autocomplete="off"
                  oninput={(e) => {
                    const field = e.currentTarget;
                    // What cannot be in a folder name leaves the field too, so what is shown is what is used.
                    if (cleanName(field.value) !== field.value) field.value = cleanName(field.value);
                    draft.name = field.value;
                  }}
                />
                <p id="project-name-help" class="help" class:warn={clash}>{nameSource}</p>
              </div>
              <div class="line">
                <span class="key">{t("greenfieldWhere")}</span>
                <span class="mono" title={`${where}/${name}`}>{shortPath(where)}/<b>{name || "…"}</b></span>
                <button class="btn link" disabled={busy} onclick={pickParent}>{t("greenfieldChange")}</button>
              </div>
              <label class="line check">
                <input type="checkbox" checked={!!draft.github} onchange={(e) => (draft.github = e.currentTarget.checked ? "private" : null)} />
                <span>{t("greenfieldGithub")}</span>
              </label>
              {#if draft.github}
                <div class="line">
                  <div class="segmented" role="radiogroup" aria-label={t("greenfieldVisibility")}>
                    {#each [["private", "greenfieldPrivate"], ["public", "greenfieldPublic"]] as const as [v, label] (v)}
                      <button role="radio" aria-checked={draft.github === v} onclick={() => (draft.github = v)}>{t(label)}</button>
                    {/each}
                  </div>
                  <span class="muted">{t(draft.github === "private" ? "greenfieldPrivateHint" : "greenfieldPublicHint")}</span>
                </div>
              {/if}
            </div>
            <aside class="kickoff">
              <Sparkles size={16} aria-hidden="true" />
              <div>
                <b>{t("greenfieldTeamTitle")}</b>
                <ol>{#each kickoff as line (line)}<li>{t(line)}</li>{/each}</ol>
              </div>
            </aside>
          {:else if here === "review"}
            {@render title(t("setupCreateTitle", { name }), t("greenfieldReviewHelp"))}
            {#if planError}
              <p class="notice error" role="alert">{planError}</p>
            {:else if plan}
              <div class="recap">
                <p class="line"><FolderOpen size={15} aria-hidden="true" /><span class="mono" title={plan.path}>{shortPath(plan.path)}</span>{#if plan.exists}<span class="badge">{t("greenfieldFolderEmpty")}</span>{/if}</p>
                <details>
                  <summary><FileCode size={14} aria-hidden="true" />{t("greenfieldFiles")}</summary>
                  {#each plan.files as f (f.name)}
                    <figure><figcaption class="mono">{f.name}</figcaption><pre><code>{f.text}</code></pre></figure>
                  {/each}
                </details>
                <details>
                  <summary><SquareTerminal size={14} aria-hidden="true" />{t("greenfieldCommands")}</summary>
                  <pre><code>{plan.commands.map((c) => `$ ${c}`).join("\n")}</code></pre>
                </details>
                {#if draft.github && !plan.gh}
                  <p class="notice error" role="alert">{t("greenfieldNoGh")}</p>
                {:else if !draft.github}
                  <p class="muted">{t("greenfieldLocalOnly")}</p>
                {/if}
                <h3>{t("setupThen")}</h3>
                <ol class="then">
                  <li>{t("greenfieldThenProject")}</li>
                  <li>{t("greenfieldThenCto")}</li>
                  <li>{t("greenfieldThenYou")}</li>
                </ol>
              </div>
            {:else}
              <Skeleton rows={4} label={t("greenfieldReading")} />
            {/if}
          {/if}
        </div>
      {/key}
    </Wizard>
  </div>
</div>

<style>
  .new-project {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }

  .bar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 12px;
    height: 52px;
    padding: 0 16px 0 20px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }

  .bar img {
    border-radius: 6px;
  }

  .bar h1 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .grow {
    flex: 1;
  }

  .saved {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-right: 4px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .saved :global(svg) {
    color: var(--done);
  }

  .body {
    flex: 1;
    min-height: 0;
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
  }

  .identity div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .identity b {
    overflow: hidden;
    font-size: var(--text-md);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .identity div > span {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mark {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-md);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 700;
  }

  .mark.empty {
    border: 1.5px dashed var(--border-strong);
    background: none;
  }

  .step {
    animation: arrive var(--dur) var(--ease-out);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .step {
      animation: none;
    }
  }

  .kicker {
    margin: 0;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h2 {
    margin: 6px 0 0;
    font-size: var(--text-2xl);
    font-weight: 600;
    letter-spacing: -0.02em;
    line-height: 1.2;
  }

  .lead {
    max-width: 62ch;
    margin: 8px 0 24px;
    color: var(--soft);
    font-size: var(--text-md);
    line-height: 1.5;
  }

  .modes {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .mode {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    min-height: 132px;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    cursor: pointer;
    transition:
      border-color var(--dur-fast) ease,
      background-color var(--dur-fast) ease;
  }

  .mode:hover {
    border-color: var(--border-strong);
  }

  .mode.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .mode:has(input:focus-visible) {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }

  .mode input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .mode .icon {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
  }

  .mode.on .icon {
    background: var(--accent);
    color: var(--accent-text);
  }

  .mode-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .mode-text b {
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .mode-text span {
    color: var(--soft);
    line-height: 1.45;
  }

  .mode-text .proof {
    margin-top: 6px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .name {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 20px;
  }

  .name label {
    font-weight: 500;
  }

  .name .field {
    max-width: 360px;
    height: var(--h-lg);
    font-size: var(--text-md);
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .help.warn {
    color: var(--warning);
  }

  .fixed {
    margin: 0 0 16px;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .fixed li + li {
    border-top: 1px solid var(--border);
  }

  .waiting {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .waiting p {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }

  .waiting p :global(svg) {
    color: var(--accent-ink);
  }

  .idea {
    width: 100%;
    height: auto;
    min-height: 120px;
    padding: 12px 14px;
    font-size: var(--text-md);
    line-height: 1.5;
    resize: vertical;
  }

  .examples {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }

  .settings {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 24px;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .settings .name {
    margin: 0;
  }

  .line {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 0;
  }

  .line .key {
    font-weight: 500;
  }

  .line.check {
    cursor: pointer;
  }

  .line.check input {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
  }

  .kickoff {
    display: flex;
    gap: 12px;
    margin-top: 16px;
    padding: 16px 18px;
    border-radius: var(--radius-lg);
    background: var(--well);
    color: var(--soft);
  }

  .kickoff :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--accent-ink);
  }

  .kickoff b {
    color: var(--text);
  }

  .kickoff ol,
  .then {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 8px 0 0;
    padding-left: 18px;
    line-height: 1.45;
  }

  .recap {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .recap h3 {
    margin: 8px 0 0;
    font-size: var(--text-md);
  }

  summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-ink);
    font-weight: 500;
    cursor: pointer;
  }

  figure {
    margin: 10px 0 0;
  }

  figcaption {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  pre {
    margin: 6px 0 0;
    padding: 12px 14px;
    overflow: auto;
    border-radius: var(--radius-md);
    background: var(--well);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    line-height: 1.5;
  }
</style>
