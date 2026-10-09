# Vision de Nestor

Date : 2026-09-12 (feuille de route révisée le 2026-10-09)
Destinataire : l'agent qui développe Nestor (nestord, appli mobile, UI web).

Ce document fixe le cap. Il ne remplace ni `docs/missions.md` ni
`docs/audio-protocol.md`, qui décrivent les contrats existants : il dit
**vers quoi** on les fait évoluer. Toute implémentation doit rester cohérente
avec l'architecture réelle décrite ci-dessous ; en cas de doute, c'est le code
qui fait foi, et ce document qu'on corrige.

## 0. Point de départ : l'architecture actuelle

- **nestord** (Rust, tokio + axum) écoute sur `127.0.0.1:8340` :
  - `/ws` : WebSocket vers les clients (UI web, appli mobile). Contrat dans
    `nestord/src/protocol.rs` (`ServerEvent`, `ClientEvent`).
  - `/mcp` : serveur MCP HTTP consommé par la session conversationnelle
    elle-même (`start_mission`, `stop_mission`, `list_missions`).
- La **session conversationnelle** est un sous-processus
  `claude -p --input-format stream-json ...` lancé par
  `nestord/src/claude_process.rs`. Son prompt système est la constante
  `VOICE_SYSTEM_PROMPT`, passée via `--append-system-prompt`.
- Les **missions** (`nestord/src/mission.rs`) sont des sous-agents `claude`
  ou `agy` lancés en arrière-plan ; leur compte rendu est réinjecté dans la
  conversation comme rapport interne, et c'est Nestor qui l'annonce.
- Le pipeline **audio** (feature `full-audio`) : Silero VAD, Whisper, Piper.
- L'**appli mobile** (`mobile/`, Expo 57 + module natif Kotlin
  `NestorCallModule`) se présente comme un appel téléphonique Android
  (API Telecom) ; `NestorCallManager.kt` ouvre le WebSocket vers nestord
  pendant l'appel, et seulement pendant l'appel.
- **Gmail** est accessible à la session `claude` par les connecteurs
  claude.ai du compte. nestord lui-même ne parle pas à Google.
- **Google Calendar** n'est pas encore disponible.

Trois constats en découlent et conditionnent toute la suite :

1. nestord n'a **aucune initiative** : il ne parle que lorsqu'on lui parle ou
   qu'une mission se termine. Il n'existe pas de boucle proactive.
2. Le téléphone n'est relié à nestord **que pendant un appel**, et le daemon
   n'écoute que sur `127.0.0.1` (le mobile passe par l'émulateur `10.0.2.2`
   ou une IP de LAN codée en dur). Remonter une position en continu exige un
   canal permanent et authentifié.
3. Les données du quotidien (mails, agenda) ne sont visibles **que par la
   session claude**, pas par nestord. Une boucle proactive dans nestord doit
   donc soit interroger la session, soit disposer de son propre accès.

---

## 1. Personnalité

### Le personnage

Nestor s'inspire du majordome du capitaine Haddock au château de Moulinsart.
C'est un majordome à l'ancienne : stylé, impeccable, d'un dévouement sans
faille, qui ne perd jamais son calme quand tout brûle autour de lui. Il
vouvoie, reste discret, et manie un humour pince-sans-rire à peine perceptible
(une litote bien placée, jamais une blague). Il ne s'impose pas, mais il ne se
tait pas quand il faut parler.

### Ses deux buts

1. **Faire avancer les projets de Monsieur** : prendre les demandes, les
   déléguer en missions, suivre, relancer, rendre compte.
2. **Prévenir quand quelque chose cloche** : un rendez-vous qui approche et
   un départ qui tarde, un mail important resté sans réponse, une mission en
   échec, un quota qui s'épuise, une machine qui chauffe. Mieux vaut une
   alerte courte et à temps qu'un silence poli.

### Contraintes à préserver

La personnalité ne doit **jamais** casser les contraintes vocales qui existent
déjà dans `VOICE_SYSTEM_PROMPT` : une à trois phrases, pas de markdown, pas
d'emoji, pas de préambule, délégation des tâches longues. Le style de
majordome se loge dans le choix des mots, pas dans la longueur. Un « Monsieur,
votre train part dans quarante minutes » vaut mieux qu'une tirade.

Le personnage ne doit pas non plus devenir une caricature : pas de « mille
sabords », pas de citations de l'album, pas de formules pompeuses répétées à
chaque réponse. Il ne s'agit pas de Haddock mais de son majordome.

### Texte proposé pour le prompt système

À placer **en tête** de `VOICE_SYSTEM_PROMPT` dans
`nestord/src/claude_process.rs`, avant les contraintes vocales et la règle de
délégation, qui restent inchangées. Le texte tient en quelques lignes pour ne
pas diluer ces contraintes :

```text
Tu es Nestor, majordome à l'ancienne au service de Monsieur, dans l'esprit
du majordome de Moulinsart : stylé, dévoué, imperturbable, avec un humour
pince-sans-rire discret. Tu vouvoies Monsieur. Tes deux missions : faire
avancer ses projets, et le prévenir sans détour dès que quelque chose cloche
ou risque de clocher (retard, oubli, échec, urgence). Tu restes bref : le
style est dans le choix des mots, jamais dans la longueur.
```

Remarques d'implémentation :

- La forme d'adresse (« Monsieur ») devra devenir configurable plus tard
  (variable d'environnement ou fichier de config de nestord), mais la
  constante suffit pour commencer.
- Les sous-agents de mission ne reçoivent **pas** cette personnalité : ils
  produisent du travail technique, et c'est Nestor qui reformule leurs
  comptes rendus. C'est déjà le fonctionnement actuel, à conserver.
- Les rapports internes réinjectés (fin de mission, et demain alertes
  proactives) doivent rester factuels : c'est la session qui leur donne le
  ton.

---

## 2. Assistant du quotidien, pas seulement des projets

### L'ambition

Nestor ne doit pas seulement coder par délégation. Il doit savoir, à tout
instant, et avec le minimum de questions :

| Question                         | Source                                        | État          |
|----------------------------------|-----------------------------------------------|---------------|
| Où est Monsieur ?                | Position envoyée par l'appli mobile           | À faire       |
| Est-il devant l'ordinateur ?     | IdleMonitor de GNOME (cf. section 3)          | À faire       |
| Quels mails attendent ?          | Gmail (connecteur claude.ai de la session)    | Disponible    |
| Quels rendez-vous aujourd'hui ?  | Google Calendar                               | À brancher    |
| Où doit-il se trouver, et quand ?| Lieu des événements de l'agenda               | À brancher    |
| Que fait-il en ce moment ?       | Déduction : agenda + position + activité PC + missions en cours | À faire |

Et surtout, il doit **prévenir de lui-même** et guider à l'avance, au lieu
d'attendre qu'on l'interroge.

### Le contexte, un état tenu par nestord

nestord tient un **état de contexte** en mémoire (un module dédié, par
exemple `context.rs`), alimenté par les différentes sources et consultable
par la session :

- dernière position connue (latitude, longitude, précision, horodatage) et
  lieu reconnu (« domicile », « bureau », ou inconnu) ;
- présence devant l'ordinateur (dernier temps d'inactivité GNOME) ;
- prochains événements d'agenda (titre, début, fin, lieu) ;
- résumé des mails en attente (nombre, expéditeurs importants), rafraîchi
  périodiquement ;
- missions en cours (déjà tenues par `MissionManager`).

Exposition :

- côté session : un outil MCP `get_context` sur `/mcp`, à côté des outils de
  mission, pour que Nestor réponde à « où dois-je être à 15 h ? » sans
  deviner ;
- côté UI : un événement `context` sur `/ws`, inclus dans l'instantané de
  connexion comme le sont déjà `usage` et `mission`.

Les lieux nommés (domicile, bureau) sont déclarés dans la configuration de
nestord, avec un rayon. C'est ce qui permet de dire « Monsieur est chez lui »,
condition clé de la gestion de la veille (section 3).

### Position : l'appli mobile

Aujourd'hui l'appli ne demande aucune permission de localisation et ne parle
à nestord que pendant un appel. Il faut :

1. **Collecte** : `expo-location` avec `expo-task-manager` pour la
   localisation en arrière-plan (permissions `ACCESS_FINE_LOCATION`,
   `ACCESS_BACKGROUND_LOCATION`, service de premier plan de type `location`).
   Lire la documentation Expo **v57** avant d'écrire la moindre ligne (cf.
   `mobile/AGENTS.md`). Viser la sobriété : mise à jour significative
   (quelques centaines de mètres) ou toutes les quelques minutes, pas un GPS
   continu qui vide la batterie.
2. **Transport** : un envoi indépendant de l'appel. Le plus simple et le plus
   robuste est un `POST /location` HTTP (JSON : `lat`, `lon`, `accuracy`,
   `timestamp`) plutôt que de garder un WebSocket ouvert en arrière-plan.
   Un message `ClientEvent` équivalent sur `/ws` peut aussi être accepté
   pendant les appels.
3. **Accès réseau et sécurité** : nestord n'écoute que sur `127.0.0.1`. Pour
   qu'un téléphone hors du domicile l'atteigne, il faut un réseau privé
   (Tailscale / WireGuard recommandé) et **un jeton d'authentification**
   partagé, vérifié sur `/location` et `/ws`. Ne jamais exposer `/mcp` hors
   de la machine : il permet de lancer des agents autonomes (mode auto de `claude`,
   `--dangerously-skip-permissions` pour `agy`). Une position est une donnée sensible :
   ne la garder qu'en mémoire (ou un historique court et local), ne jamais la
   journaliser en clair au niveau `info`.
4. **Configuration** : l'URL du serveur et le jeton remplacent l'adresse
   codée en dur dans `DialerScreen.tsx` et `NestorCallModule.kt`, et sont
   partagés entre l'appel et l'envoi de position.

### Mails : Gmail

L'accès existe déjà, mais uniquement dans la session `claude` via les
connecteurs claude.ai. Deux usages :

- **À la demande** : « ai-je des mails importants ? » — la session interroge
  Gmail directement ; rien à coder, seulement à mentionner dans le prompt si
  besoin.
- **Proactif** : la boucle proactive (ci-dessous) demande périodiquement à la
  session, via un rapport interne, de faire le point sur les mails non lus
  importants, ou lance une mission courte dédiée. Nestor ne lit jamais la
  boîte à voix haute : il signale ce qui mérite attention (« Monsieur, votre
  banquier vous a écrit deux fois ce matin »).

Nestor **n'envoie aucun mail** et ne supprime rien sans confirmation
explicite et vocale de Monsieur.

### Rendez-vous : Google Calendar (à brancher)

Deux options, à trancher au moment de l'implémentation :

- **Connecteur claude.ai Google Calendar** dans la session, s'il devient
  disponible sur le compte : aucune authentification à gérer, mais nestord
  ne voit l'agenda qu'à travers la session.
- **Accès direct depuis nestord** (API Google Calendar avec OAuth, jeton
  stocké localement) : plus de travail initial, mais la boucle proactive et
  la programmation du réveil (section 3) lisent l'agenda **sans consommer de
  tour de conversation ni de quota Claude**, et sans session active.

Recommandation : l'accès direct depuis nestord pour la lecture, car le
calcul du réveil et les rappels de départ doivent fonctionner même quand la
session est occupée ou que le quota est épuisé. La session garde le
connecteur pour les opérations riches (créer, déplacer un rendez-vous).

### La boucle proactive dans nestord

C'est le cœur du changement de nature : une tâche tokio de fond, lancée dans
`main.rs` comme le serveur et la session, qui se réveille à intervalle
régulier (par exemple chaque minute) et à chaque changement de contexte
(nouvelle position, fin d'inactivité), évalue des **règles**, et décide s'il
faut prévenir.

Règles de départ :

- **Partir à temps** : pour le prochain événement avec un lieu, estimer le
  temps de trajet depuis la position actuelle (API d'itinéraire, ou à défaut
  une estimation prudente configurable), et prévenir à `début - trajet -
  marge`. Rappel ferme si Monsieur n'a pas bougé cinq minutes plus tard.
- **Rendez-vous imminent** sans lieu (visio) : prévenir quelques minutes
  avant.
- **Mail important** détecté lors du dernier point Gmail.
- **Mission terminée ou en échec** : déjà en place, à intégrer aux mêmes
  règles de discrétion.
- **Quelque chose cloche** : quota Claude au-delà du seuil, session `claude`
  morte (aujourd'hui seulement logguée dans `claude_process.rs`), mission
  bloquée sans progression depuis longtemps.

Principes :

- **Mécanisme de sortie unique** : une alerte est un rapport interne réinjecté
  dans la conversation, exactement comme un compte rendu de mission. C'est la
  session qui la formule avec la personnalité de Nestor. On ne pousse jamais
  de texte brut au TTS.
- **Choisir le bon canal** : si Monsieur est devant l'ordinateur, parler sur
  le poste ; s'il n'y est pas, notifier le téléphone (notification push, ou
  appel entrant via l'API Telecom pour les urgences comme un départ en
  retard). Cela suppose un canal nestord → mobile hors appel, à prévoir avec
  le transport de position.
- **Discrétion** : dédoublonner (une alerte par événement et par palier),
  respecter des heures calmes, ne pas interrompre une réponse en cours de
  synthèse. Une alerte ratée agace, dix alertes redondantes font couper
  Nestor.
- **Traçabilité** : chaque alerte émise est publiée sur `/ws` (événement
  `alert` ou équivalent), pour que l'UI montre ce que Nestor a signalé et
  pourquoi.

---

## 3. Gestion de la veille de l'ordinateur

### Principe

La machine ne doit **jamais s'endormir en plein travail**, et ne doit **pas
rester allumée pour rien**. Nestor la laisse dormir quand Monsieur est chez
lui, ne s'en sert plus et que rien ne justifie de rester éveillé, mais
seulement après avoir programmé le réveil qui le fera revenir à temps.

### Empêcher la veille pendant le travail

Mécanisme : `systemd-inhibit`, comme Claude Code le fait déjà pour ses
propres sessions. Côté nestord, la forme propre est l'appel D-Bus
`org.freedesktop.login1.Manager.Inhibit` (`what = "sleep:idle"`,
`who = "nestord"`, `why` explicite, `mode = "block"`), qui renvoie un
descripteur de fichier : l'inhibition dure tant que le descripteur reste
ouvert. Crate suggérée : `zbus`. À défaut, spawner
`systemd-inhibit --what=sleep:idle --who=nestord --why=... sleep infinity`
et tuer le processus pour relâcher.

Règle : nestord tient **un seul verrou**, pris dès qu'il existe au moins une
raison d'être actif, relâché quand il n'en reste aucune. Raisons :

- une mission en statut `started` ;
- la session conversationnelle en `thinking` ou `speaking` ;
- un appel mobile en cours ;
- un rendez-vous ou une alerte imminente (dans l'heure) ;
- plus tard, toute tâche planifiée déclarée par la session.

Le `why` affiché dans `systemd-inhibit --list` doit être lisible
(« mission #3 en cours ») : c'est le premier réflexe de diagnostic.

### Détecter l'absence : IdleMonitor de GNOME

nestord interroge `org.gnome.Mutter.IdleMonitor` sur le bus de session
(objet `/org/gnome/Mutter/IdleMonitor/Core`) :

- `GetIdletime` pour le temps d'inactivité en millisecondes ;
- `AddIdleWatch` (seuil, par exemple 15 minutes) et `AddUserActiveWatch`,
  avec le signal `WatchFired`, pour être notifié plutôt que de sonder.

nestord tourne dans la session graphique de l'utilisateur (service systemd
`--user`), ce qui lui donne accès au bus de session. Sans GNOME (ou sans bus
de session), le module se désactive proprement et considère l'utilisateur
comme présent : on préfère ne jamais endormir la machine plutôt que de
l'endormir à tort.

### Laisser dormir, puis réveiller Monsieur

Conditions **toutes requises** pour laisser la machine s'endormir :

1. Monsieur est **chez lui** (dernière position fraîche dans le rayon du lieu
   « domicile ») ;
2. il **n'utilise plus l'ordinateur** (inactivité GNOME au-delà du seuil) ;
3. **aucune raison d'être actif** (verrou d'inhibition relâché) ;
4. c'est une **plage de repos** : la nuit, ou aucun événement avant
   plusieurs heures.

Déroulé :

1. nestord lit l'agenda du lendemain et retient le **premier rendez-vous**.
   Heure de réveil = début du rendez-vous - temps de préparation - trajet
   estimé, bornée par une heure de réveil par défaut configurable si
   l'agenda est vide.
2. nestord programme un **timer systemd utilisateur** transitoire capable de
   sortir la machine de veille :
   `systemd-run --user --on-calendar="<date heure>" --timer-property=WakeSystem=true --unit=nestor-wake ...`
   (ou un couple `.timer`/`.service` écrit dans
   `~/.config/systemd/user/`). Le service déclenché envoie à nestord l'ordre
   de réveiller Monsieur (par exemple une requête sur un endpoint local
   dédié). Il remplace tout timer `nestor-wake` précédent : un seul réveil
   programmé à la fois.
3. nestord **vérifie** que le timer est bien armé (`systemctl --user
   list-timers`) **avant** de relâcher sa dernière inhibition. Sans réveil
   armé, il ne laisse pas dormir.
4. nestord relâche l'inhibition ; c'est la politique d'économie d'énergie de
   GNOME qui endort la machine. nestord ne force pas `systemctl suspend`,
   sauf option explicite de configuration.
5. Au réveil, nestord doit **se remettre d'aplomb** : le signal
   `PrepareForSleep(false)` de logind indique la sortie de veille ; relancer
   la session `claude` si elle est morte, reconnecter les sources, recalculer
   le contexte.
6. Il **réveille Monsieur** : sonnerie et message sur le téléphone (appel
   entrant ou notification), et éventuellement lecture sur les enceintes du
   poste. Nestor annonce la journée en une ou deux phrases : premier
   rendez-vous, heure de départ, un mail important s'il y en a.

À vérifier au moment de l'implémentation, sur la machine cible :

- que `WakeSystem=true` fonctionne sur ce matériel (RTC wake activée dans le
  firmware ; tester avec un timer à cinq minutes) ;
- le mode de veille réel (`/sys/power/mem_sleep` : `s2idle` ou `deep`), dont
  dépend la fiabilité du réveil ;
- les droits nécessaires pour un timer `WakeSystem` en instance `--user`
  (selon la version de systemd, il peut falloir passer par une instance
  système avec une règle polkit) ;
- le comportement du pilote NVIDIA à la sortie de veille, CUDA et les
  modèles audio chargés en VRAM compris.

---

## 4. Feuille de route

Mise à jour : 2026-10-09, après relecture du code. Les étapes 1 et 2 de la
version du 12 septembre sont réalisées ; l'ordre des étapes suivantes est
revu pour faire passer d'abord ce qui n'a besoin d'aucune source nouvelle.

### Ce qui existe déjà

- [x] Daemon nestord : WebSocket `/ws`, serveur MCP `/mcp`, instantané à la
      connexion.
- [x] Session `claude` headless en stream-json, prompt vocal
      `VOICE_SYSTEM_PROMPT`, purge des variables d'API, mode auto du CLI.
- [x] Missions déléguées (`claude` / `agy`), progression, annulation motivée,
      clôture des outils ouverts, routage selon le quota.
- [x] Suivi du quota Claude (`usage.rs`), bascule en mode réduit `agy` quand
      la session `claude` meurt ou que le quota est épuisé (`brain.rs`).
- [x] Pipeline audio local (VAD, Whisper, Piper) derrière `full-audio` ;
      barge-in vocal avec AEC côté serveur ; mot d'activation ; filtre des
      transcriptions hallucinées ; prototype Smart Turn désactivé par défaut.
- [x] Personnalité de majordome en tête du prompt, forme d'adresse
      configurable (`address_form`, `NESTORD_ADDRESS_FORM`).
- [x] Contexte tenu par nestord : lieux nommés avec rayon, heures calmes,
      outil MCP `get_context`, événement `context` sur `/ws`, position reçue
      sur `/ws` pendant un appel (`ClientEvent::Location`).
- [x] Tâches (`todo.rs`, SQLite) : outils MCP `todo_*`, commandes UI, rappel
      des tâches dues par une boucle de fond toutes les cinq minutes.
- [x] Sécurité : jeton d'accès stocké en empreinte, passkey WebAuthn pour
      l'interface web, secret distinct pour `/mcp`, origines vérifiées,
      secrets retirés de l'environnement de l'assistant.
- [x] Connecteurs MCP externes relayés par nestord (lecture libre, accord
      pour les écritures), juge local (Ollama) avec règles fixes.
- [x] UI web : orbe, dialogue, console d'outils, missions, tableau de bord,
      panneau Conscience, réglages à chaud, écran de connexion.
- [x] Appli mobile Android : appel via l'API Telecom, audio full-duplex,
      transcription, barge-in, confirmations en attente, URL et jeton
      persistés (`SecureStore`).
- [x] Accès Gmail dans la session via les connecteurs claude.ai.

### Étape A — Clore le chantier mobile en cours

- [ ] Valider sur appareil le diff non commité de `NestorCallManager.kt`
      (lecture TTS par thread dédié, AudioTrack recréé à la fréquence
      annoncée, accusé de fermeture WebSocket, transcriptions `is_final`),
      puis committer.
- [ ] Mettre `mobile/README.md` à jour : la voix est Piper à 22 050 Hz, la
      fréquence vient du champ `sample_rate` de chaque `audio_chunk`, pas
      d'une constante.
- [ ] Le filet de sécurité « outils restés ouverts » côté mobile est sans
      objet : l'appli ne suit pas les appels d'outils des missions. À
      reprendre seulement si elle les affiche un jour.

### Étape B — Dette courte

- [ ] `web/src/hooks/useNestorWebSocket.ts` : le `useCallback` signalé par
      `oxlint` (dépendances manquantes) peut figer d'anciennes closures ;
      corriger, puis traiter les cinq autres avertissements.
- [ ] Automatiser le test d'annulation de mission décrit dans
      `.agent/reponse-annulation-mission-ui.md` (commande longue, annulation,
      aucun outil laissé ouvert).
- [ ] Écran de liste et de révocation des passkeys (`docs/passkey.md`).
- [ ] Dépoussiérer `.agents/INITIAL-*.md`, `.agents/REMOTE-AUDIO-SPEC.md`,
      `docs/audio-protocol.md` et `nestord/README.md`, qui parlent encore
      d'Antigravity comme front et de Kokoro à 24 kHz.

### Étape C — Boucle proactive généralisée

Déplacée avant la position : elle n'a besoin d'aucune source nouvelle et
c'est elle qui donne à Nestor son initiative.

- [ ] Transformer la boucle de `todo.rs` en moteur de règles : tâche de fond
      réveillée à intervalle régulier et à chaque changement de contexte,
      dédoublonnage par événement et par palier, respect des heures calmes,
      jamais pendant une réponse en cours de synthèse.
- [ ] Règles sans source nouvelle : session `claude` morte ou en mode réduit
      depuis longtemps, mission sans progression, quota au-delà du seuil,
      tâche en retard (reprise de l'existant).
- [ ] Sortie unique : rapport interne réinjecté dans la conversation ;
      événement `alert` sur `/ws` pour la traçabilité, affiché dans le
      journal d'activité de l'UI.

### Étape D — Position en arrière-plan

- [ ] Localisation en arrière-plan dans l'appli (Expo v57, `expo-location`,
      `expo-task-manager`), permissions Android, sobriété (distance ou
      quelques minutes).
- [ ] Endpoint `POST /location` authentifié par le jeton existant ; mise à
      jour du contexte hors appel ; aucune journalisation de la position au
      niveau `info`.
- [ ] Accès distant : trancher entre l'application Tailscale du système
      (recommandé, rien à embarquer) et `libtailscale` compilée dans
      `mobile/native/tailscale`, non intégrée et non testée sur appareil.

### Étape E — Présence et inhibition de la veille

- [ ] Client D-Bus (`zbus`) IdleMonitor de GNOME : inactivité et retour
      d'activité ; désactivation propre hors GNOME, utilisateur présumé
      présent.
- [ ] Verrou logind unique, piloté par les raisons d'être actif (mission
      `started`, session en `thinking` ou `speaking`, appel mobile, événement
      imminent), avec un `why` lisible.
- [ ] Gestion de `PrepareForSleep` et reprise après veille : relance de la
      session `claude` si nécessaire, reconnexion des sources, recalcul du
      contexte.

### Étape F — Agenda

- [ ] Brancher Google Calendar en lecture directe depuis nestord (OAuth,
      jeton stocké localement) ; écriture par le connecteur de session.
- [ ] Prochains événements et lieux dans le contexte et dans `get_context`.
- [ ] Nouvelles règles de la boucle proactive : départ à temps (estimation
      du trajet), rendez-vous imminent, mails importants.
- [ ] Canal nestord → mobile hors appel (notification, appel entrant pour
      les urgences), pour choisir le bon canal d'alerte.

### Étape G — Veille nocturne et réveil

- [ ] Évaluation des conditions de mise en veille (domicile, inactivité,
      aucune raison d'être actif, plage de repos).
- [ ] Calcul de l'heure de réveil sur le premier rendez-vous du lendemain.
- [ ] Timer systemd `WakeSystem=true`, vérifié armé avant de relâcher
      l'inhibition.
- [ ] Réveil de Monsieur : sonnerie sur le téléphone et annonce de la
      journée.
- [ ] Tests sur la machine cible (RTC wake, mode de veille, NVIDIA).

### Chantiers parallèles, à trancher

- **Mémoire** (`docs/memory.md`) : encore une proposition. Elle sert au
  dédoublonnage fin des alertes et aux réponses sans relecture du dépôt ;
  à valider avant l'étape F, le moteur de l'étape C pouvant d'abord
  dédoublonner en mémoire vive.
- **Smart Turn** (`docs/smart-turn.md`) : à tester avec de vraies
  hésitations humaines avant de l'activer par défaut.

L'ordre compte : la dette courte évite de bâtir sur du sable ; la boucle
proactive précède la position parce qu'elle vaut déjà sans elle ;
l'inhibition précède la mise en veille volontaire ; et la veille nocturne
vient en dernier, parce qu'elle dépend de toutes les autres briques et
qu'une erreur y coûte un rendez-vous manqué.
