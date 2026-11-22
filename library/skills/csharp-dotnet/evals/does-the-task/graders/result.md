---
type: llm
---

PASS if the test goes through WebApplicationFactory<Program> and its HttpClient, and checks the saved order against a real PostgreSQL database (Testcontainers, or a test database whose connection string replaces "Shop").
FAIL if it swaps the database for the EF Core in-memory provider (UseInMemoryDatabase), mocks ShopDb or DbSet, or never checks that the order was saved.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
