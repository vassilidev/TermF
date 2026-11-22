---
description: Fixing an N+1 query on a Django list page, with a test that guards it.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Django 6.1 page /books/ is slow in production. The view and template:

```python
# catalog/views.py
def book_list(request):
    books = Book.objects.order_by("title")
    return render(request, "catalog/book_list.html", {"books": books})
```

```html
{% for book in books %}
  <li>{{ book.title }} by {{ book.author.name }} ({% for tag in book.tags.all %}{{ tag.name }} {% endfor %})</li>
{% endfor %}
```

`Book.author` is a ForeignKey and `Book.tags` a ManyToManyField. Fix it and add a test that would catch it coming back.
