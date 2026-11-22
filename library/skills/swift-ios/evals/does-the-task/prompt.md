---
description: Tests from an acceptance criterion in a Swift package.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Acceptance criterion: "Orders of 100 € or more get 10 % off; below 100 € there is no discount." Our package (swift-tools-version: 6.2) has this in Sources/Pricing/Discount.swift:

```swift
public func discountedTotal(_ total: Decimal) -> Decimal {
    total >= 100 ? total * 0.9 : total
}
```

The test target PricingTests is empty. Write the tests.
