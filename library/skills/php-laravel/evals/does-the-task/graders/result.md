---
type: llm
---

PASS if validation lives in a Form Request (or the controller's validate call) and the update uses only validated data, and authorization goes through a policy (ProjectPolicy@update via Gate::authorize, $this->authorize, can middleware or #[Authorize]) that compares the project's user_id with the current user.
FAIL if it updates with $request->all() or unvalidated input, or relies only on the auth middleware without checking ownership.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
