# Server Components and Server Functions

Read when the project uses `'use client'` or `'use server'`. Verified against react.dev on 2026-11-22 (React 19.3). Framework rules (Next.js caching, routing) are in that framework's docs.

## The boundary
- A file starting with `'use client'` (first statement, single or double quotes, comments allowed above) is a boundary: it and everything it imports run in the browser too. A module imported from a client module needs no directive of its own.
- Server Components have no state, no Effects and no event handlers; they can be `async` and read data directly.
- A Client Component cannot import a Server Component, but it can receive one as `children` or as any JSX prop: compose on the server side, `<ClientShell><ServerList /></ClientShell>`.
- Keep `'use client'` on the leaf that needs interactivity (the button, the form, the chart), so the surrounding layout and data stay on the server and out of the bundle.

## Props from server to client
Allowed: primitives (symbols only through `Symbol.for`), arrays, `Map`, `Set`, typed arrays, `Date`, plain objects of those, JSX elements, Promises, and Server Functions.
Rejected: other functions (an `onClick` defined in a Server Component), class instances (an ORM model, a `Decimal`), objects with a null prototype.
Convert at the boundary: pass `user.name` and `user.id`, not the ORM `user`.

Data can be streamed: create the promise in the Server Component without awaiting it, pass it as a prop, and read it in the client with `use(promise)` under a `<Suspense>` boundary.

## Server Functions
- `'use server'` at the top of a file makes every export a Server Function (all exports async functions); at the top of an `async` function body it marks only that function.
- The client calls them over the network: arguments and return values follow the serializable list above, except that React elements are not accepted as arguments. `FormData` is accepted.
- Call them inside a Transition: `<form action>` and `formAction` do it for you; outside a form wrap the call in `startTransition`, and `await` it to read the result.
- Each one is a public endpoint: validate every argument, check the caller's permission inside the function, return the minimum.

## Forms
```tsx
// actions.ts
'use server';
export async function rename(prev: State, formData: FormData): Promise<State> {
  const name = String(formData.get('name') ?? '').trim();
  if (!name) return { error: 'empty' };
  // authorize, then write
  return { ok: true };
}

// RenameForm.tsx
'use client';
import { useActionState } from 'react';
import { rename } from './actions';

export function RenameForm() {
  const [state, action, pending] = useActionState(rename, { ok: false });
  return (
    <form action={action}>
      <input name="name" aria-describedby="rename-error" />
      <button disabled={pending}>Save</button>
      {state.error && <p id="rename-error" role="alert">{state.error}</p>}
    </form>
  );
}
```
- The action receives the previous state first, then the `FormData`.
- A third argument (`permalink`) lets the form work before hydration when the framework supports progressive enhancement.
- Since 19.3, React fires `onReset` when it resets a form after a Server Action, and `submit` events carry the `submitter`.
- `useFormStatus` reads the pending state of the parent form from a child component; `useOptimistic` shows the expected result while the action runs.

## Context
- Create a context in a `'use client'` module. Since 19.3 a Server Component can render it directly as a provider: `<ThemeContext value={theme}>{children}</ThemeContext>`. Before 19.3, wrap it in a small Client provider component.
- `useContext` and `use(Context)` only work in Client Components.

## Browser-only content
`use(browser('reason'))` (`browser` from `react-dom`, 19.3) inside a Client Component makes the server render the nearest `<Suspense>` fallback and the browser render the component after hydration. Without a `<Suspense>` boundary above it, the server render fails. It replaces mount-flag Effects, which cause a hydration flash.

## Keeping secrets on the server
- Import `server-only` (or the framework's equivalent) at the top of modules that hold credentials or the data layer: importing them from a client module then fails at build time.
- `experimental_taintObjectReference` and `experimental_taintUniqueValue` stop a value from reaching the client, but they are experimental and an extra layer, not a replacement for passing only the needed fields.
