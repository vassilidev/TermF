---
name: angular
description: >-
  Writes and changes Angular code the way the installed version expects, starting from the best-practices
  guide that ships inside @angular/core and the Angular CLI MCP server: components, signals, inputs and
  models, templates with @if/@for, services and inject(), Signal Forms, routing, HttpClient, tests with
  ng test. Use when a task touches an Angular workspace (angular.json, src/app). Not for AngularJS (1.x),
  NestJS, React, Vue or Svelte code.
paths: "**/angular.json, **/*.component.ts, **/*.component.html"
license: MIT
metadata:
  termf-stacks: "angular"
  termf-roles: "front qa"
  termf-versions: ">=19"
  termf-verified: "2026-11-22 @angular/core@22.3.0 @angular/cli@22.3.0"
  termf-sources: "https://angular.dev/ai/develop-with-ai https://angular.dev/assets/context/best-practices.md https://angular.dev/ai/mcp https://angular.dev/ai/agent-skills https://angular.dev/guide/testing https://github.com/angular/angular/blob/main/CHANGELOG.md https://github.com/angular/angular-cli/blob/main/CHANGELOG.md"
---
# Angular

1. Read the installed version of `@angular/core` in the lockfile (or `package.json`, or `ng version`), and keep it as the reference for every later step. Done when you can name the version and the file it came from.
2. Open the official guidance of that version before writing code: `node_modules/@angular/core/resources/best-practices.md` (shipped in `@angular/core` since 21.0.0, same text as https://angular.dev/assets/context/best-practices.md), or the Angular CLI MCP tool `get_best_practices` when the session has it, and `search_documentation` for each API you are unsure of. Its rules carry the major they start from ("v20+", "v22+"): skip those above the project's major. Done when you have read it in full.
3. Use the tools the project already has (package manager, the builders and test runner in `angular.json`, its lint target, its UI kit and state library), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. Read the bootstrap providers in `src/main.ts` or `app.config.ts` (zoneless or `provideZoneChangeDetection()`, router, HttpClient features) and one component and one service near the task. Done when you can name the file you are modelling on.
5. Generate files with the CLI when the project does (`ng generate component <path/name>`, `ng generate service <path/name>`), so naming and style follow the workspace settings, then write the change. Done when the generated files match their neighbours and the template, class and test compile.
6. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- In v22, a component without `changeDetection` is OnPush: a field changed in a `setTimeout`, a manual `subscribe` or a callback no longer re-renders, so hold that state in a signal (or use the `async` pipe). To keep an old component's behaviour, set `changeDetection: ChangeDetectionStrategy.Eager`, which the v22 migration adds where needed.
- Since v21, new apps are zoneless: no `zone.js` in the polyfills and no `provideZoneChangeDetection()`. An app that still relies on Zone declares `provideZoneChangeDetection()` in its bootstrap providers; check which one you are in before relying on automatic change detection.
- v22 accepts only TypeScript 6.0 (`@angular/compiler-cli` peer `>=6.0 <6.1`): leave `typescript` there, even when other packages of a monorepo moved to TypeScript 7.
- v22 HttpClient uses fetch: `withFetch()` is deprecated and can go, and upload progress events need `provideHttpClient(withXhr())`.
- v22 router: `paramsInheritanceStrategy` defaults to `'always'`, so child routes see their parents' params; `provideRoutes()` is removed, use `provideRouter()`.
- v22 removed `ComponentFactoryResolver` and `ComponentFactory` (pass the component class to `ViewContainerRef.createComponent`) and `ChangeDetectorRef.checkNoChanges` (tests call `fixture.detectChanges()`).
- New projects run Vitest through `ng test` since v21; Karma projects stay on Karma until migrated with `ng g @schematics/angular:refactor-jasmine-vitest`. Write tests for the runner the workspace has.
- Since v20 the CLI generates 2025-style names (`user-profile.ts`, class `UserProfile`); older workspaces use `user-profile.component.ts`. Match the files around you.

## Verify

- The project's scripts from `package.json` first.
- Else `ng build`, `ng test --no-watch` (Vitest or Karma, whichever the workspace uses), and `ng lint` when `angular.json` has a `lint` target.
- With the CLI MCP server, `run_target` runs the same targets.

## Security

- Angular escapes interpolation and sanitizes `[innerHTML]`, `[src]` and `[href]`; `DomSanitizer.bypassSecurityTrust*` turns that off, so use it only on content the code itself built, never on user or API data.
- Writing to `ElementRef.nativeElement` (`innerHTML`, `insertAdjacentHTML`) or to `document` directly skips sanitization: go through template bindings or `Renderer2`.
- Route guards (`canActivate`, `canMatch`) only hide screens: the API checks permissions on every request.
- HttpClient's XSRF protection reads the `XSRF-TOKEN` cookie for same-origin mutating requests: keep it on, and align cookie and header names with the server.
- Anything in `environment.ts` or under `src/` ends up in the browser bundle: no secrets there.

## References

- Develop with AI (rules files, best-practices.md, llms-full.txt): https://angular.dev/ai/develop-with-ai. `ng new --ai-config=claude-code` or `ng generate ai-config` writes a `CLAUDE.md` and an `.mcp.json` for the CLI MCP server.
- CLI MCP server (`npx -y @angular/cli mcp`, options `--read-only`, `--local-only`): https://angular.dev/ai/mcp
- Official agent skills `angular-developer` and `angular-new-app`: `npx skills add https://github.com/angular/skills` (no license file in that repository: the user imports them, TermF does not copy them). https://angular.dev/ai/agent-skills
- Breaking changes per major: https://github.com/angular/angular/blob/main/CHANGELOG.md; upgrades: `ng update` and https://angular.dev/update-guide
