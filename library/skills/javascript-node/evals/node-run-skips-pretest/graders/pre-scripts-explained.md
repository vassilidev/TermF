---
type: regex
target: last_message
flags: i
pattern: '--run[^\n]{0,160}(skip|ignor|does ?n[o''`]t (run|execute|trigger)|won''?t (run|execute|trigger)|not (run|execute|trigger)|never (runs?|executes?|triggers?)|without)[^\n]{0,80}\b(pre|post)|\b(pre|post)[\w-]*[^\n]{0,160}(skipped|ignored|not (run|executed|triggered)|never (run|executed|triggered))[^\n]{0,120}--run'
---
