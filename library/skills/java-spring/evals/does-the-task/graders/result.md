---
type: llm
---

PASS if the configuration targets Jackson 3 as used by Boot 4: imports from tools.jackson (not com.fasterxml.jackson.databind), and customizes Boot's mapper through a JsonMapperBuilderCustomizer bean or the spring.jackson properties, or registers a serializer through @JacksonComponent.
FAIL if it uses com.fasterxml.jackson.databind classes, Jackson2ObjectMapperBuilderCustomizer, or declares an ObjectMapper bean to replace Boot's mapper.
