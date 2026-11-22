---
description: PHPUnit data providers after annotations were removed.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded a Laravel package to PHPUnit 12. This test now runs once with no data and fails:

```php
/**
 * @dataProvider amounts
 */
public function test_fee_is_rounded(int $cents, int $expected): void
{
    $this->assertSame($expected, Fee::for($cents));
}

public static function amounts(): array
{
    return [[199, 200], [250, 250]];
}
```

Fix it.
