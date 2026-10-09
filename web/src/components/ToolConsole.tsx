import React, { useState } from 'react';
import { Play, CheckCircle2, ChevronDown, ChevronRight, Trash2, X } from 'lucide-react';
import type { ToolCallItem, MissionItem, UsageInfo } from '../types';
import { MissionPanel } from './MissionPanel';
import { ConsciencePanel } from './ConsciencePanel';
import { ActivityLog } from './ActivityLog';
import type { JudgeItem, ActivityItem } from '../types';

interface ToolConsoleProps {
  toolCalls: ToolCallItem[];
  missions?: MissionItem[];
  usage?: UsageInfo | null;
  onStopMission?: (id: number, reason?: string) => void;
  onClear?: () => void;
  isOpen: boolean;
  onToggle: () => void;
  judgements?: JudgeItem[];
  judgeModel?: string;
  onResolveJudgement?: (id: number, approve: boolean) => void;
  activity?: ActivityItem[];
}

export const ToolConsole: React.FC<ToolConsoleProps> = ({
  toolCalls,
  missions = [],
  usage = null,
  onStopMission,
  onClear,
  isOpen,
  onToggle,
  judgements = [],
  judgeModel,
  onResolveJudgement,
  activity = [],
}) => {
  const [view, setView] = useState<'tools' | 'conscience' | 'activity'>('tools');
  const pendingJudgements = judgements.filter((j) => j.pending).length;
  const [filter, setFilter] = useState<'all' | 'running' | 'completed'>('all');
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});

  const filteredCalls = toolCalls.filter((t) => filter === 'all' || t.status === filter);
  const runningCount = toolCalls.filter((t) => t.status === 'running').length;

  return (
    <>
      {isOpen && <div className="fixed inset-0 bg-ink-950/60 backdrop-blur-sm z-40" onClick={onToggle} aria-hidden="true" />}

      <div
        className={`fixed inset-y-0 right-0 z-50 w-[92vw] max-w-sm sm:w-[400px] flex flex-col border-l border-ink-800 bg-ink-950/98 shadow-2xl transition-transform duration-300 ease-in-out ${
          isOpen ? 'translate-x-0' : 'translate-x-full pointer-events-none'
        }`}
        aria-hidden={!isOpen}
      >
        <div className="flex items-center justify-between px-4 py-3 border-b border-ink-800 shrink-0">
          <div className="flex items-center gap-2">
            <h2 className="m-0 font-display text-[18px] text-ivory-50">Console</h2>
            {runningCount > 0 && <span className="pill h-6 text-alert-300 border-alert-600/50">{runningCount} en cours</span>}
          </div>
          <div className="flex items-center gap-1">
            {toolCalls.length > 0 && onClear && (
              <button type="button" onClick={onClear} className="btn btn-quiet btn-icon hover:text-danger-300" title="Vider la console" aria-label="Vider la console">
                <Trash2 className="w-4 h-4" />
              </button>
            )}
            <button type="button" onClick={onToggle} className="btn btn-quiet btn-icon" title="Fermer" aria-label="Fermer la console">
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {isOpen && (
          <div role="tablist" aria-label="Vues de la console" className="flex shrink-0 border-b border-ink-800 text-[13px]">
            {(
              [
                ['tools', 'Outils'],
                ['conscience', 'Conscience'],
                ['activity', 'Journal'],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={view === id}
                onClick={() => setView(id)}
                className={`flex-1 h-11 px-2 transition-colors border-b-2 ${
                  view === id ? 'text-brass-300 border-brass-400' : 'text-ivory-500 border-transparent hover:text-ivory-100'
                }`}
              >
                {label}
                {id === 'conscience' && pendingJudgements > 0 && <span className="ml-1.5 pill h-5 px-1.5 text-alert-300 border-alert-600/50">{pendingJudgements}</span>}
              </button>
            ))}
          </div>
        )}

        {isOpen && view === 'conscience' && <ConsciencePanel judgements={judgements} model={judgeModel} onResolve={onResolveJudgement ?? (() => {})} />}
        {isOpen && view === 'activity' && <ActivityLog activity={activity} />}

        {isOpen && view === 'tools' && (
          <>
            <MissionPanel missions={missions} usage={usage} onStopMission={onStopMission} />

            <div className="flex items-center gap-1 px-3 py-2 border-b border-ink-800 text-[12px] shrink-0">
              {(['all', 'running', 'completed'] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setFilter(mode)}
                  className={`pill h-7 cursor-pointer ${filter === mode ? 'text-brass-300 border-brass-500' : 'hover:text-ivory-100'}`}
                >
                  {mode === 'all' ? `Tous · ${toolCalls.length}` : mode === 'running' ? `En cours · ${runningCount}` : 'Terminés'}
                </button>
              ))}
            </div>

            <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-2">
              {filteredCalls.length === 0 ? (
                <p className="m-0 py-10 text-center text-[13px] text-ivory-700">Aucun appel d'outil. Les commandes et éditions de l'assistant apparaîtront ici.</p>
              ) : (
                filteredCalls.map((tool) => {
                  const isExpanded = !!expandedIds[tool.id];
                  const isRunning = tool.status === 'running';
                  return (
                    <div key={tool.id} className={`rounded-xl border p-2.5 text-[13px] ${isRunning ? 'bg-alert-600/10 border-alert-600/40' : 'bg-ink-900/50 border-ink-800'}`}>
                      <div className="flex items-center justify-between gap-2 cursor-pointer select-none" onClick={() => setExpandedIds((prev) => ({ ...prev, [tool.id]: !prev[tool.id] }))}>
                        <div className="flex items-center gap-2 overflow-hidden">
                          {isRunning ? <Play className="w-3.5 h-3.5 text-alert-400 fill-alert-400 shrink-0" /> : <CheckCircle2 className="w-3.5 h-3.5 text-ok-400 shrink-0" />}
                          <span className="font-mono font-medium text-ivory-100 truncate">{tool.name}</span>
                          {tool.missionId !== undefined && <span className="pill h-5 px-1.5 text-[10px] text-think-300 border-think-600/50">mission #{tool.missionId}</span>}
                        </div>
                        <div className="flex items-center gap-1 text-ivory-700 text-[11px] font-mono tabular-nums shrink-0">
                          {tool.startedAt.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                          {isExpanded ? <ChevronDown className="w-4 h-4 text-ivory-500" /> : <ChevronRight className="w-4 h-4 text-ivory-500" />}
                        </div>
                      </div>
                      {isExpanded && (
                        <pre className="m-0 mt-2 text-[12px] p-2 rounded-lg bg-ink-950 text-ivory-300 overflow-x-auto border border-ink-800 max-h-48">
                          {JSON.stringify(tool.input, null, 2)}
                        </pre>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </>
        )}
      </div>
    </>
  );
};
