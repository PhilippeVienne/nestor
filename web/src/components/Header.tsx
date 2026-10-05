import React from 'react';
import { Wifi, WifiOff, Cpu, RefreshCw, Volume2, Mic, Terminal, Zap, Sparkles, SlidersHorizontal } from 'lucide-react';
import type { DaemonStatus, ConnectionState, BackendStatusInfo } from '../types';

interface HeaderProps {
  status: DaemonStatus;
  connectionState: ConnectionState;
  onSetStatus?: (status: DaemonStatus) => void;
  isSimulated: boolean;
  onToggleSimulated: () => void;
  isConsoleOpen?: boolean;
  onToggleConsole?: () => void;
  runningToolsCount?: number;
  backendStatus?: BackendStatusInfo;
  onSetBackend?: (backend: string) => void;
  onOpenSettings?: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  status,
  connectionState,
  onSetStatus: _onSetStatus,
  isSimulated: _isSimulated,
  onToggleSimulated: _onToggleSimulated,
  isConsoleOpen,
  onToggleConsole,
  runningToolsCount = 0,
  backendStatus,
  onSetBackend,
  onOpenSettings,
}) => {
  const getStatusBadge = () => {
    switch (status) {
      case 'listening':
        return (
          <div className="flex items-center gap-1.5 px-2 sm:px-3 py-1 rounded-full bg-cyan-950/80 text-cyan-300 border border-cyan-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(6,182,212,0.25)]">
            <Mic className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-pulse text-cyan-400" />
            <span className="font-semibold tracking-wider hidden sm:inline">ÉCOUTE</span>
          </div>
        );
      case 'thinking':
        return (
          <div className="flex items-center gap-1.5 px-2 sm:px-3 py-1 rounded-full bg-purple-950/80 text-purple-300 border border-purple-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(168,85,247,0.25)]">
            <Cpu className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-spin text-purple-400" />
            <span className="font-semibold tracking-wider hidden sm:inline">RÉFLEXION</span>
          </div>
        );
      case 'speaking':
        return (
          <div className="flex items-center gap-1.5 px-2 sm:px-3 py-1 rounded-full bg-amber-950/80 text-amber-300 border border-amber-500/40 text-[11px] sm:text-xs font-mono shadow-[0_0_12px_rgba(245,158,11,0.25)]">
            <Volume2 className="w-3 h-3 sm:w-3.5 sm:h-3.5 animate-bounce text-amber-400" />
            <span className="font-semibold tracking-wider hidden sm:inline">PAROLE</span>
          </div>
        );
      case 'idle':
      default:
        return (
          <div className="flex items-center gap-1.5 px-2 sm:px-3 py-1 rounded-full bg-slate-900/80 text-slate-400 border border-slate-700/60 text-[11px] sm:text-xs font-mono" title="Dites 'Hey Nestor' pour lui parler">
            <span className="w-2 h-2 rounded-full bg-cyan-400/40" />
            <span className="font-semibold tracking-wider hidden sm:inline">VEILLE ("Hey Nestor")</span>
            <span className="font-semibold tracking-wider sm:hidden">VEILLE</span>
          </div>
        );
    }
  };

  const getConnectionBadge = () => {
    switch (connectionState) {
      case 'connected':
        return (
          <span className="flex items-center gap-1.5 text-[11px] sm:text-xs font-mono px-2 sm:px-2.5 py-1 rounded-full bg-emerald-950/60 border border-emerald-500/40 text-emerald-400 shadow-[0_0_10px_rgba(16,185,129,0.15)]" title="Connecté au daemon nestord (port 8340)">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <Wifi className="w-3 h-3 text-emerald-400" />
            <span className="hidden lg:inline text-[11px]">nestord</span>
          </span>
        );
      case 'connecting':
        return (
          <span className="flex items-center gap-1 text-[11px] font-mono p-1.5 sm:px-2 sm:py-1 rounded-full bg-amber-950/60 border border-amber-500/40 text-amber-400 animate-pulse" title="Connexion...">
            <RefreshCw className="w-3 h-3 animate-spin" />
            <span className="hidden lg:inline text-[11px]">Connexion...</span>
          </span>
        );
      case 'disconnected':
      case 'error':
      default:
        return (
          <span className="flex items-center gap-1 text-[11px] font-mono p-1.5 sm:px-2 sm:py-1 rounded-full bg-rose-950/60 border border-rose-500/40 text-rose-400" title="Déconnecté de nestord">
            <WifiOff className="w-3 h-3" />
            <span className="hidden lg:inline text-[11px]">Déconnecté</span>
          </span>
        );
    }
  };

  const getBackendBadge = () => {
    if (!backendStatus) return null;
    const isAgy = backendStatus.active_backend === 'agy' || backendStatus.is_fallback;
    return (
      <div className="relative group">
        <button
          onClick={() => {
            if (onSetBackend) {
              onSetBackend(isAgy ? 'claude' : 'agy');
            }
          }}
          className={`flex items-center gap-1.5 px-2 sm:px-3 py-1 rounded-full text-[11px] sm:text-xs font-mono transition-all border cursor-pointer ${
            isAgy
              ? 'bg-amber-950/80 text-amber-300 border-amber-500/50 shadow-[0_0_12px_rgba(245,158,11,0.25)] hover:bg-amber-900/90'
              : 'bg-indigo-950/70 text-indigo-300 border-indigo-500/40 hover:bg-indigo-900/80'
          }`}
          title={
            isAgy
              ? `Mode Réduit (AGY) actif.${backendStatus.reason ? ` Cause: ${backendStatus.reason}.` : ''} Cliquer pour tenter de rebasculer sur Claude.`
              : 'Backend standard Claude Code actif. Cliquer pour forcer le Mode Réduit (AGY).'
          }
        >
          {isAgy ? (
            <>
              <Zap className="w-3 h-3 sm:w-3.5 sm:h-3.5 text-amber-400 animate-pulse" />
              <span className="font-semibold tracking-wider hidden md:inline">⚡ MODE RÉDUIT (AGY)</span>
              <span className="font-semibold tracking-wider md:hidden text-[10px]">AGY</span>
            </>
          ) : (
            <>
              <Sparkles className="w-3 h-3 sm:w-3.5 sm:h-3.5 text-indigo-400" />
              <span className="font-semibold tracking-wider hidden md:inline">CLAUDE CODE</span>
              <span className="font-semibold tracking-wider md:hidden text-[10px]">CLAUDE</span>
            </>
          )}
        </button>
      </div>
    );
  };

  return (
    <header className="w-full flex items-center justify-between px-3 sm:px-6 py-2.5 sm:py-3 border-b border-slate-800/60 bg-slate-950/80 backdrop-blur-xl shrink-0 z-20">
      {/* Brand & Assistant Name */}
      <div className="flex items-center gap-2.5 sm:gap-3">
        <div className="w-8 h-8 rounded-xl overflow-hidden border border-cyan-500/30 shadow-[0_0_12px_rgba(6,182,212,0.3)] shrink-0 bg-[#030c28]">
          <img src="/nestor-logo.png" alt="Nestor Logo" className="w-full h-full object-cover" />
        </div>
        <div>
          <div className="flex items-center gap-1.5 sm:gap-2">
            <h1 className="text-sm font-semibold tracking-wider text-slate-100 font-mono m-0">
              NESTOR
            </h1>
            <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-slate-800/80 text-slate-400 border border-slate-700/60 hidden sm:inline">
              v0.1
            </span>
          </div>
          <p className="text-[10px] text-slate-400 font-sans hidden sm:block">
            Majordome IA & Observabilité Claude Code
          </p>
        </div>
      </div>

      {/* Middle Status Pill + Backend Pill */}
      <div className="flex items-center gap-2">
        {getStatusBadge()}
        {getBackendBadge()}
      </div>

      {/* Right controls: Connection + Console Drawer Toggle */}
      <div className="flex items-center gap-2">
        {getConnectionBadge()}

        {/* Toggle Tool & Missions Console (Desktop & Mobile) */}
        {onOpenSettings && (
          <button
            type="button"
            onClick={onOpenSettings}
            title="Réglages"
            className="flex items-center gap-1.5 px-2 sm:px-2.5 py-1 rounded-lg border border-slate-700/80 bg-slate-900/60 text-slate-300 hover:text-white hover:border-slate-500 text-[11px] sm:text-xs font-mono transition-colors"
          >
            <SlidersHorizontal className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Réglages</span>
          </button>
        )}
        {onToggleConsole && (
          <button
            onClick={onToggleConsole}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border text-xs font-mono transition-all ${
              isConsoleOpen
                ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40 shadow-[0_0_10px_rgba(6,182,212,0.15)]'
                : 'bg-slate-900/80 hover:bg-slate-800 text-slate-400 hover:text-slate-200 border-slate-800'
            }`}
            title={isConsoleOpen ? "Masquer la console d'outils" : "Afficher la console d'outils et missions"}
          >
            <div className="relative flex items-center">
              <Terminal className="w-3.5 h-3.5" />
              {runningToolsCount > 0 && (
                <span className="absolute -top-1 -right-1 w-2 h-2 bg-amber-400 rounded-full animate-ping" />
              )}
            </div>
            <span className="hidden sm:inline text-[11px]">Outils</span>
            {runningToolsCount > 0 && (
              <span className="text-[10px] px-1 rounded bg-amber-500/30 text-amber-300 font-bold">
                {runningToolsCount}
              </span>
            )}
          </button>
        )}
      </div>
    </header>
  );
};

