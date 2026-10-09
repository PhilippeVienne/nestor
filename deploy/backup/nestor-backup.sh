#!/bin/sh
# Sauvegarde chiffree de ce que Nestor sait et de ce qui ouvre son acces :
# memoire longue, taches, passkeys et sessions, configuration, secrets du bord.
# Les modeles audio (plusieurs Go, telechargeables) sont exclus.
#
# Depots (RESTIC_REPOSITORIES, separes par des espaces) et mot de passe :
# ~/.config/nestor-backup/env (cf. docs/sauvegarde.md). Chaque depot est traite ;
# un echec sur l'un n'empeche pas les autres, l'horodatage n'est ecrit que si
# au moins un a reussi, et le code de retour est non nul si l'un a echoue.
# Lance chaque jour par nestor-backup.timer ; a la main : deploy/backup/nestor-backup.sh
set -u
ENV_FILE="$HOME/.config/nestor-backup/env"
[ -f "$ENV_FILE" ] || { echo "fichier d'environnement absent : $ENV_FILE" >&2; exit 1; }
set -a; . "$ENV_FILE"; set +a
STAMP_DIR="$HOME/.local/state/nestord"
mkdir -p "$STAMP_DIR"
REPOS="${RESTIC_REPOSITORIES:-${RESTIC_REPOSITORY:-}}"
[ -n "$REPOS" ] || { echo "aucun depot : RESTIC_REPOSITORIES vide" >&2; exit 1; }

ok=0; failed=0
for repo in $REPOS; do
  echo "== $repo"
  export RESTIC_REPOSITORY="$repo"
  # Premier passage : creation du depot.
  if ! restic cat config >/dev/null 2>&1; then
    restic init || { echo "   creation impossible" >&2; failed=$((failed+1)); continue; }
  fi
  if restic backup --quiet \
      --tag nestor \
      --exclude "$HOME/.local/share/nestord/models" \
      "$HOME/.local/share/nestord" \
      "$HOME/.config/nestord" \
      "$HOME/.config/nestor-edge" \
      "$HOME/.config/nestor-backup/env" \
      "$HOME/.config/systemd/user" \
    && restic forget --quiet --tag nestor --keep-daily 14 --keep-weekly 8 --keep-monthly 6 --prune; then
    ok=$((ok+1))
  else
    echo "   echec" >&2; failed=$((failed+1))
  fi
done

if [ "$ok" -gt 0 ]; then
  # Horodatage lu par le panneau Sante de l'interface.
  date -u +%Y-%m-%dT%H:%M:%SZ > "$STAMP_DIR/last-backup"
fi
echo "sauvegarde : $ok depot(s) a jour, $failed en echec"
[ "$failed" -eq 0 ]
