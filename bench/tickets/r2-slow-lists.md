# The app is slow on big lists

**Type:** bug, performance

> From Camille, operations:
>
> Our "Warehouse" list is past 10,000 tasks now and the page takes ages to show it. Sometimes it just spins and
> the browser asks whether to stop the page. Changing the sort or picking a tag is just as slow, and so is opening
> the list on a phone. Smaller lists are fine. Can someone have a look? We will only get more tasks.

Reproduced on a copy of their data: `GET /api/lists/:id/tasks` on a list of 10,000 tasks takes 2 to 3 seconds on a
quiet developer machine and up to 9 on a busy one, and answers all 10,000 tasks in one response; the page then
renders every one of them.

## Expected

1. Opening a list, changing its sort or its filters is fast whatever the size of the list.
2. The tasks of a list come in pages (see the API notes). The order, the filters and the fields of each task stay
   as the README documents them.
3. The page follows: it shows the first page and loads the next ones on demand.
4. Tests cover paging; the README documents it.

## API notes

`GET /api/lists/:id/tasks` takes two more parameters, `limit` (1 to 200, default 50) and `cursor`, on top of the
existing filters and sorts, and answers `{"items": [<task>, …], "next_cursor": "<opaque string>" | null}` instead of
an array: this changes the shape of the response, and the page must follow. `next_cursor` is `null` on the last page;
otherwise, passed back as `cursor` with the same filters and sort, it answers the next page. A cursor only works with
the list and the sort it came from: any other `cursor` answers `400`.
