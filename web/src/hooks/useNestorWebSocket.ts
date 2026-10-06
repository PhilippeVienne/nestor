import { useState, useEffect, useRef, useCallback } from 'react';
import type {
  ToolApprovalItem,
  ToolMode,
  ContextInfo,
  TodoItem,
  ClientInfo,
  TelemetryInfo,
  ConnectorInfo,
  JudgeItem,
  ActivityItem,
  NestorSettings,
  VoiceMeter,
  DaemonStatus,
  AudioLevels,
  ServerEvent,
  ClientEvent,
  MessageItem,
  ToolCallItem,
  MissionItem,
  UsageInfo,
  ConnectionState,
  BackendStatusInfo,
} from '../types';
import { AudioRecorder } from '../audio/audioRecorder';
import { AudioPlayer } from '../audio/audioPlayer';

/**
 * Ajoute le jeton d'authentification (`VITE_NESTOR_TOKEN`) en query param.
 * Absent par defaut : nestord n'exige un jeton que si `auth_token` est
 * configure cote serveur (cf. `nestord/src/config.rs`), tolerable tant que
 * le daemon n'ecoute que sur 127.0.0.1.
 */
const TOKEN_STORAGE_KEY = 'nestor_token';

const SESSION_STORAGE_KEY = 'nestor_session';

/** Adresse par defaut du daemon. */
export const DEFAULT_WS_URL = 'ws://127.0.0.1:8340/ws';

/**
 * Jeton presente au daemon, par priorite : session ouverte par passkey (onglet en
 * cours), jeton saisi dans l'ecran Reglages (memorise dans ce navigateur), jeton du build.
 */
function readAuthToken(): string {
  try {
    const session = window.sessionStorage.getItem(SESSION_STORAGE_KEY);
    if (session) return session;
    const stored = window.localStorage.getItem(TOKEN_STORAGE_KEY);
    if (stored) return stored;
  } catch {
    // stockage indisponible (navigation privee...) : on retombe sur le jeton du build
  }
  return (import.meta.env.VITE_NESTOR_TOKEN as string | undefined) ?? '';
}

function withAuthToken(url: string): string {
  const token = readAuthToken();
  if (!token) return url;
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}token=${encodeURIComponent(token)}`;
}

interface UseNestorWebSocketOptions {
  url?: string;
  autoReconnect?: boolean;
  reconnectInterval?: number;
}

export function useNestorWebSocket({
  url = DEFAULT_WS_URL,
  autoReconnect = true,
  reconnectInterval = 2000,
}: UseNestorWebSocketOptions = {}) {
  const [connectionState, setConnectionState] = useState<ConnectionState>('connecting');
  const [status, setStatus] = useState<DaemonStatus>('idle');
  const [audioLevels, setAudioLevels] = useState<AudioLevels>({ rms: 0, peak: 0 });
  const [messages, setMessages] = useState<MessageItem[]>([]);
  const [toolCalls, setToolCalls] = useState<ToolCallItem[]>([]);
  const [missions, setMissions] = useState<MissionItem[]>([]);
  const [usage, setUsage] = useState<UsageInfo | null>(null);
  const [backendStatus, setBackendStatus] = useState<BackendStatusInfo>({
    active_backend: 'claude',
    is_fallback: false,
    reason: null,
  });
  const [isSimulated, setIsSimulated] = useState(false);

  // Web Audio state
  const [isMicActive, setIsMicActive] = useState(false);
  const [isWakeActive, setIsWakeActive] = useState(false);
  const [settings, setSettings] = useState<NestorSettings | null>(null);
  const settingsRef = useRef<NestorSettings | null>(null);
  const [judgements, setJudgements] = useState<JudgeItem[]>([]);
  const [context, setContext] = useState<ContextInfo | null>(null);
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [clients, setClients] = useState<ClientInfo[]>([]);
  const [telemetry, setTelemetry] = useState<TelemetryInfo | null>(null);
  const [connectors, setConnectors] = useState<ConnectorInfo[]>([]);
  const [toolApprovals, setToolApprovals] = useState<ToolApprovalItem[]>([]);
  const [activity, setActivity] = useState<ActivityItem[]>([]);
  const lastWakeRef = useRef<boolean | null>(null);
  const knownJudgeIdsRef = useRef(new Set<number>());
  const wasConnectedRef = useRef(false);
  const [voiceMeter, setVoiceMeter] = useState<VoiceMeter>({ rms: 0, vad: 0 });
  const [isSpeakerActive, setIsSpeakerActive] = useState(true);
  const [isBrowserTtsEnabled, setIsBrowserTtsEnabled] = useState(false);

  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<number | null>(null);
  const isExplicitCloseRef = useRef(false);
  const connectRef = useRef<() => void>(() => {});
  const watchedPositionRef = useRef<{ lat: number; lon: number } | null>(null);

  // Audio players and recorders
  const audioPlayerRef = useRef<AudioPlayer | null>(null);
  const audioRecorderRef = useRef<AudioRecorder | null>(null);
  const playbackEndRef = useRef(0);
  const hasReceivedAudioChunkRef = useRef(false);
  // Initialize AudioPlayer on mount
  useEffect(() => {
    audioPlayerRef.current = new AudioPlayer({
      onPlaybackLevels: (rms, peak) => {
        setAudioLevels({ rms, peak });
      },
      onPlaybackStateChange: (isPlaying) => {
        if (isPlaying) {
          setStatus('speaking');
        } else {
          setStatus((prev) => (prev === 'speaking' ? 'idle' : prev));
          setAudioLevels({ rms: 0, peak: 0 });
        }
      },
    });

    return () => {
      if (audioPlayerRef.current) {
        audioPlayerRef.current.stop();
      }
    };
  }, []);

  // Send ClientEvent (JSON)
  // Evenement du pipeline (interruption, echo ecarte) insere dans le fil du dialogue.
  const pushNotice = useCallback((text: string) => {
    setMessages((prev) => [
      ...prev,
      { id: `notice-${Date.now()}-${prev.length}`, role: 'notice', text, isFinal: true, timestamp: new Date() },
    ]);
  }, []);

  // Journal d'activite : les 100 derniers evenements, du plus recent au plus ancien.
  const pushActivity = useCallback((kind: ActivityItem['kind'], text: string) => {
    setActivity((prev) =>
      [{ id: `act-${Date.now()}-${prev.length}`, kind, text, timestamp: new Date() }, ...prev].slice(0, 100)
    );
  }, []);

  const sendEvent = useCallback((event: ClientEvent) => {
    if (socketRef.current && socketRef.current.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify(event));
      return true;
    }
    return false;
  }, []);

  // Suit la position GPS pour alimenter la reconnaissance de lieu du backend
  // (domicile, bureau...). Best-effort : silencieux si l'utilisateur refuse
  // ou si le navigateur ne supporte pas l'API.
  useEffect(() => {
    if (!('geolocation' in navigator)) return;

    const watchId = navigator.geolocation.watchPosition(
      (position) => {
        const location = { lat: position.coords.latitude, lon: position.coords.longitude };
        watchedPositionRef.current = location;
        sendEvent({ type: 'location', ...location });
      },
      (err) => {
        console.debug('[Nestor] Geolocalisation indisponible:', err.message);
      },
      { enableHighAccuracy: false, maximumAge: 5 * 60 * 1000, timeout: 30 * 1000 }
    );

    return () => navigator.geolocation.clearWatch(watchId);
  }, [sendEvent]);

  // Send binary audio chunk to WebSocket
  const sendBinaryAudio = useCallback((buffer: ArrayBuffer) => {
    if (socketRef.current && socketRef.current.readyState === WebSocket.OPEN) {
      socketRef.current.send(buffer);
      return true;
    }
    return false;
  }, []);

  // Connect WebSocket
  const connect = useCallback(() => {
    if (isSimulated) return;
    if (
      socketRef.current &&
      (socketRef.current.readyState === WebSocket.OPEN ||
        socketRef.current.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }

    try {
      const ws = new WebSocket(withAuthToken(url));
      ws.binaryType = 'arraybuffer';
      socketRef.current = ws;

      ws.onopen = () => {
        setConnectionState('connected');
        wasConnectedRef.current = true;
        pushActivity('system', 'Connexion au daemon établie');
        // Un daemon redemarre repart de l'identifiant 1 : l'instantane refait la liste.
        knownJudgeIdsRef.current.clear();
        setJudgements([]);
        setToolApprovals([]);
        // Rejoue la derniere position connue : le backend n'en a pas garde
        // trace d'une session a l'autre (etat en memoire uniquement).
        if (watchedPositionRef.current) {
          sendEvent({ type: 'location', ...watchedPositionRef.current });
        }
      };

      ws.onmessage = (event) => {
        // Un socket remplace (double montage StrictMode, reconnexion) peut
        // rester vivant quelques instants : sans ce garde, ses messages sont
        // appliques une seconde fois et chaque bulle apparait en double.
        if (socketRef.current !== ws) return;

        // Binary messages (audio from server)
        if (event.data instanceof ArrayBuffer) {
          if (isSpeakerActive && audioPlayerRef.current) {
            hasReceivedAudioChunkRef.current = true;
            // Decode and enqueue arraybuffer directly
            // or convert to base64
            const bytes = new Uint8Array(event.data);
            let binary = '';
            for (let i = 0; i < bytes.byteLength; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            const b64 = window.btoa(binary);
            audioPlayerRef.current.enqueueChunk(b64, 'wav');
          }
          return;
        }

        try {
          const data: ServerEvent = JSON.parse(event.data);

          switch (data.type) {
            case 'state':
              setStatus(data.status);
              break;

            case 'wake_state':
              setIsWakeActive(data.active);
              if (lastWakeRef.current !== data.active) {
                // Le premier evenement est l'instantane de connexion, pas un changement.
                if (lastWakeRef.current !== null) {
                  pushActivity('wake', data.active ? 'Réveil : dialogue actif' : 'Retour en veille, fenêtre de dialogue expirée');
                }
                lastWakeRef.current = data.active;
              }
              break;

            case 'judge_verdict': {
              const item: JudgeItem = {
                id: data.id,
                source: data.source,
                text: data.text,
                decision: data.decision,
                score: data.score,
                category: data.category,
                rationale: data.rationale,
                pending: data.pending,
                timestamp: data.at_ms ? new Date(data.at_ms) : new Date(),
              };
              // L'instantane de connexion rejoue les decisions deja connues.
              const isNew = !knownJudgeIdsRef.current.has(item.id);
              knownJudgeIdsRef.current.add(item.id);
              if (isNew && item.decision !== 'allow') {
                pushActivity('judge', `Juge : ${item.decision === 'deny' ? 'refus' : 'confirmation demandée'} — ${item.text}`);
              }
              setJudgements((prev) =>
                prev.some((j) => j.id === item.id)
                  ? prev.map((j) => (j.id === item.id ? { ...j, pending: item.pending } : j))
                  : [item, ...prev].slice(0, 30)
              );
              break;
            }

            case 'judge_resolved': {
              const { id, approved } = data;
              setJudgements((prev) =>
                prev.map((j) => (j.id === id ? { ...j, pending: false, resolved: approved ? 'approved' : 'refused' } : j))
              );
              pushActivity('judge', approved ? 'Confirmation approuvée' : 'Confirmation refusée ou abandonnée');
              break;
            }

            case 'context': {
              const { type: _type, ...info } = data;
              setContext(info);
              break;
            }

            case 'todos':
              setTodos(data.items);
              break;

            case 'clients':
              setClients(data.items);
              break;

            case 'telemetry': {
              const { type: _type, ...info } = data;
              setTelemetry(info);
              break;
            }

            case 'connectors':
              setConnectors(data.items);
              break;

            case 'tool_approval': {
              const item: ToolApprovalItem = {
                id: data.id,
                server: data.server,
                tool: data.tool,
                arguments: data.arguments,
                timestamp: new Date(data.at_ms),
              };
              setToolApprovals((prev) => (prev.some((a) => a.id === item.id) ? prev : [...prev, item]));
              pushActivity('judge', `Accord demandé : ${data.server} · ${data.tool}`);
              break;
            }

            case 'tool_approval_resolved': {
              const { id, approved } = data;
              setToolApprovals((prev) => prev.filter((a) => a.id !== id));
              pushActivity('judge', approved ? 'Écriture externe approuvée' : 'Écriture externe refusée');
              break;
            }

            case 'settings':
              settingsRef.current = data.settings;
              setSettings(data.settings);
              break;

            case 'echo_discarded':
              pushNotice('Énoncé écarté : écho de la voix de Nestor');
              pushActivity('voice', `Écho écarté : « ${data.text} »`);
              break;

            case 'interrupt':
              if (typeof data.rms === 'number') {
                const rms = data.rms;
                setVoiceMeter((prev) => ({ ...prev, lastInterruptRms: rms }));
              }
              pushNotice('Interruption vocale détectée');
              pushActivity('voice', 'Interruption vocale, réponse abandonnée');
              // Interruption vocale detectee par le daemon : on coupe la lecture en cours.
              audioPlayerRef.current?.stop();
              playbackEndRef.current = 0;
              audioRecorderRef.current?.releaseHold();
              break;

            case 'audio_levels':
              if (typeof data.vad === 'number') {
                const vad = data.vad;
                setVoiceMeter((prev) => ({ ...prev, rms: data.rms, vad }));
              }
              // If not actively playing or recording locally, use server's levels
              if (!audioRecorderRef.current?.isActive()) {
                setAudioLevels({
                  rms: typeof data.rms === 'number' ? data.rms : 0,
                  peak: typeof data.peak === 'number' ? data.peak : 0,
                });
              }
              break;

            case 'audio_chunk': {
              if (isSpeakerActive && audioPlayerRef.current) {
                hasReceivedAudioChunkRef.current = true;
                // Fin de lecture estimee (les chunks se jouent a la suite) : micro continu jusque-la.
                const rate = data.sample_rate || 24000;
                const chunkMs = ((data.data.length * 3) / 4 / 2 / rate) * 1000;
                playbackEndRef.current = Math.max(playbackEndRef.current, Date.now()) + chunkMs;
                audioRecorderRef.current?.holdOpen(playbackEndRef.current + 1500);
                audioPlayerRef.current.enqueueChunk(
                  data.data,
                  data.format || 'wav',
                  data.sample_rate || 24000
                );
              }
              break;
            }

            case 'transcript': {
              const { role, delta, text, is_final } = data;
              setMessages((prev) => {
                const now = new Date();
                const last = prev[prev.length - 1];

                if (role === 'user') {
                  // If last message was interim user message, update it
                  if (last && last.role === 'user' && !last.isFinal) {
                    const updated = [...prev];
                    updated[updated.length - 1] = {
                      ...last,
                      text: text || last.text + (delta || ''),
                      isFinal: is_final ?? true,
                      timestamp: now,
                    };
                    return updated;
                  }
                  // Avoid duplicate if same user text echoed
                  if (last && last.role === 'user' && last.text === text) {
                    return prev;
                  }
                  // Otherwise new user message
                  return [
                    ...prev,
                    {
                      id: `user-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`,
                      role: 'user',
                      text: text || delta || '',
                      isFinal: is_final ?? true,
                      timestamp: now,
                    },
                  ];
                } else {
                  // Assistant message
                  if (last && last.role === 'assistant' && last.isStreaming) {
                    const newText = delta ? last.text + delta : text || last.text;
                    const updated = [...prev];
                    updated[updated.length - 1] = {
                      ...last,
                      text: newText,
                      isStreaming: is_final === false,
                      timestamp: now,
                    };

                    // If final message and browser TTS is enabled and no Kokoro audio chunk received
                    if (
                      is_final &&
                      isSpeakerActive &&
                      isBrowserTtsEnabled &&
                      !hasReceivedAudioChunkRef.current &&
                      audioPlayerRef.current
                    ) {
                      audioPlayerRef.current.speakBrowserFallback(newText);
                    }

                    return updated;
                  }

                  // New assistant message
                  hasReceivedAudioChunkRef.current = false;
                  return [
                    ...prev,
                    {
                      id: `assistant-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`,
                      role: 'assistant',
                      text: text || delta || '',
                      isStreaming: is_final === false,
                      timestamp: now,
                    },
                  ];
                }
              });
              break;
            }

            case 'tool_call': {
              const { name, input, status: toolStatus, mission_id: missionId } = data;
              if (toolStatus === 'running') {
                pushActivity('tool', `Outil appelé : ${name}${missionId !== undefined ? ` (mission ${missionId})` : ''}`);
              }
              setToolCalls((prev) => {
                const existingIndex = prev.findIndex(
                  (t) => t.name === name && t.status === 'running' && t.missionId === missionId
                );

                if (existingIndex !== -1 && toolStatus === 'completed') {
                  const updated = [...prev];
                  updated[existingIndex] = {
                    ...updated[existingIndex],
                    status: 'completed',
                    completedAt: new Date(),
                  };
                  return updated;
                }

                const newTool: ToolCallItem = {
                  id: `tool-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`,
                  name,
                  input,
                  status: toolStatus,
                  startedAt: new Date(),
                  completedAt: toolStatus === 'completed' ? new Date() : undefined,
                  missionId,
                };
                return [newTool, ...prev];
              });
              break;
            }

            case 'mission': {
              const { id, backend, status: missionStatus, description, summary, progress } = data;
              if (!progress || missionStatus !== 'started') {
                const labels: Record<string, string> = { started: 'lancée', completed: 'terminée', failed: 'en échec', cancelled: 'annulée' };
                pushActivity('mission', `Mission ${id} ${labels[missionStatus] ?? missionStatus}`);
              }
              setMissions((prev) => {
                const existing = prev.find((m) => m.id === id);
                if (!existing) {
                  const item: MissionItem = {
                    id,
                    backend,
                    description,
                    status: missionStatus,
                    summary,
                    progress,
                    startedAt: new Date(),
                    endedAt: missionStatus === 'started' ? undefined : new Date(),
                  };
                  return [item, ...prev];
                }
                return prev.map((m) =>
                  m.id === id
                    ? {
                        ...m,
                        status: missionStatus,
                        summary: summary ?? m.summary,
                        progress: progress ?? m.progress,
                        endedAt: missionStatus === 'started' ? m.endedAt : new Date(),
                      }
                    : m
                );
              });

              // Filet de securite : une mission terminee, annulee ou en echec
              // ne peut plus avoir d'outil en cours. Le backend les referme
              // deja, ceci couvre le cas ou son evenement serait perdu.
              if (missionStatus !== 'started') {
                setToolCalls((prev) =>
                  prev.map((t) =>
                    t.missionId === id && t.status === 'running'
                      ? { ...t, status: 'completed', completedAt: new Date() }
                      : t
                  )
                );
              }
              break;
            }

            case 'usage': {
              setUsage({
                fiveHour: data.five_hour,
                sevenDay: data.seven_day,
                resetsAt: data.resets_at,
              });
              break;
            }

            case 'backend_status': {
              setBackendStatus({
                active_backend: data.active_backend,
                is_fallback: data.is_fallback,
                reason: data.reason ?? null,
              });
              break;
            }
          }
        } catch (err) {
          console.error('[Nestor WS] Failed to parse message:', err, event.data);
        }
      };

      ws.onerror = () => {
        if (socketRef.current !== ws) return;
        setConnectionState('error');
      };

      ws.onclose = () => {
        // Idem : un socket deja remplace ne doit ni changer l'etat affiche ni
        // declencher une reconnexion (sinon on empile les connexions).
        if (socketRef.current !== ws) return;

        // Une seule ligne par coupure, pas une par tentative de reconnexion.
        setConnectionState('disconnected');
        if (wasConnectedRef.current) {
          wasConnectedRef.current = false;
          pushActivity('system', 'Connexion au daemon perdue');
        }
        setClients([]);
        setToolApprovals([]);
        lastWakeRef.current = null;
        socketRef.current = null;
        if (!isExplicitCloseRef.current && autoReconnect && !isSimulated) {
          reconnectTimeoutRef.current = window.setTimeout(
            () => connectRef.current(),
            reconnectInterval
          );
        }
      };
    } catch (e) {
      console.error('[Nestor WS] Error during setup:', e);
      setConnectionState('error');
      if (autoReconnect && !isSimulated) {
        reconnectTimeoutRef.current = window.setTimeout(
          () => connectRef.current(),
          reconnectInterval
        );
      }
    }
  }, [url, autoReconnect, reconnectInterval, isSimulated, isSpeakerActive, isBrowserTtsEnabled]);

  // Keep connectRef fresh
  useEffect(() => {
    connectRef.current = connect;
  }, [connect]);

  // Connect on mount
  useEffect(() => {
    isExplicitCloseRef.current = false;
    connect();

    return () => {
      isExplicitCloseRef.current = true;
      if (reconnectTimeoutRef.current) clearTimeout(reconnectTimeoutRef.current);
      if (socketRef.current) {
        // Detacher les handlers avant la fermeture : le socket peut survivre
        // le temps du handshake de fermeture et ne doit plus rien emettre.
        const closing = socketRef.current;
        socketRef.current = null;
        closing.onopen = null;
        closing.onmessage = null;
        closing.onerror = null;
        closing.onclose = null;
        closing.close();
      }
      if (audioRecorderRef.current) {
        audioRecorderRef.current.stop();
      }
    };
  }, [connect]);

  // Toggle Microphone
  const toggleMic = useCallback(async () => {
    if (isMicActive) {
      if (audioRecorderRef.current) {
        audioRecorderRef.current.stop();
        audioRecorderRef.current = null;
      }
      setIsMicActive(false);
      setStatus('idle');
      setAudioLevels({ rms: 0, peak: 0 });
    } else {
      try {
        const recorder = new AudioRecorder({
          sampleRate: 16000,
          onAudioChunk: (chunkBuffer, pcm16Base64) => {
            // Le micro reste envoye pendant que Nestor parle : c'est ce qui permet de
            // l'interrompre a la voix. L'echo est traite par le daemon (annulation
            // d'echo avec reference, filtre d'auto-ecoute).

            // 1. Send binary frame (high performance)
            sendBinaryAudio(chunkBuffer);
            // 2. Also send JSON event for backends expecting JSON
            sendEvent({
              type: 'audio_in',
              pcm_base64: pcm16Base64,
              sample_rate: 16000,
            });
          },
          onLevels: (rms, peak) => {
            setAudioLevels({ rms, peak });
            if (rms > 0.05) {
              setStatus((prev) => (prev === 'idle' ? 'listening' : prev));
            }
          },
          onError: (err) => {
            console.error('[AudioRecorder Error]:', err);
            setIsMicActive(false);
          },
        });

        await recorder.start();
        audioRecorderRef.current = recorder;
        setIsMicActive(true);
        setStatus('listening');
      } catch (err) {
        console.error('Failed to enable mic:', err);
        alert('Impossible d’accéder au microphone dans ce navigateur. Vérifiez les autorisations.');
      }
    }
  }, [isMicActive, sendBinaryAudio, sendEvent]);

  // Annulation d'une mission en cours
  const stopMission = useCallback(
    (id: number, reason?: string) => {
      sendEvent({ type: 'stop_mission', id, reason });
    },
    [sendEvent]
  );

  // Reglages : applique tout de suite a l'ecran, le daemon confirme par un evenement `settings`.
  const updateSettings = useCallback(
    (patch: Partial<NestorSettings>) => {
      const prev = settingsRef.current;
      if (!prev) return;
      const next = { ...prev, ...patch };
      settingsRef.current = next;
      setSettings(next);
      sendEvent({ type: 'update_settings', settings: next });
    },
    [sendEvent]
  );

  // Taches : le daemon rediffuse la liste apres chaque changement.
  const addTodo = useCallback(
    (title: string, options?: { dueAt?: string; recurrence?: string }) => {
      if (!title.trim()) return;
      sendEvent({ type: 'todo_add', title: title.trim(), due_at: options?.dueAt, recurrence: options?.recurrence });
    },
    [sendEvent]
  );
  const completeTodo = useCallback((id: number) => sendEvent({ type: 'todo_complete', id }), [sendEvent]);
  const deleteTodo = useCallback((id: number) => sendEvent({ type: 'todo_delete', id }), [sendEvent]);

  // Jeton d'acces saisi dans les reglages : memorise, puis reconnexion avec ce jeton.
  const [authToken, setAuthTokenState] = useState(() => readAuthToken());
  const setAuthToken = useCallback((token: string) => {
    const trimmed = token.trim();
    try {
      if (trimmed) window.localStorage.setItem(TOKEN_STORAGE_KEY, trimmed);
      else window.localStorage.removeItem(TOKEN_STORAGE_KEY);
    } catch {
      // stockage indisponible : le jeton ne vaudra que pour cette connexion
    }
    setAuthTokenState(trimmed);
    // Ferme la connexion en cours : la reconnexion automatique reprend avec le nouveau jeton.
    if (socketRef.current) socketRef.current.close();
    else connectRef.current();
  }, []);

  // Session ouverte par passkey : memorisee pour l'onglet, puis reconnexion.
  const setSessionToken = useCallback((token: string) => {
    try {
      window.sessionStorage.setItem(SESSION_STORAGE_KEY, token);
    } catch {
      // stockage indisponible : la session ne vaudra que jusqu'au rechargement
    }
    if (socketRef.current) socketRef.current.close();
    else connectRef.current();
  }, []);

  // Connecteurs : mode d'un outil externe, et accord ou refus d'une ecriture en attente.
  const setToolMode = useCallback(
    (server: string, tool: string, mode: ToolMode) => sendEvent({ type: 'set_tool_mode', server, tool, mode }),
    [sendEvent]
  );
  const resolveToolApproval = useCallback(
    (id: number, approve: boolean) => sendEvent({ type: 'resolve_tool_approval', id, approve }),
    [sendEvent]
  );

  // Reponse, depuis l'UI, a une confirmation demandee par le juge.
  const resolveJudgement = useCallback(
    (id: number, approve: boolean) => {
      sendEvent({ type: 'resolve_judgement', id, approve });
    },
    [sendEvent]
  );

  // Barge-in (Interruption immédiate)
  const sendBargeIn = useCallback(() => {
    // 1. Stop web audio output immediately
    if (audioPlayerRef.current) {
      audioPlayerRef.current.stop();
    }
    playbackEndRef.current = 0;
    audioRecorderRef.current?.releaseHold();

    // 2. Send barge_in event to backend
    const sent = sendEvent({ type: 'barge_in' });
    if (isSimulated || !sent) {
      setStatus('idle');
      setAudioLevels({ rms: 0, peak: 0 });
    }
  }, [sendEvent, isSimulated]);

  // Send text
  const sendText = useCallback(
    (content: string) => {
      if (!content.trim()) return;

      sendEvent({ type: 'send_text', content: content.trim() });

      // Add optimistic user message
      setMessages((prev) => [
        ...prev,
        {
          id: `user-${Date.now()}`,
          role: 'user',
          text: content.trim(),
          isFinal: true,
          timestamp: new Date(),
        },
      ]);

      if (isSimulated) {
        setStatus('thinking');
        setTimeout(() => {
          setStatus('speaking');
          const response = `Bien reçu, monsieur. Je m'en occupe immédiatement.`;

          setMessages((prev) => [
            ...prev,
            {
              id: `assistant-${Date.now()}`,
              role: 'assistant',
              text: response,
              isStreaming: false,
              timestamp: new Date(),
            },
          ]);

          if (isSpeakerActive && audioPlayerRef.current) {
            audioPlayerRef.current.speakBrowserFallback(response);
          }

          setTimeout(() => {
            setStatus('idle');
          }, 2500);
        }, 1200);
      }
    },
    [sendEvent, isSimulated, isSpeakerActive]
  );

  const setBackend = useCallback(
    (backend: string) => {
      sendEvent({ type: 'set_backend', backend });
    },
    [sendEvent]
  );

  return {
    connectionState,
    status,
    setStatus,
    audioLevels,
    setAudioLevels,
    messages,
    toolCalls,
    missions,
    usage,
    backendStatus,
    setBackend,
    stopMission,
    sendBargeIn,
    sendText,
    isSimulated,
    setIsSimulated,
    isMicActive,
    isWakeActive,
    toggleMic,
    isSpeakerActive,
    toggleSpeaker: () => setIsSpeakerActive(!isSpeakerActive),
    isBrowserTtsEnabled,
    setIsBrowserTtsEnabled,
    settings,
    updateSettings,
    voiceMeter,
    judgements,
    resolveJudgement,
    activity,
    context,
    todos,
    addTodo,
    completeTodo,
    deleteTodo,
    clients,
    telemetry,
    connectors,
    setToolMode,
    toolApprovals,
    resolveToolApproval,
    authToken,
    setAuthToken,
    setSessionToken,
  };
}
