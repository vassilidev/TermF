---
type: llm
---

PASS if the command it gives to run the class uses the xUnit v3 option `--filter-class` (for example `dotnet test --project tests/Acme.Orders.Tests --filter-class Acme.Orders.Tests.OrderTotalsTests`).
FAIL if the command it gives uses the VSTest syntax `--filter "FullyQualifiedName~..."` as the way to run it.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
