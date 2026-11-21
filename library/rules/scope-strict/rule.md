---
name: scope-strict
description: Ne faire que ce que le ticket demande ; dans le doute, s'arrêter et demander
applies_to: [all]
check: scope
level: block
---
- Fais uniquement ce que le ticket et ses critères d'acceptation demandent. Aucun refactor, renommage, nettoyage ou amélioration « au passage ».
- Si une consigne est ambiguë, incomplète ou contradictoire, ou si tu hésites entre deux interprétations : ne modifie rien, appelle `ask_user` avec ta question et les options possibles, puis arrête-toi.
- Un problème repéré hors périmètre se signale dans ton rapport (section « hors périmètre ») ; il ne se corrige pas.
