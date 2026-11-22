# Injection, output encoding and file handling

Read from the `security-web` skill. Covers A05:2025 Injection and ASVS 5.0 chapters V1 Encoding and Sanitization, V2 Validation and Business Logic, V3 Web Frontend Security, V4 API and Web Service, V5 File Handling. OWASP wording is not reproduced; open the linked chapter for a requirement's exact text.

## The rule

Validate input once at the boundary with a schema (type, length, format, allowed values), then treat it as untrusted at every sink anyway: validation narrows what arrives, encoding or parameterization at the sink is what makes it safe.

## SQL and NoSQL

- Bind every value as a parameter. Identifiers (column for `ORDER BY`, table names) come from an allowlist map, never from the request.
- ORM raw APIs are where injection comes back: Django `.raw()` and `.extra()`, SQLAlchemy `text()` built with f-strings, Prisma `$queryRawUnsafe`, Sequelize `literal`, Laravel `whereRaw` and `DB::raw`, Rails `find_by_sql` and string `where`, Go `fmt.Sprintf` into `db.Query`.
- MongoDB and other document stores: a JSON body can carry operators (`{"$ne": null}`); validate types before building a filter.

## Shell and processes

- Call programs with an argument array and no shell: `subprocess.run([...])` without `shell=True`, `execFile`/`spawn` instead of `exec`, `exec.Command(name, args...)`.
- Put `--` before arguments that come from users when the program accepts it, so a value starting with `-` is not read as an option.

## Templates and HTML

- Never render a template string supplied by a user (server-side template injection).
- Encode for the context: HTML text, attribute, URL, JavaScript and CSS each need their own encoding. Framework auto-escaping covers HTML text only.
- The escape hatches to search the diff for: `dangerouslySetInnerHTML`, `v-html`, `{@html}`, `bypassSecurityTrust*`, `|safe`, `mark_safe`, `html_safe`, `raw`, `{!! !!}`, `innerHTML`, `document.write`, `insertAdjacentHTML`.
- When HTML from users is a feature, sanitize it with a maintained sanitizer (DOMPurify in browsers, the framework's sanitizer on servers) and an allowlist of tags and attributes.
- Links built from input accept only `http:` and `https:` (and `mailto:` if needed); `javascript:` URLs run code.
- Content Security Policy as a second layer: nonce- or hash-based `script-src`, `object-src 'none'`, `base-uri 'none'`, `frame-ancestors` set; Trusted Types where the app supports them.

## Files and paths

- Never build a path from input. When you must, resolve it and check it stays under the intended root after resolution (symlinks included).
- Uploads: limit size, check the type from the content and not only the extension, store under a generated name outside the web root or in object storage, and serve with the stored `Content-Type`, `Content-Disposition: attachment` for anything not meant to render, and `X-Content-Type-Options: nosniff`.
- Archives: check each entry's path before extracting (zip slip) and cap the extracted size.

## Parsers and formats

- Untrusted data never goes to a native deserializer: Python `pickle` and `yaml.load` (use `yaml.safe_load`), Java native serialization, PHP `unserialize`, Ruby `Marshal.load`, .NET `BinaryFormatter`. Use JSON with a schema.
- XML: disable DTDs and external entities in the parser.
- Regular expressions run on input need bounded patterns or a timeout; nested quantifiers can take exponential time (ReDoS).
- JavaScript: merging untrusted objects into others can set `__proto__` or `constructor.prototype` (prototype pollution); copy only known keys, or use `Object.create(null)` maps.

## Redirects, headers and logs

- Redirect targets from input: relative paths only, or an allowlist of hosts.
- Strip CR and LF from anything written into a header or a log line.

## APIs

- GraphQL: limit depth, complexity and batch size; authorize in resolvers, not only at the endpoint.
- Answer with explicit response models so internal fields (password hashes, tokens, flags) never serialize by accident.

## Links

- A05:2025 and the other categories: https://top10.owasp.org/2025
- ASVS 5.0 chapters V1 to V5: https://github.com/OWASP/ASVS/tree/master/5.0/en
- OWASP Cheat Sheets (Injection Prevention, XSS Prevention, File Upload, Deserialization, XXE): https://cheatsheetseries.owasp.org/
