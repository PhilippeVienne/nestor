import React from 'react';
import { Radio, Wifi, WifiOff, Cpu, RefreshCw, Volume2, Mic, Terminal } from 'lucide-react';
import type { DaemonStatus, ConnectionState } from '../types';

interface HeaderProps {
  status: DaemonStatus;
  connectionState: ConnectionState;
  onSetStatus?: (status: DaemonStatus) => void;
  isSimulated: boolean;
  onToggleSimulated: () => void;
  isConsoleOpen?: boolean;
  onToggleConsole?: () => void;
  runningToolsCount?: number;
}

export const Header: React.FC<HeaderProps> = ({
  status,
  connectionState,
  onSetStatus,
  isSimulated,
  onToggleSimulated,
  isConsoleOpen,
  onToggleConsole,
  runningToolsCount = 0,
}) => {
  const getStatusBadge = () => {
    switch (status) {
      case 'listening':
        return (
          <div className="flex items-center gap-1.5 px-2.5 sm:px-3 py-1 rounded-full bg-cyan-950/80 text-cyan-300 border border-cyan-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(6,182,212,0.25)]">
            <Mic className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-pulse" />
            <span className="font-semibold tracking-wider">ÉCOUTE</span>
          </div>
        );
      case 'thinking':
        return (
          <div className="flex items-center gap-1.5 px-2.5 sm:px-3 py-1 rounded-full bg-purple-950/80 text-purple-300 border border-purple-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(168,85,247,0.25)]">
            <Cpu className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-spin text-purple-400" />
            <span className="font-semibold tracking-wider">RÉFLEXION</span>
          </div>
        );
      case 'speaking':
        return (
          <div className="flex items-center gap-1.5 px-2.5 sm:px-3 py-1 rounded-full bg-amber-950/80 text-amber-300 border border-amber-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(245,158,11,0.25)]">
            <Volume2 className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-bounce text-amber-400" />
            <span className="font-semibold tracking-wider">PAROLE</span>
          </div>
        );
      case 'idle':
      default:
        return (
          <div className="flex items-center gap-1.5 px-2.5 sm:px-3 py-1 rounded-full bg-slate-900/80 text-slate-400 border border-slate-700/60 text-[11px] sm:text-xs font-mono">
            <span className="w-2 h-2 rounded-full bg-cyan-400/40" />
            <span className="font-semibold tracking-wider">EN ATTENTE</span>
          </div>
        );
    }
  };

  const getConnectionBadge = () => {
    switch (connectionState) {
      case 'connected':
        return (
          <span className="flex items-center gap-1.5 text-[11px] sm:text-xs font-mono px-2 sm:px-3 py-1 rounded-full bg-emerald-950/70 border border-emerald-500/50 text-emerald-300 shadow-[0_0_12px_rgba(16,185,129,0.2)]">
            <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
            <Wifi className="w-3 h-3 sm:w-3.5 sm:h-3.5 text-emerald-400" />
            <span className="hidden md:inline font-semibold">Live Daemon : nestord (port 8340)</span>
            <span className="md:hidden">Connecté</span>
          </span>
        );
      case 'connecting':
        return (
          <span className="flex items-center gap-1.5 text-[11px] sm:text-xs font-mono px-2 sm:px-2.5 py-1 rounded-full bg-amber-950/60 border border-amber-500/40 text-amber-300 animate-pulse">
            <RefreshCw className="w-3 h-3 animate-spin text-amber-400" />
            <span className="hidden sm:inline">Connexion...</span>
          </span>
        );
      case 'disconnected':
      case 'error':
      default:
        return (
          <span className="flex items-center gap-1.5 text-[11px] sm:text-xs font-mono px-2 sm:px-2.5 py-1 rounded-full bg-rose-950/60 border border-rose-500/40 text-rose-300" title="Tentative de reconnexion automatique vers ws://127.0.0.1:8340/ws">
            <WifiOff className="w-3 h-3 text-rose-400" />
            <span className="hidden sm:inline">Déconnecté</span>
          </span>
        );
    }
  };

  return (
    <header className="w-full flex items-center justify-between px-3 sm:px-5 py-2.5 sm:py-3.5 border-b border-slate-800/80 bg-slate-950/90 backdrop-blur-xl shrink-0 z-20">
      {/* Brand & Assistant Name */}
      <div className="flex items-center gap-2.5 sm:gap-3">
        <div className="w-8 h-8 sm:w-9 sm:h-9 rounded-xl bg-gradient-to-tr from-cyan-600 via-sky-500 to-indigo-600 flex items-center justify-center text-white shadow-[0_0_15px_rgba(6,182,212,0.35)] shrink-0">
          <Radio className="w-4 h-4 sm:w-5 sm:h-5" />
        </div>
        <div>
          <div className="flex items-center gap-1.5 sm:gap-2">
            <h1 className="text-sm sm:text-base font-bold tracking-widest text-slate-100 font-mono m-0">
              NESTOR
            </h1>
            <span className="text-[9px] sm:text-[10px] font-mono px-1.5 py-0.5 rounded bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
              v0.1
            </span>
          </div>
          <p className="text-[10px] text-slate-400 font-sans hidden md:block">
            Assistant Vocal & Observabilité Claude Code
          </p>
        </div>
      </div>

      {/* Middle Status Pill */}
      <div className="flex items-center gap-2">
        {getStatusBadge()}
      </div>

      {/* Right controls: WebSocket badge + Simulation mode button + Mobile Console Drawer Button */}
      <div className="flex items-center gap-1.5 sm:gap-2">
        {getConnectionBadge()}

        {/* Mobile Toggle Console Button */}
        {onToggleConsole && (
          <button
            onClick={onToggleConsole}
            className={`md:hidden relative p-1.5 rounded-lg border transition-all ${
              isConsoleOpen
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40'
                : 'bg-slate-900/80 text-slate-400 border-slate-800'
            }`}
            title="Ouvrir la console d'outils"
          >
            <Terminal className="w-4 h-4" />
            {runningToolsCount > 0 && (
              <span className="absolute -top-1 -right-1 w-2 h-2 bg-amber-400 rounded-full animate-ping" />
            )}
          </button>
        )}

        {/* Dev simulator toggle (hidden on small mobile to save space) */}
        <button
          onClick={onToggleSimulated}
          className={`hidden sm:inline-flex px-2 py-1 rounded-lg text-[11px] font-mono border transition-all ${
            isSimulated
              ? 'bg-purple-600 text-white border-purple-400 shadow-[0_0_10px_rgba(168,85,247,0.3)]'
              : 'bg-slate-900/60 hover:bg-slate-800 text-slate-500 hover:text-slate-300 border-slate-800'
          }`}
          title="Bascule en mode simulation pour tester manuellement sans daemon"
        >
          {isSimulated ? 'Quitter Démo' : 'Simulateur (dev)'}
        </button>

        {isSimulated && onSetStatus && (
          <div className="hidden xl:flex items-center gap-1 bg-slate-900/90 border border-purple-500/30 p-1 rounded-lg">
            {(['idle', 'listening', 'thinking', 'speaking'] as const).map((st) => (
              <button
                key={st}
                onClick={() => onSetStatus(st)}
                className={`px-2 py-0.5 text-[10px] font-mono rounded capitalize transition-all ${
                  status === st
                    ? 'bg-purple-500/30 text-purple-200 border border-purple-400/40'
                    : 'text-slate-400 hover:text-white'
                }`}
              >
                {st}
              </button>
            ))}
          </div>
        )}
      </div>
    </header>
  );
};
