# Querysets that stay fast

A page that runs one query per row is the most common Django slowdown, and it does not show in tests with three rows. Count the queries, then fix the loading.

## Find the N+1

- In a test: `self.assertNumQueries(n)` around the request (`django_assert_num_queries` with pytest-django), with enough rows that a per-row query would show.
- Django 6.1: `queryset.fetch_mode(models.FETCH_RAISE)` raises `FieldFetchBlocked` on any lazy related load, which points at the exact line.
- Templates and serializers trigger queries too: `{{ book.author.name }}` in a loop is one query per book.

## Load related rows

- Forward foreign key or one-to-one: `select_related("author", "author__publisher")` (one SQL join).
- Many-to-many and reverse foreign keys: `prefetch_related("tags")`; filter or order the prefetched rows with `Prefetch("comments", queryset=Comment.objects.filter(visible=True))`.
- Django 6.1: `fetch_mode(models.FETCH_PEERS)` prefetches a related field for all the rows of the queryset at first access, when the code that will touch it is not known in advance (admin, generic views).
- Counts and sums per row: `annotate(n_comments=Count("comments"))`, not `obj.comments.count()` in the loop.

## Load less

- `only()` / `defer()` for wide tables; `values()` / `values_list()` when no model instance is needed.
- `exists()` to test for presence; `count()` only when the number is shown.
- `iterator(chunk_size=...)` for exports over many rows, so they do not sit in memory.

## Write without races

- Increment with `F()`: `Product.objects.filter(pk=pk).update(stock=F("stock") - 1)`, not read-modify-save.
- Read-then-write that must not interleave: `select_for_update()` inside `transaction.atomic()`.
- Many rows: `bulk_create`, `bulk_update` (with `batch_size`), `update()`; they skip `save()` and signals, so check nothing relies on them.

## Look at the plan

- `print(queryset.query)` shows the SQL; `queryset.explain()` (`analyze=True` on PostgreSQL) shows the plan. An index belongs in `Meta.indexes` and a migration (see `migrations.md`).
