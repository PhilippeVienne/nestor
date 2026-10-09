# Connexion de l'interface web par passkey

L'interface web s'authentifie auprès de nestord avec une passkey (WebAuthn) :
une clé créée par votre navigateur ou votre appareil, dont la partie privée ne
quitte jamais l'authentificateur. nestord n'en garde que la clé publique.

## Enrôler un navigateur

```bash
nestord onboard --passkey                      # interface sur http://localhost:5173
nestord onboard --passkey --ui https://kanto.exemple.ts.net:8443   # autre adresse
```

La commande affiche un lien valable 10 minutes et utilisable une seule fois
(`…/?enroll=<code>`). Ouvrez-le dans le navigateur à équiper : l'écran
« Enregistrer une passkey » propose « Créer la passkey », puis le navigateur
affiche son invite habituelle. nestord
ne conserve du code que son empreinte, dans `~/.config/nestord/enroll_code`.

Dès qu'une passkey est enregistrée, **`/ws` exige une authentification** : une
session ouverte par passkey, ou le jeton de `nestord onboard` (application
mobile). Le daemon n'a pas besoin d'être redémarré, sauf pour démarrer des
connecteurs externes déclarés avant l'enrôlement.

## Se connecter

À l'ouverture de l'interface, si le daemon demande une authentification, un
écran de connexion précède l'application et propose « Se connecter » : la
passkey signe un défi, nestord vérifie la signature et délivre un jeton de
session. L'interface n'ouvre `/ws` qu'ensuite. Le jeton de `nestord onboard`
reste accepté, par le lien « Utiliser un jeton d'accès » du même écran.

- La session vaut pour l'onglet (elle survit à un rechargement, pas à la
  fermeture de l'onglet) et 7 jours au plus.
- Les sessions survivent à un redémarrage de nestord : `~/.config/nestord/sessions.json`
  garde l'empreinte de chaque jeton de session et son expiration, jamais le jeton.
- « Se déconnecter » (Réglages > Accès au daemon) ferme aussi la session côté
  daemon : son jeton n'ouvre plus rien, même s'il a été copié.

## Contraintes

- **Nom de domaine obligatoire.** Une passkey est liée au domaine de la page :
  ouvrez l'interface sur `http://localhost:5173`, pas sur `http://127.0.0.1:5173`.
  Hors de la machine, il faut une adresse en HTTPS, déclarée dans
  `allowed_origins` de `config.toml`. Une passkey créée sur `localhost` ne sert
  pas sur l'adresse Tailscale, et inversement : enrôlez chaque adresse.
- Les passkeys sont dans `~/.config/nestord/passkeys.json` (clés publiques,
  droits 600). Supprimer ce fichier retire toutes les passkeys.
- Réglages > Accès au daemon liste les passkeys (domaine, date de création) et
  permet d'en révoquer une (deux clics : « Révoquer » puis « Confirmer »). Les
  sessions déjà ouvertes avec elle restent valables jusqu'à leur expiration ou
  une déconnexion. Si c'était la dernière et qu'aucun jeton d'accès n'est
  configuré, `/ws` redevient libre.

## Points d'accès

Appelés par la page (CORS limité aux origines acceptées par `/ws`) :
`GET /auth/status`, `POST /auth/register/options` (`{code}`),
`POST /auth/register/finish` (`{id, code, credential}`),
`POST /auth/login/options`, `POST /auth/login/finish` (`{id, credential}`),
`POST /auth/logout` (`{token}`), `POST /auth/passkeys` (`{token}`, liste) et
`POST /auth/passkeys/revoke` (`{token, id}`). Ces deux derniers exigent une
preuve d'accès valable (session par passkey ou jeton d'accès).
Les deux `finish` renvoient `{token}`, à passer à `/ws?token=…`.

Vérification WebAuthn : bibliothèque `webauthn-rs` (`nestord/src/passkey.rs`).
