---
description: A repository test against the real database engine in a Spring Boot 4 service.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Spring Boot 4.2 service stores orders in PostgreSQL 17 with Spring Data JPA. This query was added:

```java
public interface OrderRepository extends JpaRepository<Order, Long> {
    @Query(value = "select * from orders where status = 'OPEN' and created_at < now() - interval '30 days'", nativeQuery = true)
    List<Order> findStaleOpenOrders();
}
```

Acceptance criterion: "Open orders older than 30 days are listed as stale; newer or closed ones are not." Write the test. Testcontainers is available in the build.
