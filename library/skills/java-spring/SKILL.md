---
name: java-spring
description: >-
  Builds and changes Java and Spring Boot applications the way the installed Boot and JDK versions expect:
  controllers and services, configuration and properties, Spring Data repositories, Jackson serialization,
  Spring Security, actuator, starters and dependencies in Maven or Gradle, Boot 3 to 4 upgrades. Use when
  a task touches .java files, pom.xml, build.gradle(.kts), application.properties or application.yml of a
  JVM service. Not for Android apps (kotlin-android), and test design lives in the junit skill.
paths: "**/*.java, **/pom.xml, **/build.gradle, **/build.gradle.kts, **/settings.gradle*, **/application*.properties, **/application*.yml, **/application*.yaml"
license: MIT
metadata:
  termf-stacks: "java spring-boot"
  termf-roles: "back qa"
  termf-versions: "spring-boot>=3.4 java>=17"
  termf-verified: "2026-11-22 spring-boot@4.2.0 spring-framework@7.1.0 jdk@25-lts gradle@9.9.2 maven@3.10.1"
  termf-sources: "https://github.com/spring-projects/spring-boot/wiki/Spring-Boot-4.0-Migration-Guide https://github.com/spring-projects/spring-boot/wiki/Spring-Boot-4.1-Release-Notes https://github.com/spring-projects/spring-boot/wiki/Spring-Boot-4.2-Release-Notes https://docs.spring.io/spring-boot/system-requirements.html https://docs.spring.io/spring-boot/reference/actuator/endpoints.html https://docs.gradle.org/llms.txt https://github.com/gradle/gradle-skills"
---
# Java and Spring Boot

Spring ships no agent skill or `llms.txt`. Gradle publishes official skills in [gradle/gradle-skills](https://github.com/gradle/gradle-skills) (Apache-2.0: `gradle-best-practices`, `gradle-wrapper-upgrade`) and `https://docs.gradle.org/llms.txt`; follow them for build questions when the plugin is loaded.

1. Read the versions: the Boot version in the `spring-boot-starter-parent` parent of `pom.xml` or the `org.springframework.boot` plugin of the Gradle build, the Java version the build targets (Gradle toolchain, `java.version` or the compiler release), and the wrapper version (`gradle/wrapper/gradle-wrapper.properties`, `.mvn/wrapper/maven-wrapper.properties`). Done when you can name Boot, Java and the build tool.
2. Open the docs of that Boot version: `https://docs.spring.io/spring-boot/<version>/` and the release notes or migration guide on the Boot wiki. Done when the page for the task is open.
3. Use what the project has: its build tool through the wrapper only (`./mvnw`, `./gradlew`), its starters, its persistence (JPA, JDBC, R2DBC), its mapping and validation style, its package layout. Add a dependency without a version when Boot manages it. Done when no new dependency pins a version Boot already manages.
4. Write the change the way neighboring code does: constructor injection, configuration in `@ConfigurationProperties` classes, validation with `@Valid` on request bodies, transactions on service methods. Done when the build compiles with no new warning.
5. Write the tests with the skill `junit`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Spring Boot 4 (Spring Framework 7, Jakarta EE 11, Java 17 to 27) has modular starters: `spring-boot-starter-web` became `spring-boot-starter-webmvc`, `-aop` became `-aspectj`, and each technology has its test starter (`spring-boot-starter-webmvc-test`). The old names are deprecated; `-classic` starters exist only as a migration aid.
- Boot 4 uses Jackson 3: packages `tools.jackson.*` (annotations stay in `com.fasterxml.jackson.annotation`), `@JacksonComponent` and `@JacksonMixin` replace `@JsonComponent` and `@JsonMixin`, mappers are immutable and built with builders, exceptions are unchecked, and defaults changed (unknown properties ignored, dates written as ISO strings, properties sorted). Jackson 2 remains only through the deprecated `spring-boot-jackson2` module.
- Boot 4 tests: `@MockBean` and `@SpyBean` are removed (`@MockitoBean`, `@MockitoSpyBean`, fields only); `@SpringBootTest` no longer configures MockMvc (`@AutoConfigureMockMvc`) nor `TestRestTemplate` (`@AutoConfigureTestRestTemplate`, or `RestTestClient` with `@AutoConfigureRestTestClient`); test slice annotations moved to per-module packages (for example `org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest`). Boot 4.2 deprecates `TestRestTemplate` and the `RestTemplate` support.
- Boot 4 dropped Undertow (no Servlet 6.1 support) and embedded launch scripts, moved `@EntityScan` to `org.springframework.boot.persistence.autoconfigure`, adopted JSpecify nullability, and needs Kotlin 2.2+ for Kotlin code.
- Boot 4.1 and 4.2 each removed what 4.0 deprecated; 4.2 renamed `spring-boot-amqp` to `spring-boot-rabbitmq` and moved forwarded-header properties to `server.request.header.forwarded`.
- Maven 3.10 fails the build on several former warnings (duplicate dependencies or plugins); Gradle 9 needs `junit-platform-launcher` declared as `testRuntimeOnly`.

## Verify

- The project's commands first (CI steps, `Makefile`).
- Gradle: `./gradlew build` (compiles and runs the tests). Maven: `./mvnw verify`.

## Security

- Actuator exposes only `health` over HTTP by default; never set `management.endpoints.web.exposure.include=*`, and list the endpoints you need. A custom `SecurityFilterChain` turns off Boot's default web security, actuator rules included, so secure those endpoints in it.
- Keep CSRF protection on for browser sessions; turn it off only for stateless token APIs.
- Never evaluate a SpEL expression from user input with `StandardEvaluationContext`; `SimpleEvaluationContext` for data binding only.
- Never enable polymorphic default typing on a Jackson mapper that reads request bodies.
- The generated `user` password logged at startup is for development only; real users come from the project's user store.
- Outgoing HTTP to URLs taken from input: restrict the targets (Boot 4.1 clients accept an `InetAddressFilter`) to block requests to internal addresses.

## References

- Read `references/boot-4-migration.md` before upgrading from Boot 3, or when a build fails on a Boot 3 name (starter, package, annotation or property).
