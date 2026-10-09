# nestor.vienne.me : un nom, deux pages, et tout passe par Tailscale

Objectif : joindre Nestor par `https://nestor.vienne.me`, avec un vrai certificat,
en garantissant que **l'application et l'API ne sont atteignables que par le
tailnet**, tandis que depuis Internet le même nom ne montre qu'une page de
présentation sans lien avec la machine.

## Comment c'est garanti

- **Kanto n'est jamais joignable depuis Internet.** `nestord` n'écoute que sur
  la boucle locale (`127.0.0.1:8340` pour l'API, `127.0.0.1:8341` pour
  l'interface construite, option `ui_dir`). Traefik et dnsmasq n'écoutent que
  sur l'adresse Tailscale de la machine (`100.84.235.85`), jamais sur le réseau
  local ni sur une interface publique. Aucune redirection de port sur la box.
- **Depuis Internet**, `nestor.vienne.me` est un domaine personnalisé d'un
  Worker Cloudflare (`deploy/cloudflare-worker`) : Cloudflare répond lui-même
  avec la page publique, sans origine. Il n'existe aucun chemin vers kanto.
- dnsmasq est **autoritaire** sur `nestor.vienne.me` (`local=`) : il répond l'A
  Tailscale et rien pour l'AAAA. Sans cela, l'adresse IPv6 publiée par
  Cloudflare pour le domaine personnalisé du Worker était transmise, et un
  client préférant l'IPv6 arrivait sur la page publique malgré le tailnet.
- **Depuis le tailnet**, le DNS partagé de Tailscale envoie les noms de
  `vienne.me` à dnsmasq sur kanto, qui répond `100.84.235.85` pour
  `nestor.vienne.me` et transmet le reste à Cloudflare. Le navigateur arrive
  donc par le tunnel WireGuard sur Traefik, qui présente un certificat
  Let's Encrypt pour `nestor.vienne.me` obtenu par défi DNS-01 (jeton Cloudflare
  limité à la zone), et relaie vers nestord.
- **Seuls `/ws`, `/auth` et `/location` atteignent l'API.** `/mcp` (outils de
  l'assistant) et `/wake` (réveil) ne sont pas routés. `/ws` et `/auth`
  vérifient en plus l'en-tête `Origin` contre `allowed_origins`.
- Traefik ajoute HSTS, `X-Frame-Options: DENY`, `nosniff`, et limite l'API à
  5 requêtes par seconde et par adresse Tailscale (rafale de 20) : large pour
  l'usage, bloquant pour une énumération de jetons. Son journal d'accès est
  désactivé, l'URL de `/ws` portant le jeton de session.
- Les preuves d'accès restent celles de `docs/passkey.md` : passkey liée au
  domaine `nestor.vienne.me`, ou jeton de `nestord onboard` pour le téléphone.

Le nom MagicDNS `kanto.felis-ionian.ts.net` continue d'être servi par Traefik
sur le même port 443, vers le service qu'il publiait déjà, avec le certificat
fourni par Tailscale (`tailscale cert`, renouvelé chaque mois par un timer).

## Les pièces

| Pièce | Où | Rôle |
|---|---|---|
| Traefik | `deploy/edge/traefik`, conteneur `nestor-edge-traefik` | TLS et routage par nom, sur `100.84.235.85:443` |
| dnsmasq | `deploy/edge/dnsmasq`, conteneur `nestor-edge-dnsmasq` | DNS partagé du tailnet pour `vienne.me`, sur `100.84.235.85:53` |
| Worker | `deploy/cloudflare-worker` | page publique de `nestor.vienne.me` |
| nestord | `ui_dir`, `allowed_origins`, `ui_url` dans `config.toml` | API et interface construite, boucle locale |
| systemd | `deploy/systemd` | `nestord.service`, et le timer de renouvellement du certificat ts.net |

## Mise en place

Trois actions sont à faire par vous, le reste est prêt.

1. **Cloudflare, jeton d'API** : *My Profile > API Tokens > Create Token*,
   modèle « Edit zone DNS », zone `vienne.me` uniquement. Déposez-le dans
   `~/.config/nestor-edge/cloudflare.env` (fichier déjà créé, droits 600) :

   ```sh
   CF_DNS_API_TOKEN=<le jeton>
   ACME_EMAIL=<votre adresse, pour le compte Let's Encrypt>
   ```

2. **Cloudflare, page publique** :

   ```sh
   cd deploy/cloudflare-worker && npx wrangler login && npx wrangler deploy
   ```

   Le domaine personnalisé crée l'enregistrement DNS proxifié de
   `nestor.vienne.me`. Vérification depuis un réseau hors tailnet : la page
   « Nestor, majordome personnel » s'affiche, rien d'autre.

3. **Tailscale, DNS partagé** : console d'administration > *DNS* >
   *Nameservers* > *Add nameserver* > *Custom* : `100.84.235.85`, cocher
   *Restrict to domain* avec `vienne.me`. MagicDNS reste activé.

Puis, sur kanto (je m'en charge dès que le fichier d'environnement est rempli) :

```sh
tailscale serve --https=443 off          # Traefik reprend le 443
cd deploy/edge
tailscale cert --cert-file certs/kanto.felis-ionian.ts.net.crt --key-file certs/kanto.felis-ionian.ts.net.key kanto.felis-ionian.ts.net
./edge.sh up                             # Traefik + dnsmasq
systemctl --user enable --now nestor-edge-cert.timer   # unites copiees dans ~/.config/systemd/user
```

Enfin une passkey pour le nouveau domaine, depuis un navigateur du tailnet :
`nestord onboard --passkey` (l'adresse proposée est déjà `https://nestor.vienne.me`),
lien à ouvrir dans les dix minutes. Sur le téléphone : Tailscale actif, adresse
`wss://nestor.vienne.me/ws` (raccourci « Tailscale ») et jeton de `nestord onboard`.
L'application sonde `/auth/status` et dit sous le bouton d'appel par où elle
passe : « joignable », « hors du tailnet : activez Tailscale » (c'est la page
publique qui a répondu), ou « injoignable » ; l'appel et la veille affichent le
même diagnostic quand la connexion échoue.

## Vérifications

```sh
dig @100.84.235.85 nestor.vienne.me +short        # 100.84.235.85 (depuis le tailnet)
curl -sI https://nestor.vienne.me/ | head -1       # 200, interface (tailnet) ; page publique ailleurs
curl -s -H 'Origin: https://nestor.vienne.me' https://nestor.vienne.me/auth/status
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://nestor.vienne.me/mcp   # 404 : non routé
curl -sI https://kanto.felis-ionian.ts.net/ | head -1                             # l'autre service, inchangé
tailscale funnel status                            # rien : pas d'exposition publique par Tailscale
```

## Historique

La première publication, par `tailscale serve --https=8443` sur
`kanto.felis-ionian.ts.net:8443`, a été retirée le 2026-10-09 une fois
`nestor.vienne.me` validé ; `tailscale serve` n'est plus utilisé pour Nestor.

## nestord en service utilisateur

`deploy/systemd/nestord.service`, copié dans `~/.config/systemd/user/`, activé le
2026-10-09 avec `loginctl enable-linger` : le daemon démarre avec la session
utilisateur et survit à sa fermeture. Il lance le binaire **release**
(`cargo build --release --features full-audio`, puis `systemctl --user restart nestord`),
bien plus rapide que le debug sur la transcription. Le PATH du service inclut `~/.local/bin`
(`claude`, `uvx`) : sans lui, la session `claude` ne se lance pas et le service
redémarre en boucle. Journal : `journalctl --user -u nestord.service -f`.
