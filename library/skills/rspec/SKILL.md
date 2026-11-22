---
name: rspec
description: >-
  Writes, fixes and runs Ruby and Rails tests with RSpec or Minitest, whichever the project uses, the way
  the installed versions expect: tests derived from acceptance criteria, request specs and integration
  tests, system tests with Capybara, model tests against the real database, fixtures or FactoryBot,
  mocks at the boundaries, narrow and parallel runs. Use when a task adds or changes files under spec/ or
  test/, spec_helper.rb, rails_helper.rb or test_helper.rb. Not for application code (ruby-rails) and not
  for JavaScript tests.
paths: "**/spec/**/*.rb, **/test/**/*.rb, **/.rspec"
license: MIT
metadata:
  termf-stacks: "rspec minitest"
  termf-roles: "qa back"
  termf-versions: ">=3.12"
  termf-verified: "2026-11-22 rspec-rails@8.1.0 rspec-core@3.13.7 minitest@6.1.0 factory_bot@6.6.0 capybara@3.40.0 rails@8.1.4.1"
  termf-sources: "https://github.com/rspec/rspec-rails https://github.com/rspec/rspec https://github.com/minitest/minitest/blob/master/History.rdoc https://guides.rubyonrails.org/testing.html https://github.com/thoughtbot/factory_bot"
---
# RSpec and Minitest

1. Find the framework: `spec/` with `rails_helper.rb` means RSpec (rspec-rails), `test/` with `test_helper.rb` means Minitest, Rails' default. Read their versions in `Gemfile.lock` (`rspec-rails`, `rspec-core`, `minitest`, `factory_bot`, `capybara`) and how test data is built (fixtures in `test/fixtures` or `spec/fixtures`, or FactoryBot). Write in the framework the project has, never the other one. Done when you can name the framework, its version and the data style.
2. Take each test from an acceptance criterion or a real flow, following the skill `tests-from-criteria`; write down what each one protects. Done when every planned test names its criterion.
3. Choose the highest level the project can run:
   - a user flow in the browser: a system test (`ApplicationSystemTestCase`, or an RSpec system spec with `type: :system`) driven by Capybara, finding elements by their visible label or text;
   - an endpoint: a request spec (`type: :request`) or an integration test (`ActionDispatch::IntegrationTest`) that sends the HTTP request and checks the response and the rows it left; controller specs are not written any more, by the Rails and RSpec teams' recommendation;
   - model rules and queries: model tests against the real test database, with records from the project's fixtures or factories.
   Mock only what leaves the app (an HTTP API, a payment provider), at the boundary the app defines. Done when no test stubs the class it checks.
4. Keep setup readable: `let` (lazy) for what an example uses, `let!` only when the record must exist before the example runs, a method with a clear name rather than an implicit `subject`; build the fewest records the criterion needs. Done when each example reads top to bottom without jumping.
5. Run the new test narrowly, then break or revert the change it protects and see it fail for the right reason, then restore. Done when it fails without the change and passes with it.
6. Run `## Verify` and put each command with its result in your report.

## Gotchas

- rspec-rails 8.x supports Rails 7.2, 8.0 and 8.1; the RSpec code now lives in the `rspec/rspec` monorepo.
- RSpec system specs use `driven_by(:selenium_chrome_headless)` by default and do not read `ApplicationSystemTestCase`; set the driver in a `config.before(type: :system)` block. Capybara does not work inside request specs.
- `allow_any_instance_of` and `expect_any_instance_of` are discouraged by RSpec itself: inject the collaborator or stub the boundary object instead.
- `--only-failures` and `--next-failure` need `config.example_status_persistence_file_path`, which the generated `spec_helper.rb` leaves commented out.
- Minitest 6 moved `minitest/mock` to the `minitest-mock` gem, removed the `MiniTest` constant and `assert_send`, refuses `assert_equal nil, x` (use `assert_nil`), removed spec expectations on every object (use `_(x).must_equal`), and renamed the worker variable `N` to `MT_CPU`.
- Rails parallel tests (`parallelize(workers: :number_of_processors)` in `test_helper.rb`) only start above 50 tests; `PARALLEL_WORKERS` overrides the count. Each worker has its own database.
- `bin/rails test` never runs system tests: `bin/rails test:system`, or `bin/rails test:all` for everything. Rails 8.1 scaffolds no longer generate system tests, so add one when the criterion is a browser flow.

## Verify

- `bin/ci` when the app has it; otherwise the project's own test command.
- One test while iterating: `bin/rails test test/models/article_test.rb:12`, or `bundle exec rspec spec/requests/articles_spec.rb:12`.
- Before reporting: the whole suite (`bin/rails test` and `bin/rails test:system`, or `bundle exec rspec`).

## Security

- Tests never call real services or use real credentials: stub outside HTTP at the boundary (or with WebMock when the project has it), and keep test credentials fake.
- System tests run against the test database and a local server only; screenshots and logs they leave in `tmp/` are not committed.
- Fixtures and factories hold made-up data, never copies of production records.

## References

- `https://github.com/rspec/rspec-rails` (feature docs in the repository, MIT) for request and system spec setup; `https://guides.rubyonrails.org/testing.html` for Minitest helpers.
