# Rapport : appels d'outils affichés « en cours » après l'annulation d'une mission

Date : 2026-09-12
Rédigé par : Nestor (session vocale pilotée par nestord), à l'intention de l'agent Claude qui travaille sur le projet.

## Symptôme

Une mission déléguée via `start_mission` (mission #1, backend `claude`, audit en lecture seule du code de nestord) a été annulée avant la fin.
Après l'annulation, l'interface continue d'afficher comme « en cours » des appels d'outils `Bash` que le sous-agent avait lancés.
Ils ne passent jamais à l'état terminé ou annulé.

## Constats

- `list_missions` renvoie bien la mission #1 à l'état `annulee` (« mission annulee avant la fin »).
- `ps` ne montre plus aucun processus `claude` lié au sous-agent. Seuls restent :
  - `./nestord/target/debug/nestord` (PID 3460128)
  - la session vocale principale `claude -p --input-format stream-json ... --mcp-config /tmp/nestord-mcp.json` (PID 3460150, enfant de nestord)
  - les serveurs de dev `vite` (web) et `expo` (mobile)
- Les appels Bash « en cours » dans l'UI ne correspondent donc plus à aucun processus : le problème vient de l'état de l'interface, pas de processus orphelins.

## Hypothèse

Quand une mission est annulée, nestord arrête le sous-agent mais n'envoie pas à l'UI d'événement de fin pour les `tool_use` restés ouverts.
Le flux stream-json du sous-agent est coupé avant l'arrivée des `tool_result` correspondants, et l'UI garde ces entrées ouvertes indéfiniment.

## Pistes de correction

1. Dans nestord, au moment de l'annulation (ou de la fin anormale du process) d'une mission, parcourir les `tool_use` sans `tool_result` et émettre pour chacun un événement de clôture explicite, par exemple avec un statut `cancelled`.
2. Côté UI (web et mobile), quand une mission passe à l'état `annulee`, `terminee` ou `en echec`, marquer comme clos tous les appels d'outils encore ouverts de cette mission. C'est un filet de sécurité même si le point 1 est corrigé.
3. Ajouter un test : lancer une mission qui exécute une commande longue (par exemple `sleep 60`), l'annuler, puis vérifier qu'aucun appel d'outil ne reste « en cours » dans l'état envoyé à l'UI.

## À vérifier

- L'endroit de nestord où l'annulation est gérée (arrêt du process enfant) et la façon dont les événements de mission sont relayés à l'UI.
- Si le même problème touche aussi les missions qui échouent ou dont le process plante, pas seulement les annulations.
