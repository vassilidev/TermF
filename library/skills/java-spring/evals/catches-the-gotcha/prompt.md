---
description: A web starter and a mock in a Boot 4 build.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We are starting a new Spring Boot 4.2 REST service with Gradle (Kotlin DSL). Give me the dependencies block for a Spring MVC API with validation and its tests, and a test of `OrderController` that replaces `PaymentClient` with a Mockito mock and calls GET /orders/42 through MockMvc in a full `@SpringBootTest`.
