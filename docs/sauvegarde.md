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

- Trois dépôts, listés dans `RESTIC_REPOSITORIES` de `~/.config/nestor-backup/env`,
  tous chiffrés avec le mot de passe de `~/.config/nestor-backup/password`
  (généré, droits 600) :
  - `~/Sauvegardes/nestor`, local, contre une fausse manipulation ;
  - `rclone:nas:perso/Sauvegardes/nestor`, le NAS coruscant par SMB, contre un
    disque mort ;
  - `rclone:gdrive:Sauvegardes/nestor`, Google Drive, hors site.
  **Copiez ce mot de passe dans votre gestionnaire de mots de passe** : sans
  lui, les sauvegardes sont illisibles, et il n'est dans aucune d'elles.
- `deploy/backup/nestor-backup.sh` traite chaque dépôt : création au premier
  passage, sauvegarde, rétention 14 quotidiennes, 8 hebdomadaires, 6 mensuelles.
  Un dépôt en échec n'empêche pas les autres ; l'horodatage
  `~/.local/state/nestord/last-backup`, lu par le panneau Santé, n'est écrit que
  si au moins un a réussi, et le script sort en erreur si l'un a échoué.
- `nestor-backup.timer` : chaque nuit à 3 h 30, rattrapé si la machine dormait.

```sh
systemctl --user list-timers nestor-backup.timer
deploy/backup/nestor-backup.sh          # à la main
set -a; . ~/.config/nestor-backup/env; set +a
export RESTIC_REPOSITORY=rclone:nas:perso/Sauvegardes/nestor   # ou un autre de la liste
restic snapshots                        # lister
restic restore latest --target /tmp/nestor-restaure   # restaurer ailleurs
restic check                            # vérifier le dépôt
```

## Restaurer

1. Arrêter le daemon : `systemctl --user stop nestord`.
2. `restic restore latest --target /` restaure en place, ou `--target /tmp/x`
   puis copie des fichiers voulus.
3. Relancer : `systemctl --user start nestord`.

## Ajouter un dépôt

Ajouter son adresse à `RESTIC_REPOSITORIES` suffit, le script le crée au passage
suivant : un autre remote rclone, `s3:https://<compte>.r2.cloudflarestorage.com/nestor`
(Cloudflare R2, avec `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`), ou
`sftp:user@hote:/chemin` (une autre machine du tailnet). Google Drive limite le
débit et renvoie parfois des erreurs 500 passagères : restic réessaie seul.
