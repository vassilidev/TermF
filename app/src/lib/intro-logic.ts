// No imports, so `pnpm test` runs it in plain Node.

/** `other`: TermF already runs on another machine, whose sync this one joins. */
export type Machine = "first" | "other";
export type SetupStep = "env" | "vault" | "sync";
export type IntroStep = "welcome" | "claude" | SetupStep | "project" | "cto" | "help";
export type Done = Record<SetupStep, boolean>;

const SETUP: Record<Machine, SetupStep[]> = {
  first: ["env", "vault", "sync"],
  // The sync first: it brings the settings, the projects and the vault from the other machine.
  other: ["sync", "env", "vault"],
};

export const isSetup = (step: string): step is SetupStep => step === "env" || step === "vault" || step === "sync";

/**
 * The introduction's steps. What is already set up when it opens is left out; `only`: the steps put off, resumed
 * on their own.
 */
export function introSteps(machine: Machine | null, done: Done, only: SetupStep[] | null = null): IntroStep[] {
  const setup = SETUP[machine ?? "first"].filter((s) => !done[s] && (!only || only.includes(s)));
  return only ? setup : ["welcome", "claude", ...setup, "project", "cto", "help"];
}

/**
 * The step Continue leads to, null past the last one. A vault or a sync set up meanwhile is passed over (the sync
 * brought the vault); the environment never is, its detection tells what this machine has.
 */
export function nextStep(steps: IntroStep[], from: IntroStep, done: Done): IntroStep | null {
  return steps.slice(steps.indexOf(from) + 1).find((s) => !(s === "vault" || s === "sync") || !done[s]) ?? null;
}

/** What stays put off once the introduction closes: its setup steps not done, and those put off before it. */
export const putOff = (before: SetupStep[], steps: IntroStep[], done: Done): SetupStep[] =>
  SETUP.first.filter((s) => (before.includes(s) || steps.includes(s)) && !done[s]);

/** The steps put off and still to do, in the order this machine takes them. */
export const pending = (machine: Machine | null, passed: SetupStep[], done: Done): SetupStep[] =>
  SETUP[machine ?? "first"].filter((s) => passed.includes(s) && !done[s]);

/** The step once the project assistant it opened closes: the CTO's when a project was created meanwhile. */
export const introResume = (step: IntroStep, projectsBefore: number, projectsNow: number): IntroStep =>
  step === "project" && projectsNow > projectsBefore ? "cto" : step;

/** The machine and the steps put off, as kept on this machine; anything unreadable reads as nothing. */
export function readSetup(text: string | null): { machine: Machine | null; passed: SetupStep[] } {
  try {
    const kept = JSON.parse(text ?? "{}");
    return {
      machine: kept.machine === "first" || kept.machine === "other" ? kept.machine : null,
      passed: Array.isArray(kept.passed) ? SETUP.first.filter((s) => kept.passed.includes(s)) : [],
    };
  } catch {
    return { machine: null, passed: [] };
  }
}
