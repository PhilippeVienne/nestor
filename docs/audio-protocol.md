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

## Annulation d'echo cote serveur (AEC)

Pendant que Nestor parle, le micro passe par l'AEC3 de WebRTC (`sonora`, Rust pur,
`audio/aec.rs`) avant le VAD du barge-in vocal, avec pour reference la voix que
Nestor envoie aux haut-parleurs. Les trames de reference avancent au rythme des
trames micro (10 ms pour 10 ms) ; l'AEC estime lui-meme le retard de lecture.

**Contrainte pour les clients** : envoyer le micro **en continu** tant que de
l'audio est lu (plus ~1,5 s), sans filtre d'energie. Des trous dans le flux micro
desynchroniseraient la reference. Le client web et l'app Android le font des
reception d'un `audio_chunk` et reprennent leur filtre apres un `interrupt`.

Desactivation : `NESTORD_AEC=0` ou `[barge_in] aec = false`.

### Validation (simulation)

Banc de test : le client rejoue au micro la voix de Nestor, retardee de 100 ms
et attenuee de moitie, avec du bruit.

| Scenario | Resultat |
|---|---|
| Echo seul, sans AEC | Nestor s'interrompt tout seul apres 0,5 s |
| Echo seul, avec AEC | aucune interruption pendant toute la reponse |
| Echo + parole utilisateur, avec AEC | interruption ~0,6 s apres le debut de la parole |

Limites : echo lineaire et sans reverberation, sans l'AEC du client ni distorsion
des haut-parleurs. A confirmer avec un vrai materiel.

### Reglages et limites du barge-in vocal (retour d'essai reel)

Un essai reel a montre qu'exiger 300 ms de parole **sans aucune fenetre sous le
seuil** echouait : la parole naturelle fait de petites chutes de probabilite
entre les syllabes. Le critere compte maintenant les fenetres de parole
(`threshold` 0,75, `min_speech_ms` 300) en tolerant jusqu'a 160 ms de chutes, et
exige une energie minimale apres AEC (`min_rms` 0,012) pour ecarter les residus
d'echo. L'AEC tourne sur toutes les trames (reference nulle hors lecture).

Un journal de bilan est ecrit apres chaque lecture (niveau `debug`) : probabilite
VAD maximale, pic du micro brut et pic apres AEC. Au declenchement, le RMS moyen
de la parole detectee est journalise : a comparer a `min_rms` si l'interruption
est trop timide ou trop sensible.

Limites connues (simulation sans AEC client) : apres une interruption, l'AEC peut
laisser passer des residus pendant sa reconvergence (quelques secondes), et une
voix faible parlant par-dessus un echo fort peut etre partiellement supprimee.

## Reglages a chaud et mesures pour l'UI

- `{"type":"settings","settings":{...}}` (serveur) : reglages courants, envoyes a la
  connexion et a chaque changement. `{"type":"update_settings","settings":{...}}`
  (client) : objet complet ; le daemon borne les valeurs, les applique tout de suite
  et les enregistre dans `~/.config/nestord/ui-settings.toml`. `config.toml` n'est
  jamais reecrit. Priorite au demarrage : config.toml < ui-settings.toml < variables
  d'environnement. Champs : interruption vocale, AEC, seuil, duree et energie
  minimales du barge-in, Smart Turn, mot-cle exige, fenetre de dialogue, modele et
  seuils du juge (cf. `nestord/src/settings.rs`).
- `{"type":"audio_levels","rms":..,"peak":..,"vad":..}` : niveau du micro recu et
  derniere probabilite de parole, environ 10 fois par seconde tant que des trames
  micro arrivent.
- `{"type":"interrupt","rms":..}` : `rms` est le niveau moyen de la parole qui a
  declenche l'interruption, a comparer a l'energie minimale reglee.
- `{"type":"echo_discarded","text":".."}` : enonce ecarte car il recopiait la voix
  de Nestor.

Le client web n'arrete plus d'envoyer le micro pendant la lecture : ce blocage
empechait toute interruption vocale depuis le navigateur.

## Conscience (juge) et journal

- `{"type":"judge_verdict","id":..,"source":"message"|"mission","text":"..",
  "decision":"allow"|"confirm"|"deny","score":..,"category":"..","rationale":"..",
  "pending":bool,"at_ms":..}` : chaque decision du juge. `pending` vaut vrai quand
  une confirmation de l'utilisateur est attendue. Les 30 dernieres sont rejouees a
  la connexion.
- `{"type":"resolve_judgement","id":..,"approve":bool}` (client) : approuve ou refuse
  la demande en attente, comme un « oui » a la voix. Sans effet si elle a deja ete
  tranchee. Le daemon repond par `{"type":"judge_resolved","id":..,"approved":bool}`,
  emis aussi quand la confirmation est donnee ou abandonnee a la voix.
- Le journal d'activite de l'UI est construit cote navigateur a partir des
  evenements recus : il repart de zero au rechargement de la page.

## Tableau de bord

Evenements du serveur (envoyes a la connexion, puis a chaque changement) :

- `context` : `place` (lieu reconnu), `quiet_start`, `quiet_end`, `quiet_active`,
  `auth_required` (un jeton est exige sur `/ws`). Rediffuse toutes les 30 s et a
  chaque changement de lieu.
- `todos` : `items`, les taches en attente (`id`, `title`, `recurrence`, `due_at`
  en secondes). Rediffuse apres chaque ajout, completion ou suppression, que ce
  soit par l'UI ou par les outils MCP `todo_*`.
- `clients` : `items` (`id`, `kind` = `web`/`mobile`/`autre`, `connected_at_ms`).
  Le type vient du parametre `?client=` de l'URL, sinon de l'en-tete User-Agent.
- `telemetry` (toutes les 5 s) : `stt_model`, `tts_voice`, `judge_model`, `gpu`
  (`name`, `memory_used_mb`, `memory_total_mb`, via `nvidia-smi`), et les
  dernieres latences mesurees : `stt_ms` (transcription), `first_word_ms` (du
  debut de reflexion au premier mot de la reponse) et `tts_ms` (synthese d'une
  phrase). Un champ absent signifie « pas encore mesure ».
- `connectors` : `items` (`name`, `kind`, `tools`). Aujourd'hui le seul serveur
  MCP est celui de nestord.

Commandes du client : `todo_add` (`title`, `due_at` `AAAA-MM-JJTHH:MM` ou
`recurrence`), `todo_complete` (`id`), `todo_delete` (`id`).

## Connecteurs externes

Voir `docs/connecteurs.md`. Evenements : `connectors` (chaque serveur avec
`status`, `detail` et ses outils : `name`, `description`, `mode`, `default_mode`),
`tool_approval` (`id`, `server`, `tool`, `arguments`, `at_ms`) quand une ecriture
attend un accord, `tool_approval_resolved` (`id`, `approved`). Commandes :
`set_tool_mode` (`server`, `tool`, `mode` = `read`/`confirm`/`off`) et
`resolve_tool_approval` (`id`, `approve`).
