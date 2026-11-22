---
description: On React 19.3 with server rendering, a browser-only widget uses use(browser()) under Suspense instead of a mount-flag Effect.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
We server-render our React app (react and react-dom 19.3.1, React Router in framework mode). `MapPanel.tsx` builds a map with `window.L` at render time and the server render crashes with "window is not defined".

What's the right way in our React version to render it only in the browser, with a placeholder on the server? Show the updated `MapPanel.tsx` and where it is used.
