# Protocole audio binaire nestord <-> front (Antigravity)

Ce document complete `.agents/INITIAL-CLAUDE.md` et `.agents/INITIAL-AGY.md` :
il decrit le contrat des **frames binaires** WebSocket, en plus des messages
JSON deja specifies (`state`, `transcript`, `tool_call`, `barge_in`, `send_text`).

Repartition des responsabilites :
- **Front (Antigravity)** : capture microphone (`getUserMedia` + resampling),
  lecture audio (Web Audio API), visualisation (orbe).
- **nestord (Rust)** : VAD (Silero v5), STT (Whisper large-v3-turbo), TTS
  (Piper, voix masculine francaise), tout sur GPU (CUDA). Aucun acces direct
  au materiel audio.

## Frame binaire client -> serveur (audio micro)

- Format : **PCM16LE mono, 16 000 Hz**.
- Taille de frame libre (recommande : 20-40 ms, soit 640-1280 octets).
- Le front doit resampler le flux du microphone (typiquement 48 kHz natif)
  vers 16 kHz mono avant envoi (ex: `AudioWorklet` + resampler, ou
  `OfflineAudioContext`).
- Chaque frame `Message::Binary` recue est ajoutee au buffer d'analyse VAD
  cote serveur ; il n'y a pas de decoupage par enonce a faire cote front, le
  serveur gere la segmentation (VAD + hangover de silence) et declenche lui
  meme la transcription puis l'envoi a Claude.

Le front actuel (`useNestorWebSocket.ts`) envoie en plus, pour chaque frame,
un evenement JSON `{"type":"audio_in","pcm_base64":"...","sample_rate":16000}`
avec le meme audio. nestord accepte ce message (`ClientEvent::AudioIn`) mais
l'ignore volontairement pour ne pas traiter deux fois le meme son - seule la
frame binaire alimente le pipeline VAD/STT.

## Audio TTS serveur -> client : evenement JSON `audio_chunk` (pas une frame binaire)

```json
{"type": "audio_chunk", "data": "<base64>", "format": "pcm16", "sample_rate": 22050}
```

- `data` : PCM16LE mono encode en base64.
- `format` : toujours `"pcm16"` cote nestord.
- `sample_rate` : depend de la voix Piper chargee (22 050 Hz pour la voix par
  defaut `fr_FR-upmc-medium`). Le front doit lire ce champ, pas une constante.
- Un message = un segment de phrase synthetise (decoupage par ponctuation
  cote serveur, cf. `claude_process.rs`).

**Pourquoi pas une frame binaire brute comme pour l'entree ?** Le hook front
existant (`useNestorWebSocket.ts`) route toute frame `ArrayBuffer` recue vers
`audioPlayer.enqueueChunk(b64, 'wav')` - il suppose donc un conteneur WAV
complet. nestord envoie du PCM16 brut sans en-tete, ce que `decodeAudioData`
rejette silencieusement (erreur uniquement visible dans la console navigateur,
aucun son ne sort). Le cas `audio_chunk` JSON existait deja cote front avec
un format explicite `'pcm16'` correctement gere par `AudioPlayer` (construction
manuelle d'un `AudioBuffer`), donc nestord l'utilise plutot que de forcer le
front a re-encoder ses frames binaires en WAV.

Si le front est modifie pour que sa reception binaire gere aussi le PCM16 brut
(second parametre de `enqueueChunk`), on pourra revenir a une frame binaire
pure pour la sortie - a coordonner avant de changer.

- Interruption (barge-in) : le front doit stopper immediatement sa lecture en
  cours (et vider toute file d'attente de segments non encore joues) des
  reception de l'evenement JSON `{"type":"state","status":"listening"}`
  suite a un `barge_in` qu'il a lui-meme emis, ou emis par un autre client.

## Bugs corriges lors de la premiere integration bout-en-bout

- **VAD toujours a ~0** : le modele Silero VAD v5 (`silero_vad.onnx`) exige de
  prefixer chaque fenetre de 512 echantillons avec 64 echantillons de
  "contexte" (fin de la fenetre precedente, zeros au demarrage). Sans ce
  prefixe, la sortie reste proche de 0 quel que soit le contenu audio (verifie
  avec un vrai extrait de parole). Voir `audio/vad.rs`.
- **Aucun son cote front malgre `audio_chunk`/frames binaires envoyes** : cf.
  la section precedente sur le choix `audio_chunk` JSON vs frame binaire.
- **Chaque message affiche deux fois dans le chat** : avec `<StrictMode>`, le
  hook `useNestorWebSocket` pouvait laisser deux sockets vivants ; le socket
  remplace continuait a alimenter l'etat React, et son evenement final
  recreait une bulle. Corrige par un garde `socketRef.current !== ws` dans les
  handlers et le detachement des handlers avant fermeture.
- **Voix hachee** : le decoupage des segments TTS se faisait aussi sur les
  virgules, produisant des fragments de quelques mots synthetises
  independamment. Corrige dans `claude_process.rs` (ponctuation de fin de
  phrase uniquement + longueur minimale de segment).

## Ce qui a change par rapport a la version initiale des prompts

- `audio_levels` (JSON) n'est plus emis par nestord : le front a deja acces
  aux echantillons bruts (capture et lecture), donc le calcul RMS/peak pour
  l'orbe se fait cote front, sur ses propres buffers.
- nestord n'utilise plus `cpal`/`rodio` : uniquement `ort` (ONNX Runtime,
  CUDA) pour Silero VAD et Kokoro, et `whisper-rs` (whisper.cpp, CUDA) pour
  la transcription.


## Interruption (barge-in)

- **Manuel** : le client coupe sa lecture puis envoie `{"type":"barge_in"}`.
- **Vocal** : pendant que Nestor parle, nestord surveille le micro (VAD). Une
  parole franche et soutenue (`[barge_in] threshold = 0.85`, `min_speech_ms = 300`)
  coupe la synthese : le serveur emet `{"type":"interrupt"}` et chaque client doit
  vider sa file de lecture. La parole qui a declenche l'interruption devient le
  debut de l'enonce suivant. `NESTORD_VOICE_BARGE_IN=0` desactive ce mode.
- Dans les deux cas, les phrases restantes de la reponse interrompue ne sont plus
  synthetisees tant qu'un nouveau tour utilisateur n'a pas commence.
- L'echo de la voix de Nestor ne doit pas l'interrompre : les clients gardent
  l'annulation d'echo activee (navigateur : `echoCancellation`, Android :
  `VOICE_COMMUNICATION` + `AcousticEchoCanceler`).

## Auto-ecoute (echo de Nestor)

Au-dela de la fenetre de suppression temporelle, `audio/echo.rs` garde les
phrases recemment prononcees (90 s). Un enonce transcrit qui les recopie (au
moins 80 % de mots communs dont un mot porteur de sens, ou la moitie des
enchainements de deux mots) est ecarte : ni dialogue, ni reveil, ni prolongation
de la fenetre conversationnelle. Les enonces de moins de 4 mots ne sont jamais
filtres. Cela traite l'echo *transcrit* ; l'echo qui declencherait une
interruption vocale releve de l'annulation d'echo des clients.
