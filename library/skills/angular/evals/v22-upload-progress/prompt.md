---
description: In Angular 22 HttpClient uses fetch; upload progress events need provideHttpClient(withXhr()).
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Angular 22.3 app. `app.config.ts` has `provideHttpClient(withFetch())`. My upload service posts a file with `this.http.post(url, form, { reportProgress: true, observe: 'events' })` and filters `HttpEventType.UploadProgress`, but no upload progress event ever arrives, only the final response. What do I change?
