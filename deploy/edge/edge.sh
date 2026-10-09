#!/bin/sh
# Raccourci : ./edge.sh up | down | logs | status
set -eu
cd "$(dirname "$0")"
ENV_FILE="$HOME/.config/nestor-edge/cloudflare.env"
[ -f "$ENV_FILE" ] || { echo "fichier d'environnement absent : $ENV_FILE (voir docs/tailscale.md)" >&2; exit 1; }
case "${1:-status}" in
  up)     docker compose --env-file "$ENV_FILE" up -d --build ;;
  down)   docker compose --env-file "$ENV_FILE" down ;;
  logs)   docker compose --env-file "$ENV_FILE" logs -f --tail=100 ;;
  status) docker compose --env-file "$ENV_FILE" ps ;;
  *) echo "usage : $0 up|down|logs|status" >&2; exit 2 ;;
esac
