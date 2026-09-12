import React, { useState } from 'react';
import {
  Terminal,
  Play,
  CheckCircle2,
  Clock,
  ChevronDown,
  ChevronRight,
  Filter,
  Trash2,
  Code2,
  X,
} from 'lucide-react';
import type { ToolCallItem, MissionItem, UsageInfo } from '../types';
import { MissionPanel } from './MissionPanel';

interface ToolConsoleProps {
  toolCalls: ToolCallItem[];
  missions?: MissionItem[];
  usage?: UsageInfo | null;
  onStopMission?: (id: number, reason?: string) => void;
  onClear?: () => void;
  isOpen: boolean;
  onToggle: () => void;
}

export const ToolConsole: React.FC<ToolConsoleProps> = ({
  toolCalls,
  missions = [],
  usage = null,
  onStopMission,
  onClear,
  isOpen,
  onToggle,
}) => {
  const [filter, setFilter] = useState<'all' | 'running' | 'completed'>('all');
  const [expandedIds, setExpandedIds] = useState<Record<string, boolean>>({});

  const toggleExpand = (id: string) => {
    setExpandedIds((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const filteredCalls = toolCalls.filter((t) => {
    if (filter === 'all') return true;
    return t.status === filter;
  });

  const runningCount = toolCalls.filter((t) => t.status === 'running').length;

  return (
    <>
      {/* Mobile Backdrop Overlay when drawer is open */}
      {isOpen && (
        <div
          className="md:hidden fixed inset-0 bg-black/60 backdrop-blur-sm z-40 transition-opacity"
          onClick={onToggle}
          aria-hidden="true"
        />
      )}

      {/* Console Panel (Drawer on mobile, Sidebar on desktop) */}
      <div
        className={`flex flex-col border-l border-slate-800 bg-slate-950/95 backdrop-blur-xl transition-all duration-300 ${
          isOpen
            ? 'fixed md:relative inset-y-0 right-0 z-50 w-[85vw] max-w-sm sm:w-96 shadow-2xl md:shadow-none'
            : 'hidden md:flex w-12'
        }`}
      >
        {/* Header bar */}
        <div className="flex items-center justify-between px-3 py-3 border-b border-slate-800/80 bg-slate-900/50 shrink-0">
          <button
            onClick={onToggle}
            className="flex items-center gap-2 text-slate-300 hover:text-white transition-colors"
            title={isOpen ? 'Replier la console' : 'Déplier la console'}
          >
            <div className="relative">
              <Terminal className="w-4 h-4 text-cyan-400" />
              {runningCount > 0 && (
                <span className="absolute -top-1 -right-1 w-2 h-2 bg-amber-400 rounded-full animate-ping" />
              )}
            </div>
            {isOpen && (
              <span className="text-xs font-mono font-semibold tracking-wider uppercase text-slate-200">
                Console Outils
              </span>
            )}
          </button>

          {isOpen && (
            <div className="flex items-center gap-2">
              {runningCount > 0 && (
                <span className="px-2 py-0.5 rounded text-[10px] font-mono bg-amber-500/20 text-amber-300 border border-amber-500/40 animate-pulse">
                  {runningCount} en cours
                </span>
              )}
              {toolCalls.length > 0 && onClear && (
                <button
                  onClick={onClear}
                  className="text-slate-500 hover:text-rose-400 p-1 transition-colors"
                  title="Vider la console"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              )}
              {/* Close X button for mobile */}
              <button
                onClick={onToggle}
                className="md:hidden text-slate-400 hover:text-white p-1 rounded-lg"
                title="Fermer la console"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          )}
        </div>

        {isOpen && (
          <>
            <MissionPanel missions={missions} usage={usage} onStopMission={onStopMission} />

            {/* Filter tabs */}
            <div className="flex items-center justify-between px-3 py-2 border-b border-slate-800/50 bg-slate-900/20 text-[11px] font-mono shrink-0">
              <div className="flex gap-1">
                {(['all', 'running', 'completed'] as const).map((mode) => (
                  <button
                    key={mode}
                    onClick={() => setFilter(mode)}
                    className={`px-2 py-1 rounded transition-colors ${
                      filter === mode
                        ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/30'
                        : 'text-slate-400 hover:text-slate-200'
                    }`}
                  >
                    {mode === 'all'
                      ? `Tous (${toolCalls.length})`
                      : mode === 'running'
                      ? `Actifs (${runningCount})`
                      : `Terminés`}
                  </button>
                ))}
              </div>
              <Filter className="w-3 h-3 text-slate-500" />
            </div>

            {/* List of tool calls */}
            <div className="flex-1 overflow-y-auto p-3 space-y-2.5">
              {filteredCalls.length === 0 ? (
                <div className="h-40 flex flex-col items-center justify-center text-center p-4">
                  <Code2 className="w-8 h-8 text-slate-600 mb-2" />
                  <p className="text-xs text-slate-500 font-mono">
                    Aucun appel d'outil détecté
                  </p>
                  <span className="text-[10px] text-slate-600 mt-1">
                    Les commandes Bash et éditions de code apparaîtront ici en temps réel.
                  </span>
                </div>
              ) : (
                filteredCalls.map((tool) => {
                  const isExpanded = !!expandedIds[tool.id];
                  const isRunning = tool.status === 'running';

                  return (
                    <div
                      key={tool.id}
                      className={`rounded-xl border p-2.5 text-xs transition-all font-mono ${
                        isRunning
                          ? 'bg-amber-950/20 border-amber-500/40 shadow-[0_0_10px_rgba(245,158,11,0.1)]'
                          : 'bg-slate-900/60 border-slate-800/80 hover:border-slate-700'
                      }`}
                    >
                      {/* Header line */}
                      <div
                        className="flex items-center justify-between cursor-pointer select-none"
                        onClick={() => toggleExpand(tool.id)}
                      >
                        <div className="flex items-center gap-2 overflow-hidden">
                          {isRunning ? (
                            <div className="relative flex items-center justify-center">
                              <Play className="w-3.5 h-3.5 text-amber-400 fill-amber-400 animate-pulse" />
                            </div>
                          ) : (
                            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                          )}
                          <span className="font-semibold text-slate-200 truncate">
                            {tool.name}
                          </span>
                          {tool.missionId !== undefined && (
                            <span
                              className="px-1.5 py-0.5 rounded text-[9px] bg-fuchsia-500/20 text-fuchsia-300 border border-fuchsia-500/40 shrink-0"
                              title="Execute par un sous-agent de mission"
                            >
                              mission #{tool.missionId}
                            </span>
                          )}
                        </div>

                        <div className="flex items-center gap-1 text-slate-500 text-[10px]">
                          <Clock className="w-2.5 h-2.5" />
                          <span>
                            {tool.startedAt.toLocaleTimeString('fr-FR', {
                              hour: '2-digit',
                              minute: '2-digit',
                              second: '2-digit',
                            })}
                          </span>
                          {isExpanded ? (
                            <ChevronDown className="w-3.5 h-3.5 text-slate-400 ml-1" />
                          ) : (
                            <ChevronRight className="w-3.5 h-3.5 text-slate-400 ml-1" />
                          )}
                        </div>
                      </div>

                      {/* Input payload preview */}
                      {isExpanded && (
                        <div className="mt-2.5 pt-2 border-t border-white/5 space-y-1">
                          <div className="text-[10px] text-slate-400 font-sans uppercase font-medium tracking-wider">
                            Paramètres d'entrée :
                          </div>
                          <pre className="text-[11px] p-2 rounded-lg bg-black/50 text-cyan-300 overflow-x-auto border border-white/5 font-mono max-h-48">
                            {JSON.stringify(tool.input, null, 2)}
                          </pre>
                        </div>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </>
        )}

        {/* Collapsed state mini-badge (Desktop only) */}
        {!isOpen && (
          <div className="hidden md:flex flex-1 flex-col items-center justify-start pt-6 space-y-4">
            <button
              onClick={onToggle}
              className="text-slate-400 hover:text-cyan-400 transition-colors p-2"
              title="Ouvrir la console d'outils"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
            {runningCount > 0 && (
              <span className="w-2 h-2 rounded-full bg-amber-400 animate-ping" />
            )}
          </div>
        )}
      </div>
    </>
  );
};
