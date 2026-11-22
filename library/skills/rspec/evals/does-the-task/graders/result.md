---
type: llm
---

PASS if it is a request spec (type: :request, or under spec/requests) that sends PATCH /articles/:id, builds the author, the other user and the article with FactoryBot, and checks both the response status and the stored title (article.reload.title) for each criterion.
FAIL if it writes a controller spec, stubs Article or uses allow_any_instance_of, or checks only the status code.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
