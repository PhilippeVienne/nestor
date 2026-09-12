import React, { useState } from 'react';
import {
  Rocket,
  Loader2,
  CheckCircle2,
  XCircle,
  ChevronDown,
  ChevronRight,
  Gauge,
  Ban,
  CircleSlash,
  Activity,
} from 'lucide-react';
import type { MissionItem, UsageInfo } from '../types';

interface MissionPanelProps {
  missions: MissionItem[];
  usage: UsageInfo | null;
  onStopMission?: (id: number, reason?: string) => void;
}

const BACKEND_STYLES: Record<string, string> = {
  claude: 'bg-violet-500/20 text-violet-300 border-violet-500/40',
  agy: 'bg-sky-500/20 text-sky-300 border-sky-500/40',
};

/** Jauge de quota : au-dela de 85 %, les missions basculent sur agy. */
const UsageGauge: React.FC<{ usage: UsageInfo }> = ({ usage }) => {
  const pct = Math.round(Math.max(usage.fiveHour, usage.sevenDay) * 100);
  const tone =
    pct >= 85 ? 'bg-rose-500' : pct >= 60 ? 'bg-amber-400' : 'bg-emerald-400';

  return (
    <div className="flex items-center gap-2" title={`Fenêtre 5 h : ${Math.round(usage.fiveHour * 100)} % — fenêtre 7 j : ${Math.round(usage.sevenDay * 100)} %`}>
      <Gauge className="w-3 h-3 text-slate-500 shrink-0" />
      <div className="h-1 w-16 rounded-full bg-slate-800 overflow-hidden">
        <div className={`h-full ${tone} transition-all`} style={{ width: `${Math.min(pct, 100)}%` }} />
      </div>
      <span className="text-[10px] font-mono text-slate-400 tabular-nums">{pct} %</span>
    </div>
  );
};

export const MissionPanel: React.FC<MissionPanelProps> = ({ missions, usage, onStopMission }) => {
  const [expandedIds, setExpandedIds] = useState<Record<number, boolean>>({});

  if (missions.length === 0 && !usage) return null;

  const runningCount = missions.filter((m) => m.status === 'started').length;

  return (
    <div className="border-b border-slate-800/50 bg-slate-900/20 shrink-0">
      <div className="flex items-center justify-between px-3 py-2">
        <div className="flex items-center gap-2">
          <Rocket className="w-3.5 h-3.5 text-fuchsia-400" />
          <span className="text-[11px] font-mono font-semibold tracking-wider uppercase text-slate-300">
            Missions
          </span>
          {runningCount > 0 && (
            <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-fuchsia-500/20 text-fuchsia-300 border border-fuchsia-500/40">
              {runningCount} en cours
            </span>
          )}
        </div>
        {usage && <UsageGauge usage={usage} />}
      </div>

      {missions.length > 0 && (
        <div className="max-h-56 overflow-y-auto px-3 pb-2 space-y-1.5">
          {missions.map((mission) => {
            const isExpanded = !!expandedIds[mission.id];
            const backendStyle =
              BACKEND_STYLES[mission.backend] ?? 'bg-slate-500/20 text-slate-300 border-slate-500/40';

            return (
              <div
                key={mission.id}
                className={`rounded-lg border p-2 text-xs font-mono transition-all ${
                  mission.status === 'started'
                    ? 'bg-fuchsia-950/20 border-fuchsia-500/40'
                    : mission.status === 'failed'
                    ? 'bg-rose-950/20 border-rose-500/40'
                    : mission.status === 'cancelled'
                    ? 'bg-slate-900/40 border-slate-700/60'
                    : 'bg-slate-900/60 border-slate-800/80'
                }`}
              >
                <div
                  className="flex items-center justify-between gap-2 cursor-pointer select-none"
                  onClick={() =>
                    setExpandedIds((prev) => ({ ...prev, [mission.id]: !prev[mission.id] }))
                  }
                >
                  <div className="flex items-center gap-2 overflow-hidden">
                    {mission.status === 'started' ? (
                      <Loader2 className="w-3.5 h-3.5 text-fuchsia-400 animate-spin shrink-0" />
                    ) : mission.status === 'failed' ? (
                      <XCircle className="w-3.5 h-3.5 text-rose-400 shrink-0" />
                    ) : mission.status === 'cancelled' ? (
                      <CircleSlash className="w-3.5 h-3.5 text-slate-500 shrink-0" />
                    ) : (
                      <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 shrink-0" />
                    )}
                    <span className="text-slate-500 shrink-0">#{mission.id}</span>
                    <span className={`px-1.5 py-0.5 rounded text-[9px] border shrink-0 ${backendStyle}`}>
                      {mission.backend}
                    </span>
                    <span className="text-slate-300 truncate">{mission.description}</span>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    {mission.status === 'started' && onStopMission && (
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onStopMission(mission.id);
                        }}
                        className="text-slate-500 hover:text-rose-400 p-0.5 transition-colors"
                        title="Annuler cette mission"
                      >
                        <Ban className="w-3.5 h-3.5" />
                      </button>
                    )}
                    {isExpanded ? (
                      <ChevronDown className="w-3.5 h-3.5 text-slate-400" />
                    ) : (
                      <ChevronRight className="w-3.5 h-3.5 text-slate-400" />
                    )}
                  </div>
                </div>

                {mission.status === 'started' && mission.progress && (
                  <div className="mt-1.5 flex items-center gap-1.5 text-[10px] text-fuchsia-200/70 overflow-hidden">
                    <Activity className="w-3 h-3 shrink-0 animate-pulse" />
                    <span className="truncate" title={mission.progress}>
                      {mission.progress}
                    </span>
                  </div>
                )}

                {isExpanded && (
                  <div className="mt-2 pt-2 border-t border-white/5 space-y-2">
                    <p className="text-[11px] text-slate-400 font-sans whitespace-pre-wrap">
                      {mission.description}
                    </p>
                    {mission.progress && (
                      <div className="text-[10px] text-slate-400 font-sans">
                        Derniere activite : <span className="text-slate-300">{mission.progress}</span>
                      </div>
                    )}
                    {mission.summary && (
                      <div className="space-y-1">
                        <div className="text-[10px] text-slate-400 font-sans uppercase tracking-wider">
                          {mission.status === 'cancelled' ? 'Motif et travail partiel :' : 'Compte rendu :'}
                        </div>
                        <pre
                          className={`text-[11px] p-2 rounded-lg bg-black/50 overflow-x-auto border border-white/5 max-h-48 whitespace-pre-wrap ${
                            mission.status === 'cancelled'
                              ? 'text-amber-200'
                              : mission.status === 'failed'
                              ? 'text-rose-300'
                              : 'text-emerald-300'
                          }`}
                        >
                          {mission.summary}
                        </pre>
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
