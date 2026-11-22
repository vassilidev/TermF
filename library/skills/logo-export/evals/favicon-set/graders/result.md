---
type: llm
---

PASS if the answer lists favicon.ico with several sizes (16, 32, 48), an SVG icon, apple-touch-icon at 180 × 180, 192 and 512 PNG icons in a web manifest, gives commands of a named tool (rsvg-convert, resvg, Inkscape, ImageMagick or sips) and says how to check each file produced.
FAIL if it gives a single favicon size, omits the 180 px touch icon, or invents a tool or flag.
