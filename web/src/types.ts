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

/** Contexte tenu par nestord (panneau « Situation »). */
export interface ContextInfo {
  place?: string;
  quiet_start: string;
  quiet_end: string;
  quiet_active: boolean;
  auth_required: boolean;
}

/** Tache ou rappel (cf. nestord/src/todo.rs). */
export interface TodoItem {
  id: number;
  title: string;
  notes?: string | null;
  status: string;
  /** `daily`, `weekly:<jour>` ou `monthly:<1-31>`. */
  recurrence?: string | null;
  /** Echeance en secondes (epoch), pour une tache ponctuelle. */
  due_at?: number | null;
  created_at: number;
}

export interface ClientInfo {
  id: number;
  kind: 'web' | 'mobile' | 'autre' | string;
  connected_at_ms: number;
}

export interface GpuInfo {
  name: string;
  memory_used_mb: number;
  memory_total_mb: number;
}

export interface TelemetryInfo {
  stt_model?: string;
  tts_voice?: string;
  judge_model: string;
  gpu?: GpuInfo;
  stt_ms?: number;
  first_word_ms?: number;
  tts_ms?: number;
}

export type ToolMode = 'read' | 'confirm' | 'off';

export interface ConnectorTool {
  name: string;
  description?: string;
  /** Mode applique (absent pour les outils internes de nestord). */
  mode?: ToolMode;
  /** Mode sans reglage : lecture libre si le serveur l'annonce en lecture seule. */
  default_mode?: ToolMode;
}

export interface ConnectorInfo {
  name: string;
  kind: 'interne' | 'externe' | string;
  status: 'connected' | 'connecting' | 'error' | 'disabled' | string;
  detail?: string;
  tools: ConnectorTool[];
}

/** Ecriture d'un outil externe en attente de l'accord de l'utilisateur. */
export interface ToolApprovalItem {
  id: number;
  server: string;
  tool: string;
  arguments: string;
  timestamp: Date;
}

export type JudgeDecision = 'allow' | 'confirm' | 'deny';

/** Decision du juge de conscience (panneau « Conscience »). */
export interface JudgeItem {
  id: number;
  source: string;
  text: string;
  decision: JudgeDecision;
  score?: number;
  category?: string;
  rationale?: string;
  /** Une confirmation de l'utilisateur est attendue. */
  pending: boolean;
  /** Issue d'une confirmation tranchee : approuvee ou non. */
  resolved?: 'approved' | 'refused';
  timestamp: Date;
}

/** Ligne du journal d'activite. */
export interface ActivityItem {
  id: string;
  kind: 'wake' | 'tool' | 'mission' | 'voice' | 'judge' | 'system';
  text: string;
  timestamp: Date;
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
  | ({ type: 'context' } & ContextInfo)
  | {
      type: 'todos';
      items: TodoItem[];
    }
  | {
      type: 'clients';
      items: ClientInfo[];
    }
  | ({ type: 'telemetry' } & TelemetryInfo)
  | {
      type: 'connectors';
      items: ConnectorInfo[];
    }
  | {
      type: 'tool_approval';
      id: number;
      server: string;
      tool: string;
      arguments: string;
      at_ms: number;
    }
  | {
      type: 'tool_approval_resolved';
      id: number;
      approved: boolean;
    }
  | {
      type: 'judge_verdict';
      id: number;
      source: 'message' | 'mission' | string;
      text: string;
      decision: JudgeDecision;
      score?: number;
      category?: string;
      rationale?: string;
      pending: boolean;
      /** Horodatage de la decision (epoch ms). */
      at_ms?: number;
    }
  | {
      type: 'judge_resolved';
      id: number;
      approved: boolean;
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
      type: 'todo_add';
      title: string;
      due_at?: string;
      recurrence?: string;
    }
  | {
      type: 'todo_complete';
      id: number;
    }
  | {
      type: 'todo_delete';
      id: number;
    }
  | {
      type: 'set_tool_mode';
      server: string;
      tool: string;
      mode: ToolMode;
    }
  | {
      type: 'resolve_tool_approval';
      id: number;
      approve: boolean;
    }
  | {
      type: 'resolve_judgement';
      id: number;
      approve: boolean;
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
