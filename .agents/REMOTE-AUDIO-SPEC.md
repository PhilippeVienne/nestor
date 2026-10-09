# SPÉCIFICATION TECHNIQUE : AUDIO CAPTURE & PLAYBACK DANS LA PAGE WEB (USAGE REMOTE)

> Document historique (septembre 2026) : prompt de depart, conserve pour la tracabilite. Le contrat en vigueur est celui de `nestord/src/protocol.rs` et de `docs/audio-protocol.md` ; la voix est Piper (22 050 Hz), pas Kokoro, et l'UI web vit dans `web/`.

## 1. Motivation
Permettre l'utilisation de Nestor à distance (depuis un navigateur distant, laptop, smartphone, etc.) sans dépendre du micro et des haut-parleurs physiques de la machine hôte serveur (Linux/ALSA/cpal/rodio).

## 2. Audio Entrée (Microphone capturé par le Web)
- **Côté Web (Client)** :
  - Capture du microphone utilisateur via `navigator.mediaDevices.getUserMedia({ audio: { channelCount: 1, sampleRate: 16000, echoCancellation: true, noiseSuppression: true } })`.
  - Calcul local en temps réel du RMS et du Peak via `AnalyserNode` pour piloter instantanément l'Orbe 3D Three.js sans latence réseau.
  - Streaming vers `nestord` sur `ws://127.0.0.1:8340/ws` :
    * **Frames binaires WebSocket (`Message::Binary(bytes)`)** : Chunks PCM 16 kHz 16-bit mono Little-Endian (Int16Array, 512 ou 1024 samples par paquet).
    * Ou alternative JSON : `{"type": "audio_in", "pcm_base64": string}`.
- **Côté Daemon (`nestord`)** :
  - Reçoit les buffers PCM 16 kHz envoyés par le client et les achemine vers Silero VAD + Whisper large-v3-turbo STT (remplaçant ou complétant `cpal`).

## 3. Audio Sortie (Synthèse vocale jouée par le Web)
- **Côté Daemon (`nestord`)** :
  - Après synthèse Kokoro-82M (24 kHz mono), au lieu de jouer sur `rodio`, `nestord` émet sur le WebSocket :
    * Événement serveur JSON :
      ```json
      {
        "type": "audio_chunk",
        "data": "<base64_encoded_audio>",
        "format": "wav",
        "sample_rate": 24000,
        "is_final": false
      }
      ```
- **Côté Web (Client)** :
  - Réception de l'événement `audio_chunk`.
  - Décodage via `AudioContext.decodeAudioData` (ou buffer PCM) et enchaînement dans une file audio fluide.
  - Les flux audio lus passent par un `AnalyserNode` web audio pour alimenter le Hero Visual (l'Orbe 3D) en temps réel avec les vraies fréquences perçues par l'utilisateur.
  - Fallback local : synthèse vocale Web Speech API (`window.speechSynthesis`) si `audio_chunk` n'est pas configuré.

## 4. Barge-In (Interruption immédiate)
- Émission client : `{"type": "barge_in"}`.
- Le client Web coupe immédiatement la lecture audio en cours, vide les files d'attente locales.
- Le daemon coupe la génération et vide les buffers de génération.
