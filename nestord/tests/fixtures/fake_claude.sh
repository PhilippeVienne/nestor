#!/bin/sh
# Sous-agent factice pour les tests de `mission.rs` : annonce, dans le format
# stream-json du CLI `claude`, un appel d'outil Bash qui ne se termine jamais,
# puis bloque comme le ferait la commande. nestord le tue a l'annulation.
printf '%s\n' '{"type":"system","subtype":"init"}'
printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"Je commence par lister les fichiers."},{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"sleep 60"}}]}}'
exec sleep 60
