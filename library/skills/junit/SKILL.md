---
name: junit
description: >-
  Writes, fixes and runs JVM tests the way the installed JUnit, Spring Boot and Testcontainers versions
  expect: tests derived from acceptance criteria, JUnit Jupiter, AssertJ, Spring Boot test slices and
  full application tests, Testcontainers for real databases and brokers, Mockito in Java and MockK in
  Kotlin, Gradle and Maven test runs. Use when a task adds or changes tests under src/test of a Java or
  Kotlin service, or their test dependencies. Not for Android instrumented or Compose UI tests
  (kotlin-android), and not for JavaScript tests.
paths: "**/src/test/**/*.java, **/src/test/**/*.kt, **/src/integrationTest/**, **/pom.xml, **/build.gradle, **/build.gradle.kts"
license: MIT
metadata:
  termf-stacks: "junit java kotlin spring-boot"
  termf-roles: "qa back"
  termf-versions: ">=5.10"
  termf-verified: "2026-11-22 junit@6.1.3 assertj@3.27.7 mockito@5.24.0 mockk@1.14.12 testcontainers-java@2.0.5 spring-boot@4.2.0"
  termf-sources: "https://docs.junit.org/current/release-notes/ https://docs.junit.org/6.0.0/release-notes/ https://docs.spring.io/spring-boot/reference/testing/spring-boot-applications.html https://docs.spring.io/spring-boot/reference/testing/testcontainers.html https://java.testcontainers.org/ https://github.com/testcontainers/testcontainers-java/releases/tag/2.0.0 https://javadoc.io/static/org.mockito/mockito-core/5.24.0/org.mockito/org/mockito/Mockito.html https://docs.gradle.org/current/userguide/java_testing.html"
---
# JUnit and JVM tests

1. Read the versions: `junit-bom` or `junit-jupiter`, `mockito-core`, `assertj-core`, `testcontainers-bom`, `mockk` in the build or the version catalog, or the ones Spring Boot's dependency management sets. Note the test task setup (`useJUnitPlatform()` in Gradle, Surefire and Failsafe in Maven) and whether Docker is available for Testcontainers. Done when you can name each version and how tests run.
2. Take each test from an acceptance criterion or a real flow, following the skill `tests-from-criteria`; write down what each one protects. Done when every planned test names its criterion.
3. Choose the highest level the project can run, and the narrowest Spring context that still covers it:
   - an HTTP endpoint with its controller, validation and error handling: `@WebMvcTest(OrderController.class)` with MockMvc, or a full `@SpringBootTest` with `@AutoConfigureMockMvc` (or `RestTestClient`) when the flow crosses services;
   - queries and mappings: `@DataJpaTest` (or `@DataJdbcTest`) against the production database engine in a container with `@ServiceConnection`, not H2, which accepts SQL the real engine rejects and the reverse;
   - real rules (pricing, parsing, state machines): plain JUnit tests without Spring.
   Done when no test mocks the code it is meant to check.
4. Mock only what leaves the service (a payment API, a mail server), with `@MockitoBean` in Spring tests, Mockito in plain Java tests, MockK in Kotlin tests. Assert with AssertJ (`assertThat(...)`) when the project uses it. Done when each mock stands for an outside system.
5. Run the new test narrowly, then break or revert the change it protects and see it fail for the right reason, then restore. Done when it fails without the change and passes with it.
6. Run `## Verify` and put each command with its result in your report.

## Gotchas

- JUnit 6 (current 6.1): Java 17 and Kotlin 2.2 minimum; Platform, Jupiter and Vintage share one version number (no more Platform 1.x); Vintage (JUnit 4) is deprecated; `@RunWith(JUnitPlatform.class)` (`junit-platform-runner`) and `MethodOrderer.Alphanumeric` are removed; Kotlin `suspend` test methods are allowed.
- Gradle 9 needs `testRuntimeOnly("org.junit.platform:junit-platform-launcher")` next to `useJUnitPlatform()`. Surefire 3.6 runs every test through the JUnit Platform (JUnit 4 tests through Vintage).
- Spring Boot 4: `@MockitoBean` and `@MockitoSpyBean` replace the removed `@MockBean` and `@SpyBean`; `@SpringBootTest` needs `@AutoConfigureMockMvc` for MockMvc; slice annotations moved (for example `org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest`, `org.springframework.boot.data.jpa.test.autoconfigure.DataJpaTest`); each technology has its test starter (`spring-boot-starter-webmvc-test`).
- Testcontainers 2.0: JUnit 4 support removed; module artifacts are prefixed (`org.testcontainers:testcontainers-postgresql`); container classes moved to `org.testcontainers.<module>` (`org.testcontainers.postgresql.PostgreSQLContainer`); module constructors take the image explicitly.
- With Spring, declare containers as beans (`@Bean @ServiceConnection` in a `@TestConfiguration`) rather than static `@Container` fields: the JUnit extension stops a static container after its class while Spring keeps the cached context that points at it.
- Mockito 5 uses the inline mock maker, which attaches a Java agent; on JDK 21 and later that self-attach is restricted, so the build passes `-javaagent` to the test JVM (Maven `argLine` with the `mockito-core` jar, Gradle a `jvmArgumentProviders` entry). Add it when the warning appears, rather than silencing it.

## Verify

- The project's commands first (CI steps, `Makefile`).
- One test while iterating: `./gradlew test --tests 'com.acme.orders.OrderControllerTest'` (a method name may follow the class), or `./mvnw test -Dtest=OrderControllerTest`.
- Before reporting: `./gradlew build` or `./mvnw verify` (Maven runs `*IT` integration tests in `verify` through Failsafe when configured).

## Security

- Tests never use real credentials or reach real services: secrets come from test properties with fake values, outside APIs are mocked or replaced by a local stub, databases run in containers.
- Container images are pinned to a version tag the project already uses, never `latest`.
- The test profile must not disable security for the whole application to make a test pass: authenticate the test request (`@WithMockUser`, a test token) instead.

## References

- `https://docs.spring.io/spring-boot/reference/testing/` for the slice that fits a layer; `https://java.testcontainers.org/` for a module's container class.
