---
type: llm
---

PASS if the queryset loads the author with select_related("author") and the tags with prefetch_related("tags") (or uses fetch_mode(models.FETCH_PEERS)), and the test requests the page through the test client with several books and asserts a fixed query count (assertNumQueries or django_assert_num_queries).
FAIL if the fix adds caching or raw SQL instead, or the test does not bound the number of queries.
