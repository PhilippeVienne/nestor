#!/bin/sh
# Sauvegarde chiffree de ce que Nestor sait et de ce qui ouvre son acces :
# memoire longue, taches, passkeys et sessions, configuration, secrets du bord.
# Les modeles audio (plusieurs Go, telechargeables) sont exclus.
#
# Depot et mot de passe : ~/.config/nestor-backup/env (cf. docs/sauvegarde.md).
# Lance chaque jour par nestor-backup.timer ; a la main : deploy/backup/nestor-backup.sh
set -eu
ENV_FILE="$HOME/.config/nestor-backup/env"
[ -f "$ENV_FILE" ] || { echo "fichier d'environnement absent : $ENV_FILE" >&2; exit 1; }
set -a; . "$ENV_FILE"; set +a
STAMP_DIR="$HOME/.local/state/nestord"
mkdir -p "$STAMP_DIR"

# Premier passage : creation du depot.
if ! restic cat config >/dev/null 2>&1; then
  restic init
fi

restic backup \
  --tag nestor \
  --exclude "$HOME/.local/share/nestord/models" \
  "$HOME/.local/share/nestord" \
  "$HOME/.config/nestord" \
  "$HOME/.config/nestor-edge" \
  "$HOME/.config/nestor-backup/env" \
  "$HOME/.config/systemd/user"

# Retention : deux semaines de quotidiennes, deux mois d'hebdomadaires, six mois de mensuelles.
restic forget --tag nestor --keep-daily 14 --keep-weekly 8 --keep-monthly 6 --prune

# Horodatage lu par le panneau Sante de l'interface.
date -u +%Y-%m-%dT%H:%M:%SZ > "$STAMP_DIR/last-backup"
echo "sauvegarde terminee : $(cat "$STAMP_DIR/last-backup")"
