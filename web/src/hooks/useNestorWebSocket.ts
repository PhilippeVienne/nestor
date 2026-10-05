import { useState, useEffect, useRef, useCallback } from 'react';
import type {
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
function withAuthToken(url: string): string {
  const token = import.meta.env.VITE_NESTOR_TOKEN as string | undefined;
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
  url = 'ws://127.0.0.1:8340/ws',
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
  const hasReceivedAudioChunkRef = useRef(false);
  // Usage typique : micro + haut-parleurs. Pendant la lecture, le micro
  // reentend la synthese ; si on l'envoie au backend, elle est transcrite et
  // renvoyee a Claude comme un nouveau message, ce qui coupe sa reponse.
  const isPlayingRef = useRef(false);

  // Initialize AudioPlayer on mount
  useEffect(() => {
    audioPlayerRef.current = new AudioPlayer({
      onPlaybackLevels: (rms, peak) => {
        setAudioLevels({ rms, peak });
      },
      onPlaybackStateChange: (isPlaying) => {
        isPlayingRef.current = isPlaying;
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
              break;

            case 'interrupt':
              // Interruption vocale detectee par le daemon : on coupe la lecture en cours.
              audioPlayerRef.current?.stop();
              break;

            case 'audio_levels':
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

        setConnectionState('disconnected');
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
            // Ne rien envoyer pendant que Nestor parle : ce serait son propre
            // echo (micro + haut-parleurs), transcrit puis renvoye a Claude.
            if (isPlayingRef.current) return;

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

  // Barge-in (Interruption immédiate)
  const sendBargeIn = useCallback(() => {
    // 1. Stop web audio output immediately
    if (audioPlayerRef.current) {
      audioPlayerRef.current.stop();
    }

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
  };
}
