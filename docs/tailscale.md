# Nestor sur le tailnet : nom de domaine et TLS, sans rien exposer d'autre

Objectif : joindre Nestor depuis le téléphone ou un autre poste par un vrai nom
de domaine en HTTPS, en garantissant que **seul le tailnet** y accède.

## Ce qui garantit le passage par Tailscale

- `nestord` n'écoute que sur la boucle locale : `127.0.0.1:8340` (API) et
  `127.0.0.1:8341` (interface construite, option `ui_dir`). Aucune interface
  réseau ne porte Nestor.
- `tailscale serve` publie ces ports en HTTPS sur le nom MagicDNS de la machine
  (`kanto.felis-ionian.ts.net`), **tailnet only** : le certificat est émis par
  Let's Encrypt via Tailscale, le trafic entre par le tunnel WireGuard, rien
  n'est ouvert sur Internet (Funnel reste désactivé : `tailscale funnel status`).
- Seuls trois chemins de l'API sont montés : `/ws`, `/auth` et `/location`.
  `/mcp` (outils de l'assistant) et `/wake` (réveil) ne sont pas publiés : une
  requête vers eux tombe sur le serveur statique, qui répond 405 ou 404.
- `/ws` et `/auth` vérifient l'en-tête `Origin` : seule l'origine déclarée dans
  `allowed_origins` passe, en plus de la machine locale.
- Les preuves d'accès restent celles de `docs/passkey.md` : passkey liée au
  domaine Tailscale, ou jeton de `nestord onboard` pour l'application mobile.

## Mise en place (faite sur kanto le 2026-10-09)

1. `config.toml` :

   ```toml
   allowed_origins = ["https://kanto.felis-ionian.ts.net:8443"]
   ui_url = "https://kanto.felis-ionian.ts.net:8443"
   ui_dir = "~/github.com/PhilippeVienne/nestor/web/dist"   # npm run build dans web/
   ```

2. Interface construite : `cd web && npm run build`. La page servie en HTTPS
   parle au daemon par sa propre origine (`wss://<hôte>/ws`), sans variable
   d'environnement.

3. Publication, persistante au redémarrage (le port 443 reste à son usage
   actuel ; Nestor prend 8443) :

   ```sh
   tailscale serve --bg --https=8443 --set-path=/         http://127.0.0.1:8341
   tailscale serve --bg --https=8443 --set-path=/ws       http://127.0.0.1:8340/ws
   tailscale serve --bg --https=8443 --set-path=/auth     http://127.0.0.1:8340/auth
   tailscale serve --bg --https=8443 --set-path=/location http://127.0.0.1:8340/location
   tailscale serve status
   ```

   Le chemin cible doit répéter le chemin monté : sans lui, Tailscale le
   retire avant de relayer. Servir un dossier directement par `tailscale serve`
   exigerait root, d'où le second port de nestord.

4. Passkey pour ce domaine (une passkey est liée à un nom d'hôte, celle de
   `localhost` ne sert pas ici) :

   ```sh
   nestord onboard --passkey        # ui_url pointe déjà sur l'adresse Tailscale
   ```

   Ouvrir le lien affiché, dans les dix minutes, depuis un navigateur connecté
   au tailnet.

5. Téléphone : Tailscale installé et connecté, puis dans l'application le
   raccourci « Tailscale » (`wss://kanto.felis-ionian.ts.net:8443/ws`) et le
   jeton de `nestord onboard`. Le partage de position et le canal hors appel
   passent par la même adresse.

## Vérifications

```sh
curl -H 'Origin: https://kanto.felis-ionian.ts.net:8443' https://kanto.felis-ionian.ts.net:8443/auth/status
# {"auth_required":true,"domain_ok":true,"passkeys":…,"rp_id":"kanto.felis-ionian.ts.net"}
curl -X POST https://kanto.felis-ionian.ts.net:8443/mcp     # 405 : non publié
```

## Reste à faire

- Lancer `nestord` comme service utilisateur systemd (avec
  `LD_LIBRARY_PATH` des bibliothèques CUDA 13, cf. `nestord/README.md`), pour
  qu'il survive à une fermeture de session.
- Un nom de domaine personnel (`nestor.exemple.fr`) à la place du nom MagicDNS
  demanderait un certificat obtenu par DNS-01 et un mandataire lié à l'IP
  Tailscale (100.x) : possible, mais rien de plus que ce que Tailscale offre déjà.
