export type DaemonStatus = 'listening' | 'thinking' | 'speaking' | 'idle';

export type Role = 'user' | 'assistant';

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
      type: 'wake_state';
      active: boolean;
    }
  | {
      type: 'audio_levels';
      rms: number;
      peak: number;
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
