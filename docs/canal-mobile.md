# Canal nestord → téléphone hors appel

Jusqu'ici le téléphone n'était relié à nestord que pendant un appel. Ce canal
permet à Nestor de **prévenir** Monsieur hors appel (alerte en notification) et
de le **réveiller** (le téléphone sonne comme pour un appel entrant).

## Choix du transport

Un WebSocket léger maintenu par un service de premier plan de l'application
(`NestorStandbyService.kt`), plutôt qu'un service de notifications push :

- aucun compte Google ni serveur tiers, cohérent avec le reste du projet ;
- la machine est déjà jointe par le réseau privé (Tailscale) ;
- le prix : une connexion maintenue et une notification permanente « Nestor en
  veille ». Un ping toutes les 60 s, reconnexion à délai croissant (5 s à
  5 min). Android peut la couper en économie d'énergie agressive ; exempter
  l'application de l'optimisation de batterie si les alertes se perdent.

## Côté daemon

- Le téléphone se connecte à `/ws?client=standby&token=…`. Le daemon ne lui
  envoie que `alert`, `context`, `mission` et `backend_status` : jamais
  l'audio ni les transcriptions. Pas de rappel de tâches à la connexion.
- Il apparaît dans le panneau « Appareils » comme « Mobile en veille » et ne
  compte pas comme un appel pour le verrou de veille.
- Quand seul un téléphone en veille est connecté, la boucle proactive émet ses
  alertes comme événements (notification) sans réinjecter de rapport dans la
  conversation, puisque personne n'entendrait Nestor. Les tâches dues et le
  point mails attendent un client qui écoute.

## Côté téléphone

Interrupteur « Rester joignable hors appel » sur l'écran d'accueil (préférence
mémorisée, service relancé à l'ouverture de l'application).

- Une alerte devient une notification (canal « Alertes de Nestor »), titrée
  selon son type : départ, rendez-vous imminent, mission, quota, tâches.
- Une alerte de réveil (`kind == "wake"`) fait **sonner** le téléphone : le
  service signale un appel entrant à l'API Telecom (`addNewIncomingCall`), et
  comme le compte est autogéré, l'application affiche elle-même l'interface
  d'appel entrant (`NestorIncomingCall.kt`) : notification de style appel,
  sonnerie par défaut, vibration, plein écran, boutons Répondre / Refuser.
  Répondre ouvre un vrai appel avec Nestor, qui annonce alors la journée (son
  annonce attendait un client qui écoute) ; l'application passe au premier
  plan et retrouve l'appel en cours même si son JavaScript avait été déchargé.

## Pas encore fait

- Relance du service au démarrage du téléphone (`BOOT_COMPLETED`) : il faut
  pour cela mémoriser l'URL et le jeton côté natif, hors du stockage sécurisé
  Expo. Aujourd'hui, ouvrir l'application suffit à le relancer.
- Validation sur appareil : sonnerie, réponse depuis l'écran verrouillé,
  survie du service en économie d'énergie.
