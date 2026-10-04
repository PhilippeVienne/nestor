import { NativeModules, NativeEventEmitter, Platform } from 'react-native';

const { NestorCallModule } = NativeModules;

/**
 * Ajoute le jeton d'authentification en query param, sauf si l'URL en porte
 * deja un (URL complete fournie par `nestord onboard`). Priorite : jeton saisi
 * dans l'app, puis `EXPO_PUBLIC_NESTOR_TOKEN`. Sans jeton, nestord n'en exige
 * que si `auth_token` est configure cote serveur (cf. `nestord/src/config.rs`).
 */
function withAuthToken(url: string, token?: string): string {
  if (/[?&]token=/.test(url)) return url;
  const effective = token?.trim() || process.env.EXPO_PUBLIC_NESTOR_TOKEN;
  if (!effective) return url;
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}token=${encodeURIComponent(effective)}`;
}

export type CallState = 'IDLE' | 'CONNECTING' | 'ACTIVE' | 'ENDED' | 'ERROR';
export type NestorState = 'idle' | 'listening' | 'thinking' | 'speaking';
export type AudioRoute = 'EARPIECE' | 'SPEAKER' | 'BLUETOOTH' | 'WIRED_HEADSET' | 'UNKNOWN';

export interface CallStateEvent {
  state: CallState;
  details?: string;
}

export interface NestorStateEvent {
  state: NestorState;
}

export interface AudioRouteEvent {
  route: AudioRoute;
  isMuted: boolean;
}

export interface AudioLevelsEvent {
  rms: number;
  peak: number;
}

export interface TranscriptEvent {
  role: 'user' | 'assistant';
  text: string;
  isPartial: boolean;
}

export interface ToolCallEvent {
  id: string;
  name: string;
  status: 'running' | 'completed' | 'failed';
}

export interface BackendStatusEvent {
  active_backend: 'claude' | 'agy' | string;
  is_fallback: boolean;
  reason?: string;
}

const callEmitter = NestorCallModule ? new NativeEventEmitter(NestorCallModule) : null;

export const NestorCall = {
  isAvailable(): boolean {
    return Platform.OS === 'android' && !!NestorCallModule;
  },

  async setBackend(backend: string): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.setBackend(backend);
  },

  /**
   * Demande RECORD_AUDIO et la localisation. Retourne `true` si RECORD_AUDIO
   * est accorde (la localisation est facultative, cf. reconnaissance de lieu
   * cote nestord).
   */
  async requestPermissions(): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.requestPermissions();
  },

  async startCall(serverUrl: string = 'ws://10.0.2.2:8340/ws', token?: string): Promise<boolean> {
    if (!NestorCallModule) {
      console.warn('[NestorCall] Native module not available');
      return false;
    }
    return NestorCallModule.startCall({ serverUrl: withAuthToken(serverUrl, token) });
  },

  async endCall(): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.endCall();
  },

  async setMuted(muted: boolean): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.setMuted(muted);
  },

  async setSpeakerphoneOn(speakerOn: boolean): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.setSpeakerphoneOn(speakerOn);
  },

  async bargeIn(): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.bargeIn();
  },

  async sendTextMessage(text: string): Promise<boolean> {
    if (!NestorCallModule) return false;
    return NestorCallModule.sendTextMessage(text);
  },

  async getCallState(): Promise<{ isCallActive: boolean; isMicMuted: boolean; serverUrl: string }> {
    if (!NestorCallModule) {
      return { isCallActive: false, isMicMuted: false, serverUrl: '' };
    }
    return NestorCallModule.getCallState();
  },

  // Event Listeners
  onCallStateChanged(listener: (event: CallStateEvent) => void) {
    return callEmitter?.addListener('onCallStateChanged', listener);
  },

  onNestorStateChanged(listener: (event: NestorStateEvent) => void) {
    return callEmitter?.addListener('onNestorStateChanged', listener);
  },

  onAudioRouteChanged(listener: (event: AudioRouteEvent) => void) {
    return callEmitter?.addListener('onAudioRouteChanged', listener);
  },

  onAudioLevels(listener: (event: AudioLevelsEvent) => void) {
    return callEmitter?.addListener('onAudioLevels', listener);
  },

  onTranscript(listener: (event: TranscriptEvent) => void) {
    return callEmitter?.addListener('onTranscript', listener);
  },

  onToolCall(listener: (event: ToolCallEvent) => void) {
    return callEmitter?.addListener('onToolCall', listener);
  },

  onBackendStatus(listener: (event: BackendStatusEvent) => void) {
    return callEmitter?.addListener('onBackendStatusChanged', listener);
  }
};
