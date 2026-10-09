#!/bin/sh
# Renouvelle le certificat ts.net de la machine (Tailscale, Let's Encrypt, 90 jours)
# et relance Traefik pour qu'il le recharge. Lance par le timer nestor-edge-cert.
set -eu
cd "$(dirname "$0")"
tailscale cert --cert-file certs/kanto.felis-ionian.ts.net.crt --key-file certs/kanto.felis-ionian.ts.net.key kanto.felis-ionian.ts.net
docker compose --env-file "$HOME/.config/nestor-edge/cloudflare.env" restart traefik
