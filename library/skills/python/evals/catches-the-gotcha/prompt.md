---
description: Choosing the event loop after the asyncio policy deprecation.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

We are moving our service to Python 3.15. At startup we do:

```python
import asyncio, uvloop
asyncio.set_event_loop_policy(uvloop.EventLoopPolicy())
asyncio.run(main())
```

Python now warns about it. Rewrite these lines the way current Python expects.
