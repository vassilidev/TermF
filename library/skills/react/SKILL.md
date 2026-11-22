---
name: react
description: >-
  Writes and changes React components the way the installed React and the project expect: reuse of
  the design system, Server and Client Components, Server Functions and forms with useActionState,
  Effects, React Compiler, View Transitions and the APIs added in React 19.2 and 19.3. Use when a
  task adds or changes .jsx or .tsx components, hooks or React state in a web app. Not for Next.js
  routing, caching or config, not for React Native, and not for styling alone.
paths: "**/*.jsx, **/*.tsx"
license: MIT
metadata:
  termf-stacks: "react"
  termf-roles: "front qa"
  termf-versions: ">=18"
  termf-verified: "2026-11-22 react@19.3.1"
  termf-sources: "https://react.dev/llms.txt https://react.dev/blog/2026/09/09/react-19-3 https://react.dev/blog/2025/10/01/react-19-2 https://react.dev/learn/react-compiler/installation https://react.dev/reference/rsc/use-server https://react.dev/blog/2025/12/03/critical-security-vulnerability-in-react-server-components"
---
# React

1. Read the installed version in the lockfile entries for `react` and `react-dom` (they must match), and keep it as the reference for every later step. In a Next.js App Router project the React that renders is the canary bundled in `next`. Note whether the React Compiler is on (`babel-plugin-react-compiler` in devDependencies, `reactCompiler: true` in `next.config`, `reactCompilerPreset` in `vite.config`) and whether the code uses Server Components (`'use client'`, `'use server'`). Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code: the `react` package ships none, so use `https://react.dev/llms.txt` and the page of each API with `.md` appended. Done when the page for the task is open.
3. Use the tools the project already has (package manager, linter, test runner, design system, state, form and data-fetching libraries), because a second tool splits the project. Look for the design system in a `components/ui/` folder, a workspace UI package, shadcn's `components.json` or Storybook stories. Done when every command you plan to run and every component you plan to use comes from the project, or the gap is written in your report.
4. In a Server Components project, place the client boundary: components stay Server Components by default, `'use client'` goes on the smallest leaf that needs state, Effects, event handlers or browser APIs, and the props it receives are serializable. Read `references/server-components.md` first. Done when no client module imports a server-only module.
5. Write the component. Compute derived values during render rather than syncing them with an Effect, and keep Effects for external systems, each with its cleanup. Forms use `<form action={fn}>` with `useActionState` for the result and the pending state, `useOptimistic` for optimistic UI; a mutation outside a form runs inside `startTransition`. List keys come from stable ids when items can move. Every interactive element has an accessible name, a visible `<label>` first. Done when it renders, type-checks and the hooks lint passes.
6. Test the behaviour with the project's runner, querying by role and label as a user would, from the acceptance criteria (skills `tests-from-criteria` and `testing-js`). Done when the new test fails without your change.
7. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- React 19.3 (September 2026) made `<ViewTransition>` (`import { ViewTransition } from 'react'`) and Fragment refs (`<Fragment ref={…}>`) stable; drop any `unstable_` import. Only updates inside a Transition (`startTransition`, a `<Suspense>` reveal, `useDeferredValue`) animate, a plain `setState` does not. Give `name` only to shared-element transitions.
- 19.3 double-invokes Effects in Strict Mode during hydration as well: an Effect without a correct cleanup now misbehaves in development on server-rendered pages too.
- 19.3: a Server Component can render a context imported from a `'use client'` module directly, `<ThemeContext value={theme}>`; the context itself is still created in a client module.
- 19.3: for UI that only works in the browser, call `use(browser())` (`browser` from `react-dom`) in a Client Component inside a `<Suspense>` boundary. The server renders the fallback; this replaces the `useEffect(() => setMounted(true))` workaround and its flash.
- 19.2: `useEffectEvent` holds logic an Effect calls but should not re-run for; call it only from Effects and never use it to drop a real dependency. `<Activity mode="hidden">` hides UI while keeping its state and cleaning up its Effects.
- React Compiler 1.0 is stable. When the project uses it, write new code without `useMemo`, `useCallback` or `memo` for performance, and leave existing ones in code the task does not touch. A `'use no memo'` directive is a temporary opt-out: mention any you add in your report.
- `@vitejs/plugin-react` 6 removed its inline `babel` option: the compiler is wired through `reactCompilerPreset()` with `@rolldown/plugin-babel`.
- `eslint-plugin-react-hooks` 7 puts the compiler rules (`purity`, `refs`, `set-state-in-effect`, `immutability`…) in `reactHooks.configs.flat.recommended`; `recommended-latest` is the experimental set. Fix what they report rather than disabling the rule on the line.
- Create React App is sunset (February 2025): React goes into a framework or into Vite, never through `create-react-app`.

## Verify

- The project's own scripts from `package.json`: `lint`, `typecheck`, `test`, `build`.
- `npx tsc --noEmit` and `npx eslint .` when the project has no such script.
- With the compiler on, React DevTools marks compiled components with a "Memo ✨" badge in development.

## Security

- `dangerouslySetInnerHTML` only receives HTML that went through the sanitizer the project already uses. If the project enforces Trusted Types (`require-trusted-types-for 'script'`), pass a `TrustedHTML`; since 19.3 React hands it to the DOM unchanged.
- React does not check URL schemes: a user-supplied `href` or `src` goes through an allowlist (`https:`, `http:`, `mailto:`) before rendering.
- Arguments of a Server Function (`'use server'`) are controlled by the client: validate them, check that the signed-in user may do the action inside the function, and return only what the UI needs.
- Everything passed as a prop to a Client Component reaches the browser: pass the fields the component shows, never a full database row or a secret.
- React Server Components had a critical remote code execution flaw (CVE-2025-55182, fixed in `react-server-dom-*` 19.0.1, 19.1.2 and 19.2.1), followed by denial-of-service and source-exposure fixes. When you touch the dependencies of an RSC app, keep `react-server-dom-*` or the framework on its latest patch.

## References

- Read `references/server-components.md` when the project uses Server Components or Server Functions (`'use client'`, `'use server'`, forms with actions).
- Read `references/compiler-and-effects.md` when you write or fix an Effect, touch memoization, or add a transition or animation.
- `https://react.dev/llms.txt`; release notes `https://react.dev/blog/2026/09/09/react-19-3` and `https://react.dev/blog/2025/10/01/react-19-2`.
- Performance reviews: Vercel's `react-best-practices` skill (`vercel-labs/agent-skills`, installed with `npx skills add vercel-labs/agent-skills`). Suggest it to the user; TermF does not ship it.
