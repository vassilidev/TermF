---
name: tests-from-criteria
description: >-
  Decides what to test and at which level, then proves each new test can fail: tests come from the
  acceptance criteria and the real user flows, at the highest level the project can run, and never
  restate the implementation. Use when adding or changing behaviour that needs tests, fixing a bug,
  writing a regression test, or judging whether existing or proposed tests are useful, in any
  language or framework. Not for setting up or configuring a test runner (see the framework skill:
  testing-js, playwright, pytest, pest…), nor for changes with no behaviour (docs, formatting, config).
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "qa front back devops"
  termf-versions: "*"
  termf-verified: "2026-11-22 method"
  termf-sources: "https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents https://github.com/mattpocock/skills/tree/main/skills/engineering/tdd"
  credits: "mattpocock/skills@abeeaf95660ba9e8b7c753eb38f441ca707a8823 (MIT): skills/engineering/tdd/SKILL.md, skills/engineering/tdd/tests.md, skills/engineering/tdd/mocking.md"
---
# Tests from criteria

1. **List what must hold.** Write the acceptance criteria of the ticket or plan, and the user flows the change touches, as a short list of observable outcomes (given this input or action, the user or caller sees this). For a bug, the reproduction steps and the expected result. Done when every item can be checked from outside the code.
2. **Use the test tools the project already has**, because a second runner splits the suite. Read its test scripts (`package.json`, `Makefile`, `composer.json`, `pyproject.toml`, the CI workflow) and two neighbouring tests; reuse their runner, folders, naming, fixtures and helpers. The framework skill of the stack (`testing-js`, `playwright`, `pytest`, `pest`…) says how to write and run tests there. Done when you know the command that runs one test and the one that runs the suite.
3. **Choose the level, item by item: the highest the project can run.**
   - End to end through the real interface or API, when the project has such tests and the item is a user flow.
   - Integration with the real collaborators (database, HTTP handler, queue, filesystem in a temp dir) for most behaviour.
   - Unit only for real logic: rules, calculations, parsing, formatting, state machines, edge cases.
   - A fake only at the boundary the project does not control.
   Done when each item has a level and you can say why a higher one would not work.
4. **Write the test.** Name it after the expected behaviour (`rejects an expired coupon at checkout`, not `test applyCoupon`). Arrange through public entry points, act like the user or caller, assert the observable outcome against an independent expected value. One behaviour per test; add the edge cases the criterion implies (empty, limit, invalid, not allowed). Done when it passes with the change.
5. **Prove it can fail.** Break what the test protects (revert the fix, invert the condition, remove the branch), run the test, and check it fails for the expected reason: an assertion about the behaviour, not a setup or import error. Restore the code and run it green again. For a bug, write the test before the fix and see it fail first. Done when you have seen red, then green, and the working tree holds no broken code.
6. **Run the commands of `## Verify` and put each one with its result in your report**, the narrowest set first, then everything CI runs for this part. For each test, say the criterion it protects, when it fails, and what its passing does not prove (for example "unit test of the discount rule; does not prove the cart page shows the discount"). List the criteria left without an automated test, why, and the manual check you did instead. Done when they pass, or when the failure is reported with its output.

## Gotchas
- The usual agent test calls the function and expects what the function was written to return. It passes by construction and protects nothing: a test earns its place only if it fails without the change (step 5).
- Expected values come from the criterion, a worked example or a known-good literal. Recomputing them the way the code does (`expect(total(xs)).toBe(xs.reduce(…))`) makes the test agree with any bug.
- Mocking the project's own modules tests the mock. Replace only what the project does not control (third-party APIs, payment, email, clock, randomness) and keep the real database, HTTP layer and filesystem the run provides.
- Coverage is not the goal. A criterion with no test is the gap to close; a line covered by a test that cannot fail is not covered.
- Existing tests record decisions. Leave their assertions, skips and snapshots as they are; when one looks wrong, report it with your evidence instead of loosening, skipping, deleting or re-recording it to get green.
- A flaky test is a broken test: find the race or the missing wait-for-state. Retries, sleeps and longer timeouts hide it.

## Examples

<example>
Restating the implementation:
```
BAD   test "total"
        items = [10, 5]
        expect total(items) == sum(items)       # agrees with any bug in total
GOOD  test "total of two lines is the sum of their prices"
        expect total([10, 5]) == 15
```
</example>

<example>
Mocking the project's own code:
```
BAD   test "checkout calls PaymentService.charge"
        mock PaymentService; checkout(cart); expect charge called with cart.total
GOOD  test "a declined card leaves the order unpaid and shows the reason"
        fake the external gateway to answer "declined"
        checkout(cart) through the app's entry point
        expect order.status == "unpaid" and the page or response says "card declined"
```
</example>

<example>
A level too low for the criterion "a signed-out visitor who opens /account lands on the login page":
```
BAD   unit test: isAuthenticated(null) returns false
GOOD  request GET /account with no session -> redirect to /login
      (or, with browser tests in the project: open /account, expect the login form)
```
</example>

<example>
Never seen failing:
```
BAD   fix written, test added afterwards, one green run, assertion `result is not null`
GOOD  bug "the discount is applied twice when the form is sent twice"
      test sends the same order twice and expects one discount line
      before the fix: fails with "expected 1 discount line, got 2"; after: passes
```
</example>

## Verify
- Every new test was seen failing without the change and passing with it (step 5); say how in the report.
- The project's test command passes on the whole suite CI runs for the touched part.
- `git diff` on test files shows only additions, or changes the ticket asked for: no skip, no deleted test, no loosened assertion, no re-recorded snapshot.

## Security
- Tests run against the local services of the run (test database, local server, fakes). A real environment, a production URL or a real account is never a test target.
- Fixtures hold obviously fake data and secrets (`test-token`, `alice@example.test`), read from the project's test env files; a real key in a fixture ends up in git.
- Every access rule in the criteria gets its negative test: another user's resource, a missing role, a signed-out request is refused. Broken access control is the top web risk (OWASP A01:2025) and the case nobody tests.
- No network calls in unit or integration tests: a fake at the boundary keeps them deterministic and keeps test data from leaving the machine.

## References
- How to write and run tests in this stack: the framework skill (`testing-js`, `playwright`; `pytest`, `pest` and others in their own skills).
- Anthropic, [Prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices): tests verify correctness, they do not define the solution.
- Anthropic, [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents): test end to end as a user would.
