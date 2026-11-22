---
description: Tests from acceptance criteria for a pricing rule.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Acceptance criteria for shipping fees:
- Orders under 50 EUR pay 4.90 EUR shipping.
- Orders of 50 EUR or more ship free.
- A negative total is rejected with ValueError.

The code is `acme/shipping.py`:

```python
from decimal import Decimal

def shipping_fee(total: Decimal) -> Decimal:
    if total < 0:
        raise ValueError("negative total")
    return Decimal("0") if total >= 50 else Decimal("4.90")
```

Our pyproject.toml has `[tool.pytest]` with `strict = true`. Write tests/test_shipping.py.
