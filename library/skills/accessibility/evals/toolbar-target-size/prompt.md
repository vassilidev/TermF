---
description: A review of an icon toolbar against WCAG 2.2 AA cites the 2.2 target-size criterion by number.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
Our audit target is WCAG 2.2 AA. Can you review this editor toolbar and give me a fixed version? The icons are 16×16 and the buttons have no padding, they sit 2px apart.

```tsx
export function Toolbar({ onBold, onItalic, onLink }: Props) {
  return (
    <div className="flex gap-[2px]">
      <div className="icon-btn" onClick={onBold}><BoldIcon width={16} height={16} /></div>
      <div className="icon-btn" onClick={onItalic}><ItalicIcon width={16} height={16} /></div>
      <div className="icon-btn" onClick={onLink} aria-label="Insert link"><LinkIcon width={16} height={16} /></div>
    </div>
  );
}
```

List each problem with the WCAG criterion it breaks, then the fixed component.
