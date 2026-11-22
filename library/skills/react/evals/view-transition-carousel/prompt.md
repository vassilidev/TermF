---
description: On React 19.3 a slide animation uses the stable ViewTransition from react, triggered from a Transition.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Our app runs React 19.3.1 with Vite. In `Carousel.tsx`, clicking "Next" calls `setIndex(i => i + 1)` and the slide just swaps. I want the new slide to slide in from the right with the browser's view transitions, using React's own support for it, not a library. Show the component.
