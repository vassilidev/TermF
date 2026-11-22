---
name: ruby-rails
description: >-
  Builds and changes Ruby and Rails applications the way the installed Ruby and Rails versions expect:
  routes, controllers and strong parameters, Active Record models, queries and migrations, jobs on Solid
  Queue, views with Hotwire, credentials, gems and Bundler, the local CI of bin/ci, RuboCop and Brakeman.
  Use when a task touches .rb or .erb files, Gemfile, Gemfile.lock, config/routes.rb, db/migrate or
  app/. Not for writing tests (the rspec skill covers RSpec and Minitest), and not for non-Rails Ruby gems
  beyond the Ruby facts.
paths: "**/Gemfile, **/Gemfile.lock, **/.ruby-version, **/Rakefile, **/config/**/*.rb, **/app/**/*.rb, **/app/**/*.erb, **/lib/**/*.rb, **/db/**/*.rb"
license: MIT
metadata:
  termf-stacks: "rails ruby"
  termf-roles: "back qa"
  termf-versions: ">=7.1"
  termf-verified: "2026-11-22 rails@8.1.4.1 ruby@4.0.10 bundler@4.1.0"
  termf-sources: "https://guides.rubyonrails.org/8_1_release_notes.html https://guides.rubyonrails.org/8_0_release_notes.html https://www.ruby-lang.org/en/news/2025/12/25/ruby-4-0-0-released/ https://www.ruby-lang.org/llms.txt https://docs.ruby-lang.org/llms.txt https://blog.rubygems.org/2025/12/03/upgrade-to-rubygems-bundler-4.html https://rubyonrails.org/maintenance https://guides.rubyonrails.org/security.html"
---
# Ruby and Rails

Rails ships no agent skill, `llms.txt` or MCP server, and `rails new` writes no `AGENTS.md`; Ruby has `https://www.ruby-lang.org/llms.txt` and `https://docs.ruby-lang.org/llms.txt`.

1. Read the versions: `.ruby-version`, the `RUBY VERSION`, `BUNDLED WITH` and `rails (x.y.z)` entries of `Gemfile.lock`, or `bin/rails --version`. Note the database, the job backend, the asset pipeline (Propshaft or Sprockets) and whether `bin/ci` exists. Done when you can name Ruby, Rails, Bundler and those choices.
2. Open the docs of that version: `https://guides.rubyonrails.org/v<major.minor>/` and `https://api.rubyonrails.org/v<major.minor>/`, plus the Ruby docs through `https://docs.ruby-lang.org/llms.txt`. Done when the page for the task is open.
3. Use what the app has: `bin/rails generate` for new files (so they land where Rails expects), its job backend, its frontend (Hotwire, importmap or a bundler), its service objects or concerns, RuboCop config. Add gems with `bundle add <gem>` (with `--group development,test` for tooling). Done when every file and gem follows an existing pattern.
4. Write the change: strong parameters with `params.expect(...)`, authorization the way the app does it, queries that eager load every association shown in a list (`includes`), schema changes in a new reversible migration (never an edited old one). On a large PostgreSQL table, add indexes with `algorithm: :concurrently` and `disable_ddl_transaction!`. Done when `bin/rails db:migrate` and `bin/rails db:rollback` both run on the development database.
5. Write the tests with the skill `rspec`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Rails 8.0: Solid Queue, Solid Cache and Solid Cable replace Redis-backed defaults; Propshaft replaces Sprockets; Kamal 2 and Thruster deploy; `bin/rails generate authentication` gives built-in sessions. Strong parameters use `params.expect(post: [:title, :body])`, which rejects malformed shapes with 400, instead of `params.require(:post).permit(...)`.
- Rails 8.1: local CI with `bin/ci` running the steps of `config/ci.rb`; Active Job continuations (`include ActiveJob::Continuable`, `step :name`) for long jobs that resume after a restart; `has_many :x, deprecated: true`; `bin/rails credentials:fetch`. Removed: `bin/rake stats`, the `:azure` storage service, `;` as a query-string separator; the built-in Sidekiq adapter is deprecated (it lives in the sidekiq gem); `schema.rb` sorts columns alphabetically, so a schema diff after a migration may reorder lines.
- Rails 8.1 scaffolds no longer generate system tests, and `bin/rails test` never runs them (`bin/rails test:system` or `bin/rails test:all`).
- Ruby 4.0: `Set` and `Pathname` are core (`Set#inspect` prints `Set[1, 2]`; `SortedSet` is gone, use the `sorted_set` gem); `Ractor::Port` replaces `Ractor.yield` and `Ractor#take`; `Net::HTTP` no longer adds `Content-Type: application/x-www-form-urlencoded` to requests with a body, so set it yourself; CGI is no longer a default gem (only `cgi/escape` remains); `Kernel#open` with a leading `|` is removed.
- Bundler 4 removed the `bundle install` flags `--path`, `--deployment`, `--frozen`, `--without`, `--with`, `--clean` and `--system`: use `bundle config set --local <key> <value>` (for example `without development test`). New lockfiles carry a `CHECKSUMS` section; `Bundler.with_clean_env` is gone (`with_unbundled_env`).
- Rails 8.0.x security fixes end on 2026-11-07; Ruby 3.2 is end of life. Say so when the app is on them.

## Verify

- `bin/ci` when the app has it (it runs `config/ci.rb`: setup, RuboCop, bundler-audit, importmap audit, Brakeman, tests, seeds).
- Otherwise: `bin/rubocop`, `bin/brakeman --no-pager`, `bin/rails test` (or `bundle exec rspec`), `bin/rails test:system` when system tests exist, `bin/rails db:migrate:status` after a migration.

## Security

- Never interpolate input into SQL fragments (`where("name = '#{x}'")`, `order(params[:sort])`, `find_by_sql`, `connection.execute`): use placeholders (`where("name = ?", x)`), hash conditions, or an allow-list for column names.
- `params.expect` or `permit` with explicit keys; `permit!` allows every current and future attribute.
- `html_safe` and `raw` mark a string as safe without escaping it: only for markup the app built; user HTML goes through `sanitize` with allowed tags.
- `redirect_to` with a URL taken from params is an open redirect: redirect to paths, or check the host.
- Secrets live in credentials (`bin/rails credentials:edit`), never in the repository; add new secret parameter names to `config.filter_parameters`.
- Login and password-reset endpoints get `rate_limit` (Rails 7.2+), as the authentication generator does.

## References

- `https://guides.rubyonrails.org/upgrading_ruby_on_rails.html` before a Rails upgrade; the release notes of each version on the way.
