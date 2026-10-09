import React, { useState } from 'react';
import { Loader2, CheckCircle2, XCircle, ChevronDown, ChevronRight, Ban, CircleSlash, Activity } from 'lucide-react';
import type { MissionItem, UsageInfo } from '../types';

interface MissionPanelProps {
  missions: MissionItem[];
  usage: UsageInfo | null;
  onStopMission?: (id: number, reason?: string) => void;
  /** Sans en-tete : la carte qui l'accueille en porte deja un. */
  bare?: boolean;
}

/** Jauge de quota : au-dela de 85 %, les missions basculent sur agy. */
const UsageGauge: React.FC<{ usage: UsageInfo }> = ({ usage }) => {
  const pct = Math.round(Math.max(usage.fiveHour, usage.sevenDay) * 100);
  const tone = pct >= 85 ? 'bg-danger-400' : pct >= 60 ? 'bg-alert-400' : 'bg-brass-400';
  return (
    <div className="flex items-center gap-2" title={`Fenêtre 5 h : ${Math.round(usage.fiveHour * 100)} % — fenêtre 7 j : ${Math.round(usage.sevenDay * 100)} %`}>
      <div className="h-1.5 w-16 rounded-full bg-ink-700 overflow-hidden">
        <div className={`h-full ${tone} transition-all`} style={{ width: `${Math.min(pct, 100)}%` }} />
      </div>
      <span className="text-[11px] font-mono text-ivory-500 tabular-nums">{pct} %</span>
    </div>
  );
};

const STATUS_ICON: Record<MissionItem['status'], React.ReactNode> = {
  started: <Loader2 className="w-4 h-4 text-think-300 animate-spin shrink-0" />,
  failed: <XCircle className="w-4 h-4 text-danger-400 shrink-0" />,
  cancelled: <CircleSlash className="w-4 h-4 text-ivory-700 shrink-0" />,
  completed: <CheckCircle2 className="w-4 h-4 text-ok-400 shrink-0" />,
};

export const MissionPanel: React.FC<MissionPanelProps> = ({ missions, usage, onStopMission, bare = false }) => {
  const [expandedIds, setExpandedIds] = useState<Record<number, boolean>>({});

  if (missions.length === 0 && !usage) return null;

  const runningCount = missions.filter((m) => m.status === 'started').length;

  return (
    <div className={bare ? '' : 'border-b border-ink-800 shrink-0'}>
      {!bare && (
        <div className="flex items-center justify-between px-3 py-2">
          <div className="flex items-center gap-2 text-[13px] text-ivory-300">
            <span className="font-medium">Missions</span>
            {runningCount > 0 && <span className="pill h-6 text-think-300 border-think-600/50">{runningCount} en cours</span>}
          </div>
          {usage && <UsageGauge usage={usage} />}
        </div>
      )}

      {missions.length > 0 && (
        <div className={`${bare ? '' : 'max-h-56 px-3 pb-2'} overflow-y-auto flex flex-col gap-1.5`}>
          {missions.map((mission) => {
            const isExpanded = !!expandedIds[mission.id];
            return (
              <div
                key={mission.id}
                className={`rounded-xl border p-2.5 text-[13px] ${
                  mission.status === 'started'
                    ? 'bg-think-600/10 border-think-600/40'
                    : mission.status === 'failed'
                    ? 'bg-danger-600/10 border-danger-600/40'
                    : 'bg-ink-900/50 border-ink-800'
                }`}
              >
                <div
                  className="flex items-center justify-between gap-2 cursor-pointer select-none"
                  onClick={() => setExpandedIds((prev) => ({ ...prev, [mission.id]: !prev[mission.id] }))}
                >
                  <div className="flex items-center gap-2 overflow-hidden">
                    {STATUS_ICON[mission.status]}
                    <span className="font-mono text-[11px] text-ivory-700 shrink-0">#{mission.id}</span>
                    <span className="text-ivory-100 truncate">{mission.description}</span>
                  </div>
                  <div className="flex items-center gap-1 shrink-0">
                    <span className="font-mono text-[10px] text-ivory-700 uppercase">{mission.backend}</span>
                    {mission.status === 'started' && onStopMission && (
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onStopMission(mission.id);
                        }}
                        className="btn btn-quiet btn-icon min-h-8 w-8 hover:text-danger-300"
                        title="Annuler cette mission"
                        aria-label="Annuler cette mission"
                      >
                        <Ban className="w-3.5 h-3.5" />
                      </button>
                    )}
                    {isExpanded ? <ChevronDown className="w-4 h-4 text-ivory-500" /> : <ChevronRight className="w-4 h-4 text-ivory-500" />}
                  </div>
                </div>

                {mission.status === 'started' && mission.progress && (
                  <div className="mt-1.5 flex items-center gap-1.5 text-[12px] text-think-300/80 overflow-hidden">
                    <Activity className="w-3 h-3 shrink-0" />
                    <span className="truncate font-mono" title={mission.progress}>
                      {mission.progress}
                    </span>
                  </div>
                )}

                {isExpanded && (
                  <div className="mt-2 pt-2 border-t border-ink-800 flex flex-col gap-2">
                    <p className="m-0 text-[12px] text-ivory-500 whitespace-pre-wrap">{mission.description}</p>
                    {mission.summary && (
                      <div className="flex flex-col gap-1">
                        <div className="text-[11px] text-ivory-500">{mission.status === 'cancelled' ? 'Motif et travail partiel' : 'Compte rendu'}</div>
                        <pre
                          className={`m-0 text-[12px] p-2 rounded-lg bg-ink-950 overflow-x-auto border border-ink-800 max-h-48 whitespace-pre-wrap font-sans ${
                            mission.status === 'cancelled' ? 'text-alert-300' : mission.status === 'failed' ? 'text-danger-300' : 'text-ivory-100'
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
