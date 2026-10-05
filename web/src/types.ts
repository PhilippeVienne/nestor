export type DaemonStatus = 'listening' | 'thinking' | 'speaking' | 'idle';

/** `notice` : evenement du pipeline affiche dans le fil (interruption, echo ecarte). */
export type Role = 'user' | 'assistant' | 'notice';

/** Reglages modifiables a chaud (cf. nestord/src/settings.rs). */
export interface NestorSettings {
  voice_barge_in: boolean;
  aec: boolean;
  barge_threshold: number;
  barge_min_speech_ms: number;
  barge_min_rms: number;
  smart_turn: boolean;
  wake_word_enabled: boolean;
  wake_timeout_secs: number;
  judge_model: string;
  judge_confirm_threshold: number;
  judge_reject_threshold: number;
}

/** Mesures audio en direct pour le bloc Voix. */
export interface VoiceMeter {
  /** Niveau du micro recu par le daemon. */
  rms: number;
  /** Probabilite de parole (VAD). */
  vad: number;
  /** Niveau moyen de la derniere interruption vocale detectee. */
  lastInterruptRms?: number;
}

export type ToolCallStatus = 'running' | 'completed';

export type MissionStatus = 'started' | 'completed' | 'failed' | 'cancelled';

/** Backend d'execution d'une mission deleguee. */
export type MissionBackend = 'claude' | 'agy' | string;

export interface AudioLevels {
  rms: number;
  peak: number;
}

export type ServerEvent =
  | {
      type: 'state';
      status: DaemonStatus;
    }
  | {
      type: 'interrupt';
      /** Niveau moyen (RMS) de la parole qui a declenche l'interruption. */
      rms?: number;
    }
  | {
      type: 'echo_discarded';
      text: string;
    }
  | {
      type: 'settings';
      settings: NestorSettings;
    }
  | {
      type: 'wake_state';
      active: boolean;
    }
  | {
      type: 'audio_levels';
      rms: number;
      peak: number;
      /** Probabilite de parole (VAD) vue par le daemon. */
      vad?: number;
    }
  | {
      type: 'transcript';
      role: Role;
      delta?: string;
      text: string;
      is_final?: boolean;
    }
  | {
      type: 'tool_call';
      name: string;
      input: Record<string, unknown> | unknown;
      status: ToolCallStatus;
      /** Present quand l'outil est execute par un sous-agent de mission. */
      mission_id?: number;
    }
  | {
      type: 'mission';
      id: number;
      backend: MissionBackend;
      status: MissionStatus;
      description: string;
      summary?: string;
      /** Derniere activite du sous-agent (outil en cours, etc.). */
      progress?: string;
    }
  | {
      type: 'usage';
      five_hour: number;
      seven_day: number;
      resets_at?: number;
    }
  | {
      type: 'audio_chunk';
      data: string; // Base64 encoded audio (WAV / PCM)
      format?: 'wav' | 'pcm16' | 'pcm';
      sample_rate?: number;
      is_final?: boolean;
    }
  | {
      type: 'backend_status';
      active_backend: 'claude' | 'agy' | string;
      is_fallback: boolean;
      reason?: string | null;
    };

export interface BackendStatusInfo {
  active_backend: string;
  is_fallback: boolean;
  reason?: string | null;
}

export type ClientEvent =
  | {
      type: 'barge_in';
    }
  | {
      type: 'send_text';
      content: string;
    }
  | {
      type: 'audio_in';
      pcm_base64: string;
      sample_rate?: number;
    }
  | {
      type: 'stop_mission';
      id: number;
      reason?: string;
    }
  | {
      type: 'update_settings';
      settings: NestorSettings;
    }
  | {
      type: 'set_backend';
      backend: 'claude' | 'agy' | 'auto' | string;
    }
  | {
      type: 'location';
      lat: number;
      lon: number;
    };

export interface MessageItem {
  id: string;
  role: Role;
  text: string;
  isFinal?: boolean;
  isStreaming?: boolean;
  timestamp: Date;
}

export interface ToolCallItem {
  id: string;
  name: string;
  input: unknown;
  status: ToolCallStatus;
  startedAt: Date;
  completedAt?: Date;
  /** Absent pour la session conversationnelle, defini pour un sous-agent. */
  missionId?: number;
}

export interface MissionItem {
  id: number;
  backend: MissionBackend;
  description: string;
  status: MissionStatus;
  summary?: string;
  /** Derniere activite connue, pour suivre l'avancement en direct. */
  progress?: string;
  startedAt: Date;
  endedAt?: Date;
}

export interface UsageInfo {
  fiveHour: number;
  sevenDay: number;
  resetsAt?: number;
}

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'error';
