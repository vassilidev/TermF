---
type: llm
---

PASS if the file uses Pest syntax (it/test closures), goes through HTTP with actingAs and patchJson (or patch), uses RefreshDatabase (in the file or already applied by tests/Pest.php), builds records with factories, and checks each criterion on the database (assertDatabaseHas / assertDatabaseMissing or a fresh model) as well as on the status: 200-family for the owner, 403 for the other user, a validation error on name.
FAIL if it mocks Eloquent or the controller, checks only status codes without the stored name, or writes a PHPUnit class with doc-comment annotations.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
