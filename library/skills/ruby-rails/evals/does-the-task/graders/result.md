---
type: llm
---

PASS if update reads parameters with params.expect(article: [:title, :body]) (or require/permit with only those keys), index maps params[:sort] through an allow-list of columns before ordering, and the list eager loads the author (includes/preload) and counts comments without one query per article (counter cache, a grouped count or a left join with count).
FAIL if it passes params[:sort] straight to order, uses permit!, or loads the author or comments lazily inside the loop.
