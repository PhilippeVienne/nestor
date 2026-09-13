import { useState, useEffect, useRef, useCallback } from 'react';
import {
  NestorCall,
  CallState,
  NestorState,
  AudioRoute,
  TranscriptEvent,
  ToolCallEvent,
  BackendStatusEvent,
} from '../native/NestorCall';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  time: string;
}

export function useNestorCall() {
  const [callState, setCallState] = useState<CallState>('IDLE');
  const [callStatusDetails, setCallStatusDetails] = useState<string>('');
  const [nestorState, setNestorState] = useState<NestorState>('idle');
  const [audioRoute, setAudioRoute] = useState<AudioRoute>('SPEAKER');
  const [isMuted, setIsMuted] = useState<boolean>(false);
  const [isSpeakerOn, setIsSpeakerOn] = useState<boolean>(true);
  const [audioLevels, setAudioLevels] = useState<{ rms: number; peak: number }>({ rms: 0, peak: 0 });
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [activeTools, setActiveTools] = useState<ToolCallEvent[]>([]);
  const [callDurationSeconds, setCallDurationSeconds] = useState<number>(0);
  const [backendStatus, setBackendStatus] = useState<BackendStatusEvent>({
    active_backend: 'claude',
    is_fallback: false,
  });

  const durationTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // In-call duration timer
  useEffect(() => {
    if (callState === 'ACTIVE') {
      setCallDurationSeconds(0);
      durationTimerRef.current = setInterval(() => {
        setCallDurationSeconds((prev) => prev + 1);
      }, 1000);
    } else {
      if (durationTimerRef.current) {
        clearInterval(durationTimerRef.current);
        durationTimerRef.current = null;
      }
    }

    return () => {
      if (durationTimerRef.current) {
        clearInterval(durationTimerRef.current);
      }
    };
  }, [callState]);

  // Subscribe to native events
  useEffect(() => {
    const subCallState = NestorCall.onCallStateChanged((e) => {
      console.log('[NestorCall] CallState:', e.state, e.details);
      setCallState(e.state);
      if (e.details) setCallStatusDetails(e.details);
    });

    const subNestorState = NestorCall.onNestorStateChanged((e) => {
      setNestorState(e.state);
    });

    const subAudioRoute = NestorCall.onAudioRouteChanged((e) => {
      setAudioRoute(e.route);
      setIsMuted(e.isMuted);
      setIsSpeakerOn(e.route === 'SPEAKER');
    });

    const subAudioLevels = NestorCall.onAudioLevels((e) => {
      setAudioLevels(e);
    });

    const subTranscript = NestorCall.onTranscript((e: TranscriptEvent) => {
      if (!e.text.trim()) return;

      const now = new Date();
      const timeStr = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`;

      setMessages((prev) => {
        if (e.isPartial && prev.length > 0 && prev[prev.length - 1].role === e.role) {
          // Update last partial message
          const updated = [...prev];
          updated[updated.length - 1] = {
            ...updated[updated.length - 1],
            text: e.text,
          };
          return updated;
        }

        // Avoid exact immediate echo duplicate
        if (prev.length > 0 && prev[prev.length - 1].text === e.text) {
          return prev;
        }

        return [
          ...prev,
          {
            id: `msg-${Date.now()}-${Math.random()}`,
            role: e.role,
            text: e.text,
            time: timeStr,
          },
        ];
      });
    });

    const subToolCall = NestorCall.onToolCall((e: ToolCallEvent) => {
      setActiveTools((prev) => {
        const index = prev.findIndex((t) => t.id === e.id);
        if (index >= 0) {
          const updated = [...prev];
          updated[index] = e;
          return updated;
        }
        return [...prev, e];
      });
    });

    const subBackend = NestorCall.onBackendStatus((e: BackendStatusEvent) => {
      console.log('[NestorCall] BackendStatus:', e);
      setBackendStatus(e);
    });

    return () => {
      subCallState?.remove();
      subNestorState?.remove();
      subAudioRoute?.remove();
      subAudioLevels?.remove();
      subTranscript?.remove();
      subToolCall?.remove();
      subBackend?.remove();
    };
  }, []);

  const formatDuration = useCallback((seconds: number) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }, []);

  const startCall = useCallback(async (serverUrl?: string) => {
    setCallState('CONNECTING');
    setCallStatusDetails('Connexion à Nestor...');
    setMessages([]);
    setActiveTools([]);
    await NestorCall.startCall(serverUrl);
  }, []);

  const endCall = useCallback(async () => {
    await NestorCall.endCall();
    setCallState('IDLE');
    setNestorState('idle');
  }, []);

  const toggleMute = useCallback(async () => {
    const nextMuted = !isMuted;
    setIsMuted(nextMuted);
    await NestorCall.setMuted(nextMuted);
  }, [isMuted]);

  const toggleSpeaker = useCallback(async () => {
    const nextSpeaker = !isSpeakerOn;
    setIsSpeakerOn(nextSpeaker);
    await NestorCall.setSpeakerphoneOn(nextSpeaker);
  }, [isSpeakerOn]);

  const bargeIn = useCallback(async () => {
    await NestorCall.bargeIn();
  }, []);

  const sendTextMessage = useCallback(async (text: string) => {
    if (!text.trim()) return;
    await NestorCall.sendTextMessage(text);
  }, []);

  const setBackend = useCallback(async (backend: string) => {
    await NestorCall.setBackend(backend);
  }, []);

  return {
    callState,
    callStatusDetails,
    nestorState,
    audioRoute,
    isMuted,
    isSpeakerOn,
    audioLevels,
    messages,
    activeTools,
    backendStatus,
    setBackend,
    callDuration: formatDuration(callDurationSeconds),
    startCall,
    endCall,
    toggleMute,
    toggleSpeaker,
    bargeIn,
    sendTextMessage,
  };
}
