---
type: llm
---

PASS if the token file writes each colour as an object with colorSpace and components (hex optional) and each spacing value as an object with value and unit, uses $type and $value, and explains a command that generates CSS custom properties from the file.
FAIL if a colour is written as a plain hex string or a dimension as a "16px" string in $value, or no generation step is given.
