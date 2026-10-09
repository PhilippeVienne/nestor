# Boucle proactive : les alertes à l'initiative de Nestor

Nestor ne parle pas seulement quand on lui parle. Une tâche de fond de nestord
(`nestord/src/proactive.rs`) évalue des règles à intervalle régulier et, quand
l'une se déclenche, réinjecte une **alerte** dans la conversation comme rapport
interne, sur le modèle des comptes rendus de mission : c'est Nestor qui la
formule, avec sa personnalité et ses contraintes de concision. Aucun texte brut
n'est poussé au TTS.

## Règles

| Règle | Déclencheur | Paliers |
|---|---|---|
| `mission_stalled` | mission `started` sans appel d'outil depuis `mission_stall_minutes` | 1, 3, 6, 9… fois le délai (10, 30, 60, 90 min par défaut) |
| `session_fallback` | session Claude toujours en mode réduit | tous les `fallback_remind_minutes` (30 min) ; la bascule elle-même est annoncée par `brain.rs` |
| `quota` | pire fenêtre du quota Claude ≥ `quota_threshold` (90 %) | une alerte par tranche de 5 points ; oubliée une fois redescendu de 10 points |
| `todo_due` | tâches dues (`todo.rs`), toutes les `todo_interval_minutes` (5 min) | mémoire du magasin (`mark_notified`) |

Chaque règle est réévaluée à chaque tour : une condition disparue ne produit
plus rien, et une alerte en attente n'est jamais rejouée à retardement.

## Discrétion

- **Dédoublonnage** par événement et par palier, en mémoire. Une mission
  terminée ou un retour en mode normal remettent le compteur à zéro.
- **Silence** en heures calmes (`quiet_hours`), quand aucun client n'est
  connecté à `/ws` (ce serait du quota dépensé pour personne), et pendant que
  Nestor réfléchit ou parle : l'alerte attend le tour suivant.
- **Un seul rapport par tour**, même avec plusieurs alertes : Nestor les
  mentionne naturellement, sans lire une liste.

## Traçabilité

Chaque alerte émise est publiée sur `/ws` :

```json
{"type": "alert", "id": 3, "kind": "mission_stalled", "text": "la mission 7 (…) n'a donne aucun signe d'activite depuis 12 minutes", "at_ms": 1760000000000}
```

L'interface web l'affiche dans le journal d'activité (étiquette « Alerte »). Les
20 dernières alertes font partie de l'instantané de connexion.

## Configuration

```toml
[proactive]
enabled = true
interval_secs = 60            # période d'évaluation (10 au minimum)
todo_interval_minutes = 5
mission_stall_minutes = 10
fallback_remind_minutes = 30
quota_threshold = 0.9
```

## Suite prévue

Les sources de `.agent/VISION.md` (position, agenda, mails) enrichiront
l'`Observation` que lisent les règles ; elles ne créeront pas une autre
boucle. Le choix du canal (poste ou téléphone hors appel) viendra avec le
transport de position.
