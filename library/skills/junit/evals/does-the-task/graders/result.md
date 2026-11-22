---
type: llm
---

PASS if the test runs against PostgreSQL in a container (Testcontainers with @ServiceConnection, or an equivalent real PostgreSQL), uses a JPA slice (@DataJpaTest with the database not replaced) or a @SpringBootTest, inserts an old open order, a recent open order and an old closed order, and asserts that only the first is returned.
FAIL if it uses H2 or another in-memory database for this native PostgreSQL query, mocks the repository, or covers only one of the three cases.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
