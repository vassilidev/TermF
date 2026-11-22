# React Compiler, Effects and transitions

Read when you write or fix an Effect, touch memoization, or add a transition or animation. Verified against react.dev and `eslint-plugin-react-hooks@7.1.1` on 2026-11-22 (React 19.3, React Compiler 1.0).

## Is the compiler on?
Look for one of:
- `babel-plugin-react-compiler` in `devDependencies`;
- `reactCompiler: true` in `next.config.*` (stable option since Next.js 16, off by default);
- `reactCompilerPreset()` from `@vitejs/plugin-react` (6.0+) passed to `@rolldown/plugin-babel` in `vite.config.*`, or `babel-plugin-react-compiler` listed first in a Babel config.

When it is on:
- New code gets no `useMemo`, `useCallback` or `React.memo` added for speed; the compiler memoizes. Keep a manual one only when a value's identity is part of the behaviour (an Effect dependency that must not change).
- Existing manual memoization stays in code outside the task: removing it is a refactor of its own, and the `preserve-manual-memoization` rule checks the compiler can keep it.
- `'use no memo'` at the top of a component skips it. It is a temporary escape for a compiler bug: fix the cause when you can and report any directive you add.
- Turning the compiler on in a project is its own task, never a side effect of another one.

## Rules the compiler and the lint enforce
`eslint-plugin-react-hooks` 7 (flat config `reactHooks.configs.flat.recommended`) adds the compiler rules to the hooks rules. The ones agents trip on most:
- `purity`: rendering is pure; no `Date.now()`, `Math.random()` or I/O in the render body. Read them in an event handler or an Effect, or pass them in.
- `refs`: no reading or writing `ref.current` during render.
- `set-state-in-render` and `set-state-in-effect`: no `setState` during render, and no Effect whose only job is to copy props into state. Compute the value during render, or reset with a `key`.
- `immutability`: never mutate props, state or values returned by hooks; build a new object.
- `static-components`: no component defined inside another component (it remounts on each render).
These rules hold whether or not the compiler runs; fix the code rather than adding a disable comment.

## Effects
- First ask whether an Effect is needed: derived data is computed during render; work caused by a user action goes in the event handler; data loading goes through the framework or the project's data library.
- An Effect synchronizes with something outside React and returns a cleanup that undoes it (unsubscribe, clear the timer, abort the request with an `AbortController`).
- Strict Mode runs setup, cleanup, setup in development, on client renders and, since 19.3, during hydration too. A missing cleanup shows up there first.
- Dependencies list every reactive value the Effect reads. For logic that must read the latest props without re-running the Effect, use `useEffectEvent` (19.2+):
```tsx
const onTick = useEffectEvent(() => refresh(filters));
useEffect(() => {
  const id = setInterval(() => onTick(), intervalMs);
  return () => clearInterval(id);
}, [intervalMs]); // filters are read through the Effect Event, so changing them does not restart the timer
```
  Call an Effect Event only from Effects (or other Effect Events) of the same component; never pass it down, never list it in dependencies, and never use it to hide a dependency that should re-run the Effect.

## Hidden UI that keeps its state
`<Activity mode={open ? 'visible' : 'hidden'}>` (19.2+) hides its children with `display: none`, keeps their state, and cleans up their Effects while hidden. Use it for tabs and panels the user comes back to. A hidden Activity whose child renders only text renders nothing.

## View Transitions (stable in 19.3)
```tsx
import { ViewTransition, startTransition, addTransitionType } from 'react';

<ViewTransition enter={{ forward: 'slide-in-end', back: 'slide-in-start' }} exit={{ forward: 'slide-out-start', back: 'slide-out-end' }}>
  <Slide key={index} />
</ViewTransition>

startTransition(() => {
  addTransitionType('forward');
  setIndex((i) => i + 1);
});
```
- Only Transition updates animate (`startTransition`, a `<Suspense>` reveal, `useDeferredValue`); a plain `setState` changes the DOM at once.
- Triggers: `enter` and `exit` for the first boundary inserted or removed, `update` for DOM changes inside, `share` for two boundaries with the same `name` (one removed, one inserted). Set `name` only for that shared-element case.
- Classes such as `'slide-in-end'` are CSS classes the project defines for the `::view-transition-*` pseudo-elements; transition types are also exposed to CSS through `:active-view-transition-type(...)`.
- Wrapping a `<Suspense>` boundary animates the fallback-to-content reveal; use `<ViewTransition update="auto" default="none">` so cached content does not animate.
- Honour `prefers-reduced-motion` in those CSS classes.
- DOM only for now.

## Fragment refs (stable in 19.3)
`<Fragment ref={ref}>` gives a `FragmentInstance` acting on the children as a group, without a wrapper element: `addEventListener`, `focus` / `focusLast` / `blur`, `observeUsing(observer)` for an `IntersectionObserver` or `ResizeObserver`, `getClientRects`, `scrollIntoView`. Use it instead of adding a `div` only to hold a ref.
