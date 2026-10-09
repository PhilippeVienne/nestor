# PROMPT INITIAL POUR ANTIGRAVITY

> Document historique (septembre 2026) : prompt de depart, conserve pour la tracabilite. Le contrat en vigueur est celui de `nestord/src/protocol.rs` et de `docs/audio-protocol.md` ; la voix est Piper (22 050 Hz), pas Kokoro, et l'UI web vit dans `web/`.

Tu collabores avec Claude Code (en charge du backend daemon Rust `jarvisd`) sur un projet commun : "Jarvis local pour Claude Code".
Consulte et synchronise-toi impérativement avec les spécifications techniques ci-dessous pour respecter les contrats d'interface (WebSocket, JSON, ports).

## 1. Contexte & Architecture
- Rôle : Développer l'interface utilisateur web (Desktop-first, thème sombre futuriste/minimaliste type orbe Jarvis + console d'observabilité).
- Stack recommandée : Vite, React/TypeScript, Tailwind CSS, Three.js / React Three Fiber (pour l'orbe de particules).
- Connectivité : Se branche en WebSocket sur le backend Rust à `ws://127.0.0.1:8340/ws`.

## 2. Composants UI requis
1. **L'Orbe interactif (Hero Visual)** :
   - Rendu Three.js à base de particules.
   - Réagit visuellement au flux d'amplitude reçu du WebSocket (`rms`, `peak`).
   - Couleurs et pulsation indexées sur le statut :
     * Bleue/Cyan pulsante : `listening` (écoute micro active)
     * Violette/Ambre en rotation rapide : `thinking` (inférence Claude)
     * Éclats dorés ou blancs calés sur l'audio : `speaking` (synthèse Kokoro)
     * Pulsation lente atténuée : `idle`
2. **Zone de dialogue en direct** :
   - Affichage en temps réel des transcriptions STT (voix utilisateur).
   - Streaming markdown des réponses de Claude Code au fur et à mesure des deltas reçus.
3. **Console d'exécution & Outils** :
   - Panneau latéral affichant les appels d'outils (`tool_call`) en direct (commandes Bash, édition de fichiers, exécution de tests).
4. **Contrôles d'intervention rapide** :
   - Bouton d'interruption immédiate (Barge-in).
   - Champ de fallback pour saisie texte au clavier.

## 3. Contrat d'Interface WebSocket avec Claude Code (Rust)
Le client WebSocket doit gérer les messages suivants :

- Réception depuis Rust :
  * `{"type": "state", "status": "listening" | "thinking" | "speaking" | "idle"}`
  * `{"type": "audio_levels", "rms": number, "peak": number}`
  * `{"type": "transcript", "role": "user" | "assistant", "delta"?: string, "text": string, "is_final"?: boolean}`
  * `{"type": "tool_call", "name": string, "input": object, "status": "running" | "completed"}`

- Émission vers Rust :
  * Interruption vocale : `{"type": "barge_in"}`
  * Envoi de texte : `{"type": "send_text", "content": string}`

## 4. Tâche immédiate
1. Initialise l'application Vite / React / Tailwind.
2. Crée la couche de connexion WebSocket résiliente avec reconnexion automatique vers `ws://127.0.0.1:8340/ws`.
3. Implémente l'orbe 3D Three.js et branche-le sur les payloads `state` et `audio_levels`.
