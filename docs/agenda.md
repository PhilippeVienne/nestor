# Agenda : Google Calendar en lecture directe

nestord lit l'agenda **lui-même** (`nestord/src/calendar.rs`), sans passer par la
session `claude` : la boucle proactive et, plus tard, le calcul du réveil ont
besoin du prochain rendez-vous sans dépenser de tour de conversation ni de
quota, et même quand la session est occupée ou morte. L'écriture (créer,
déplacer un rendez-vous) reste au connecteur Google Calendar de la session,
s'il est disponible sur le compte.

## Brancher l'agenda

1. Dans la console Google Cloud : activer l'API Google Calendar, puis créer un
   client OAuth de type **Application de bureau**. Récupérer l'identifiant
   client et le secret (fourni avec ce type de client).
2. Les renseigner dans `config.toml`, ou dans `NESTORD_GOOGLE_CLIENT_ID` /
   `NESTORD_GOOGLE_CLIENT_SECRET` (prioritaires) :

   ```toml
   [google]
   client_id = "….apps.googleusercontent.com"
   client_secret = "…"
   calendar_ids = ["primary"]   # ou des adresses d'agendas partagés
   poll_minutes = 5
   horizon_hours = 36
   ```

3. `nestord onboard --google` : le navigateur s'ouvre sur le consentement
   Google (portée `calendar.events.readonly`, lecture seule), le code revient
   sur la boucle locale (PKCE), et le jeton de rafraîchissement est écrit dans
   `~/.config/nestord/google_token.json` (droits 600). Redémarrer nestord.

Supprimer ce fichier débranche l'agenda. Le jeton d'accès est rafraîchi par
nestord ; rien n'est journalisé au-delà du nombre d'événements lus.

## Ce que nestord en fait

- **Contexte** : l'événement `context` porte `next_event` (prochain
  rendez-vous à heure fixe, avec lieu et indicateur visio), `calendar_connected`
  et `calendar_error`. Le panneau « Situation » les affiche.
- **`get_context`** (outil MCP) cite les trois prochains rendez-vous.
- **Boucle proactive** (`docs/proactive.md`), deux règles de plus :

| Règle | Déclencheur | Paliers |
|---|---|---|
| `departure` | rendez-vous avec un lieu : à `début − trajet − marge` (`default_travel_minutes` 30, `departure_margin_minutes` 10) | « il est temps de partir » ; rappel ferme 5 min plus tard si Monsieur est toujours dans un lieu reconnu |
| `event_imminent` | rendez-vous sans lieu ou visio, `event_reminder_minutes` (10) avant | une fois |

Le trajet est une **estimation prudente configurable**, pas un itinéraire
calculé : aucune API de trajet n'est appelée.

## Mails importants

Règle `mail_check_minutes` de `[proactive]` (0 par défaut, donc inactive) :
toutes les N minutes, nestord adresse à la session une consigne interne de
faire le point sur les mails non lus importants (Gmail est accessible à la
session par ses connecteurs). Nestor ne signale que ce qui mérite attention et
se tait s'il n'y a rien. Chaque point consomme un tour de conversation :
à activer en connaissance de cause.

## Hors appel

Le canal nestord → mobile hors appel est décrit dans `docs/canal-mobile.md` :
une alerte devient une notification sur le téléphone, un réveil le fait sonner.
