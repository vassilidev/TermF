---
name: no-text-changes
description: Toute modification textuelle non demandée est interdite
applies_to: [front, back, devops]
check: text-changes
level: block
---
- Ne modifie aucun texte visible ou éditorial (libellés, messages d'erreur, traductions, contenus, README, commentaires existants) si le ticket ne le demande pas explicitement.
- Si ta tâche exige un nouveau texte, reprends mot pour mot celui du ticket. S'il n'est pas fourni, demande-le via `ask_user`.
- Déclare chaque texte ajouté ou modifié dans `report.text_changes`, avec le critère qui le justifie.
