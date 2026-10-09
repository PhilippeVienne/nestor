# Sauvegarde chiffrée

Ce que Nestor sait a de la valeur : mémoire longue, tâches, passkeys, sessions,
configuration et secrets du bord. Une sauvegarde `restic` chiffrée les met à
l'abri chaque nuit.

## Ce qui est sauvegardé

- `~/.local/share/nestord` : `memory.db`, `todos.db` (les modèles audio,
  plusieurs Go téléchargeables, sont exclus) ;
- `~/.config/nestord` : `config.toml`, `ui-settings.toml`, `passkeys.json`,
  `sessions.json`, `auth_token`, `google_token.json`, `wake_header` ;
- `~/.config/nestor-edge` : jeton Cloudflare ;
- `~/.config/nestor-backup/env` : adresse du dépôt ;
- `~/.config/systemd/user` : les unités.

## Où et comment

- Dépôt : `~/Sauvegardes/nestor` par défaut, chiffré par restic avec le mot de
  passe de `~/.config/nestor-backup/password` (généré, droits 600). **Copiez ce
  mot de passe dans votre gestionnaire de mots de passe** : sans lui, la
  sauvegarde est illisible, et il n'est pas dans la sauvegarde.
- `deploy/backup/nestor-backup.sh` : sauvegarde, puis rétention 14 quotidiennes,
  8 hebdomadaires, 6 mensuelles, et écrit `~/.local/state/nestord/last-backup`,
  lu par le panneau Santé.
- `nestor-backup.timer` : chaque nuit à 3 h 30, rattrapé si la machine dormait.

```sh
systemctl --user list-timers nestor-backup.timer
deploy/backup/nestor-backup.sh          # à la main
set -a; . ~/.config/nestor-backup/env; set +a
restic snapshots                        # lister
restic restore latest --target /tmp/nestor-restaure   # restaurer ailleurs
restic check                            # vérifier le dépôt
```

## Restaurer

1. Arrêter le daemon : `systemctl --user stop nestord`.
2. `restic restore latest --target /` restaure en place, ou `--target /tmp/x`
   puis copie des fichiers voulus.
3. Relancer : `systemctl --user start nestord`.

## Hors de la machine

Un dépôt local protège d'une erreur, pas d'un disque mort. restic sait écrire
ailleurs sans changer le script : `RESTIC_REPOSITORY=rclone:remote:nestor`
(Google Drive, etc.), `s3:https://<compte>.r2.cloudflarestorage.com/nestor`
(Cloudflare R2, avec `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`), ou
`sftp:user@hote:/chemin` (une autre machine du tailnet). Mettre les variables
dans `~/.config/nestor-backup/env`, puis `restic init` une fois.
