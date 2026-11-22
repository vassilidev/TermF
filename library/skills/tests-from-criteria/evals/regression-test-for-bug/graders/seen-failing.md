---
type: regex
target: last_message
flags: i
pattern: '(fails?|failing|\bred\b)[^\n]{0,160}(without|before|revert|undo)[^\n]{0,40}(fix|change|patch)|(without|before|revert|undo|stash)[^\n]{0,40}(fix|change|patch)[^\n]{0,160}(fails?|failing|\bred\b)|unfixed[^\n]{0,160}(fails?|failing)|(fails?|failing)[^\n]{0,160}unfixed'
---
