# Upgrading from Spring Boot 3 to Spring Boot 4

Facts from the Spring Boot 4.0 migration guide and the 4.1 and 4.2 release notes on the Spring Boot wiki (checked on 2026-11-22). Read the guide itself for anything not listed here.

## Order of work

1. Move to the latest Boot 3.5.x first and fix every deprecation warning: Boot 4.0 removed what 3.x deprecated, and 4.1 and 4.2 removed what 4.0 deprecated.
2. Raise the build: Java 17 or later (Boot 4.2 runs up to Java 27), Gradle 8.14+ or 9.x, Maven 3.6.3+; Kotlin 2.2+ for Kotlin code.
3. Change the Boot version, then fix compile errors module by module with the tables below.
4. Run the whole test suite, then start the application and call its main endpoints and actuator health.

## Starters

| Boot 3 | Boot 4 |
|---|---|
| `spring-boot-starter-web` | `spring-boot-starter-webmvc` |
| `spring-boot-starter-aop` | `spring-boot-starter-aspectj` |
| `spring-boot-starter-test` alone | the test starter of each technology, such as `spring-boot-starter-webmvc-test` (it brings `spring-boot-starter-test`) |
| `spring-boot-starter-undertow` | removed: Tomcat 11 or Jetty 12.1 |
| `spring-boot-amqp` (4.0, 4.1) | `spring-boot-rabbitmq` (4.2) |

`spring-boot-starter-classic` and `spring-boot-starter-test-classic` bring back the old all-in-one classpath while you migrate; plan to remove them.

## Jackson 3

- Imports: `com.fasterxml.jackson.databind` and `.core` become `tools.jackson.databind` and `tools.jackson.core`; annotations keep `com.fasterxml.jackson.annotation`.
- Boot annotations: `@JsonComponent` becomes `@JacksonComponent`, `@JsonMixin` becomes `@JacksonMixin`.
- Mappers are immutable: build them with `JsonMapper.builder()...build()`. `Jackson2ObjectMapperBuilderCustomizer` becomes `JsonMapperBuilderCustomizer`, and replacing Boot's mapper takes a `JsonMapper` bean: an `ObjectMapper` bean is no longer enough.
- Exceptions are unchecked (`JacksonException`).
- Changed defaults to check in tests: unknown properties are ignored, dates are written as ISO-8601 strings, null into a primitive fails, properties are sorted alphabetically.
- To stay on Jackson 2 for a while: the deprecated `spring-boot-jackson2` module with properties under `spring.jackson2.*`.

## Tests

| Boot 3 | Boot 4 |
|---|---|
| `@MockBean`, `@SpyBean` (`org.springframework.boot.test.mock.mockito`) | `@MockitoBean`, `@MockitoSpyBean` (`org.springframework.test.context.bean.override.mockito`), on fields only, not in `@Configuration` classes |
| MockMvc available in any `@SpringBootTest` | add `@AutoConfigureMockMvc` |
| `TestRestTemplate` injected in `@SpringBootTest(webEnvironment = RANDOM_PORT)` | `@AutoConfigureTestRestTemplate`, or `RestTestClient` with `@AutoConfigureRestTestClient` (`TestRestTemplate` is deprecated in 4.2) |
| `org.springframework.boot.test.autoconfigure.web.servlet.WebMvcTest` | `org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest` |
| `org.springframework.boot.test.autoconfigure.orm.jpa.DataJpaTest` | `org.springframework.boot.data.jpa.test.autoconfigure.DataJpaTest` |
| `@Mock` handled by `MockitoTestExecutionListener` | `@ExtendWith(MockitoExtension.class)` |

`@JsonTest` stays in `org.springframework.boot.test.autoconfigure.json`.

## Other moves

- `@EntityScan`: `org.springframework.boot.persistence.autoconfigure.EntityScan`.
- `BootstrapRegistry`: `org.springframework.boot.bootstrap`.
- Null safety uses JSpecify annotations; actuator endpoint parameters that may be null use `org.jspecify.annotations.Nullable`. Kotlin and null checkers may report new errors.
- Liveness and readiness probes are on by default.
- Embedded launch scripts (fully executable jars) are gone.
- 4.1: the `layertools` jar mode is now `tools`; `-DskipTests` no longer skips test AOT processing (`-Dmaven.test.skip=true` does).
- 4.2: forwarded-header properties move to `server.request.header.forwarded`; `X-Forwarded-Prefix` is ignored by default; Pulsar support is deprecated.

## Checks

- `./gradlew build` or `./mvnw verify` green, with no deprecation warning from Boot left in the output.
- The application starts, `/actuator/health` answers `UP`, and one request per main endpoint returns what it did before the upgrade (compare JSON bodies: Jackson 3 defaults change dates and property order).
