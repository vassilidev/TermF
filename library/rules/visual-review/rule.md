---
name: visual-review
description: Revue avant / après de chaque écran modifié, avant la PR
applies_to: [front]
check: ui-changed
gate: visual
level: block
---
- Déclare dans `report.affected_views` chaque écran et chaque état touchés par ta modification.
