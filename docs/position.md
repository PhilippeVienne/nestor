# Position : savoir où est Monsieur

L'appli mobile peut envoyer sa position à nestord **hors appel**, en arrière-plan.
nestord n'en garde que le **lieu reconnu** (`domicile`, `bureau`…) : la position
elle-même n'est ni conservée ni journalisée. Le lieu alimente le contexte
(panneau « Situation », outil MCP `get_context`) et, plus tard, la boucle
proactive et la mise en veille.

## Côté daemon

- `POST /location`, corps JSON `{ "lat", "lon", "accuracy"?, "timestamp"? }`.
  Preuve d'accès : en-tête `Authorization: Bearer <jeton>` (jeton de
  `nestord onboard` ou session par passkey), ou champ `token` dans le corps.
  Réponse : `{ "place": "domicile" }` ou `{ "place": null }`.
- `ClientEvent::Location` sur `/ws` reste accepté pendant un appel ou depuis
  l'interface web ; les deux chemins passent par `location::apply`.
- Les lieux sont déclarés dans `config.toml` :

```toml
[[places]]
name = "domicile"
lat = 48.8566
lon = 2.3522
radius_m = 300
```

- Le contexte n'est rediffusé que lorsque le lieu change. Le journal ne
  mentionne jamais les coordonnées, seulement le lieu reconnu.

## Côté appli mobile

Interrupteur « Partager ma position avec Nestor » sur l'écran d'accueil.

- `expo-location` + `expo-task-manager` (Expo SDK 57). La tâche
  `nestor-location-sharing` est définie dans la portée globale du bundle
  (`index.ts` importe `src/location/sharing.ts` avant l'application), comme
  l'exige `TaskManager.defineTask`.
- Sobriété : précision `Balanced` (~100 m), mise à jour tous les 200 m ou
  toutes les 5 minutes, pas de GPS continu. Un service de premier plan avec
  notification « Nestor suit votre position » est imposé par Android.
- Permissions : premier plan, puis arrière-plan (« Toujours autoriser » dans
  les réglages Android 11+). Un refus est expliqué sous l'interrupteur.
- L'URL et le jeton mémorisés (`SecureStore`) servent aussi à la tâche de fond :
  l'interrupteur les enregistre avant de lancer le suivi.
- Le dossier `android/` étant généré, les permissions
  `ACCESS_BACKGROUND_LOCATION` et `FOREGROUND_SERVICE_LOCATION` sont ajoutées
  dans `AndroidManifest.xml` en plus du plugin `expo-location` de `app.json`
  (qui ne joue qu'à un `npx expo prebuild`).

## Accès distant

nestord n'écoute que sur `127.0.0.1`. Hors du domicile, le téléphone l'atteint
par un réseau privé : l'application Tailscale du système suffit (adresse en
HTTPS déclarée dans `allowed_origins` si l'interface web est servie par là).
La bibliothèque `libtailscale` compilée dans `mobile/native/tailscale` reste
un prototype non intégré ; elle n'est pas nécessaire.
