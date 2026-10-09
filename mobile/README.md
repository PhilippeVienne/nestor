# Nestor Mobile — React Native avec Intégration Téléphonique Android (Telecom API)

Application mobile compagnon pour **Nestor** (l'assistant vocal local pour Claude Code).
Cette application transforme votre smartphone Android en combiné d'appel dédié pour dialoguer avec Nestor comme lors d'un véritable appel téléphonique.

---

## 🌟 Fonctionnalités Clés

### 1. API Telecom Android (`ConnectionService`)
- **Appel Système Réel** : L'appel avec Nestor est géré au niveau OS via l'API Android Telecom (`PhoneAccount.CAPABILITY_SELF_MANAGED`).
- **Notification d'Appel en Cours** : Présence permanente dans le volet de notifications Android (`NotificationCompat.CATEGORY_CALL`) avec bouton direct *Raccrocher*.
- **Contrôles Écran Verrouillé & Écran de Veille** : Possibilité de maintenir la communication même téléphone verrouillé dans la poche ou en voiture via Bluetooth mains-libres (SCO).
- **Capteur de Proximité** : L'écran s'éteint automatiquement lorsqu'on porte le téléphone à l'oreille (`PROXIMITY_SCREEN_OFF_WAKE_LOCK`).

### 2. Audio Haute Fidélité & Full-Duplex
- **Annulation Matérielle d'Écho (AEC)** & **Suppression de Bruit** : Utilisation de `MediaRecorder.AudioSource.VOICE_COMMUNICATION` et activation native des effets audio Android (`AcousticEchoCanceler`, `NoiseSuppressor`).
- **Bascule des Sorties Audio** : Bouton direct pour commuter entre l'écouteur interne (oreille), le haut-parleur externe et les casques Bluetooth connectés.
- **Microphone 16 kHz Mono PCM** : Flux binaire brut envoyé au daemon `nestord` via WebSocket.
- **AudioTrack à la fréquence annoncée** : lecture de la synthèse vocale Piper (22 050 Hz pour la voix par défaut) ; la fréquence vient du champ `sample_rate` de chaque `audio_chunk`, un thread dédié écrit les segments par tranches pour que le barge-in prenne effet aussitôt.

### 3. Interface Sombre & Orbe Réactif
- **Orbe Audio-Réactif** : Orbe holographique pulsant au rythme des amplitudes vocales (RMS & Peak) avec transitions de couleurs d'état :
  - 🔵 **Bleu / Cyan** : Écoute active de votre voix.
  - 🟣 **Violet** : Réflexion / Exécution Claude Code.
  - 🟢 **Émeraude** : Nestor vous répond vocalement.
- **Barge-In Instantané** : Coupure instantanée de la synthèse dès que vous reprenez la parole ou appuyez sur le bouton *Interrompre*.
- **Retranscription en Direct & Clavier** : Affichage des bulles de discussion en temps réel et tiroir clavier pour saisir des requêtes textuelles pendant l'appel.
- **Confirmations en attente** : une demande du juge (`judge_verdict` avec `pending`) ou une écriture d'un connecteur externe (`tool_approval`) s'affiche dans l'écran d'appel avec « Approuver » / « Refuser » ; le bandeau disparaît à l'événement `*_resolved` du daemon.
- **Interruption vocale** : une pastille brève signale l'événement `interrupt` reçu du daemon.
- L'app s'annonce au daemon avec `?client=mobile` (panneau « Appareils »).

---

## 🛠️ Architecture Technique

```
mobile/
├── android/
│   └── app/src/main/
│       ├── AndroidManifest.xml          # Permissions Telecom, Audio, Foreground Service
│       └── java/com/nestor/assistant/
│           ├── MainActivity.kt
│           ├── MainApplication.kt       # Enregistrement de NestorCallPackage
│           └── telecom/
│               ├── NestorConnectionService.kt       # Service Telecom Android
│               ├── NestorConnection.kt              # Objet Connection OS (voip=true)
│               ├── NestorCallForegroundService.kt   # Notification système d'appel
│               ├── NestorCallManager.kt             # Moteur Audio (AudioRecord/Track), AEC & WebSocket
│               ├── NestorCallModule.kt              # Bridge React Native NativeModule
│               └── NestorCallPackage.kt             # ReactPackage
└── src/
    ├── native/
    │   └── NestorCall.ts                # Typage TypeScript et écouteurs d'événements
    ├── hooks/
    │   └── useNestorCall.ts             # Hook d'état, timer d'appel, gestion messages
    ├── components/
    │   └── SoundWaveOrb.tsx             # Orbe animé audio-réactif
    └── screens/
        ├── DialerScreen.tsx             # Écran d'accueil, configuration URL et composition
        └── CallScreen.tsx               # Écran d'appel actif style Android In-Call UI
```

---

## 🚀 Démarrage Rapide

### Prérequis
- Node.js ≥ 20
- JDK 17 (configuré dans `android/gradle.properties`)
- Android SDK (API 34 ou 35)
- Appareil Android physique connecté en USB (`adb`) ou émulateur Android en cours d'exécution.

### 1. Installation des dépendances
```bash
cd mobile
npm install
```

### 2. Lancement du bundler Metro
```bash
npm start
```

### 3. Compilation et installation sur Android
```bash
npm run android
# Ou via Gradle direct :
cd android && ./gradlew installDebug
```

### 4. Configuration de l'adresse du serveur
- **Sur émulateur Android** : laisser `ws://10.0.2.2:8340/ws` (adresse passerelle vers l'hôte).
- **Sur téléphone physique en Wi-Fi** : entrer l'adresse IP locale de votre machine de dev (ex: `ws://192.168.1.50:8340/ws`).
