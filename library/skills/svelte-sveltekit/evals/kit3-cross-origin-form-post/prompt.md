---
description: In a SvelteKit 3 project, allows one trusted cross-origin form post through csrf.trustedOrigins in vite.config, without turning CSRF off.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our SvelteKit app (`@sveltejs/kit` 3.4, `svelte` 5.59, Vite 8, there is no svelte.config.js) receives a POST form submission from our payment provider at https://pay.example.com on `/checkout/callback` (a form action). SvelteKit answers 403 "Cross-site POST form submissions are forbidden". How do I accept that provider's post and keep protection for everything else? Show the exact config change.
