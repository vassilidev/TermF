---
description: Error-message assertions after Go 1.27 moved encoding/json to the v2 implementation.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

After upgrading our module from go 1.26 to go 1.27, this test fails although the decoder still rejects the input:

```go
_, err := ParseConfig([]byte(`{"port": "eighty"}`))
if err == nil || err.Error() != "json: cannot unmarshal string into Go struct field Config.port of type int" {
    t.Fatalf("unexpected error: %v", err)
}
```

Why, and how should the test be written?
