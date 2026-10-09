import React from 'react';
import { Mic, Cpu, Volume2, Terminal, SlidersHorizontal, LayoutDashboard } from 'lucide-react';
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
  /** Ouvre le tableau de bord en tiroir (affiche seulement sous 1280 px). */
  onOpenDashboard?: () => void;
}

const STATUS: Record<DaemonStatus, { label: string; hint: string; tone: string; dot: string; Icon: typeof Mic }> = {
  listening: { label: 'Écoute', hint: 'Nestor vous écoute', tone: 'text-listen-300 border-listen-600/50', dot: 'bg-listen-400', Icon: Mic },
  thinking: { label: 'Réflexion', hint: 'Nestor réfléchit', tone: 'text-think-300 border-think-600/50', dot: 'bg-think-400', Icon: Cpu },
  speaking: { label: 'Parole', hint: 'Nestor vous répond', tone: 'text-brass-300 border-brass-500/50', dot: 'bg-brass-400', Icon: Volume2 },
  idle: { label: 'Veille', hint: 'Dites « Hey Nestor » pour lui parler', tone: 'text-ivory-500 border-ink-700', dot: 'bg-ivory-500', Icon: Mic },
};

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
  onOpenDashboard,
}) => {
  const state = STATUS[status] ?? STATUS.idle;
  const isAgy = !!backendStatus && (backendStatus.active_backend === 'agy' || backendStatus.is_fallback);
  const connected = connectionState === 'connected';

  return (
    <header className="w-full flex flex-wrap items-center justify-between gap-x-3 gap-y-2 px-4 sm:px-6 py-3 border-b border-ink-800 bg-ink-950/90 backdrop-blur shrink-0 z-20">
      {/* Nom et devise */}
      <div className="flex items-center gap-3 min-w-0">
        <div className="w-9 h-9 rounded-full overflow-hidden border border-brass-500/50 shrink-0 bg-ink-900">
          <img src="/nestor-logo.png" alt="" className="w-full h-full object-cover" />
        </div>
        <div className="min-w-0">
          <h1 className="font-display text-[22px] leading-none text-ivory-50 m-0">Nestor</h1>
          <p className="text-[12px] text-ivory-500 m-0 hidden sm:block">À votre service, discrètement.</p>
        </div>
      </div>

      {/* Etat de parole et moteur */}
      <div className="flex items-center gap-2">
        <span className={`pill ${state.tone}`} title={state.hint}>
          <span className={`w-2 h-2 rounded-full ${state.dot} ${status === 'idle' ? '' : 'nestor-breathe'}`} />
          {state.label}
        </span>
        {backendStatus && (
          <button
            type="button"
            onClick={() => onSetBackend?.(isAgy ? 'claude' : 'agy')}
            className={`pill cursor-pointer transition-colors ${
              isAgy ? 'text-alert-300 border-alert-600/60 hover:border-alert-400' : 'text-ivory-300 hover:border-ink-600'
            }`}
            title={
              isAgy
                ? `Mode réduit sur AGY.${backendStatus.reason ? ` Cause : ${backendStatus.reason}.` : ''} Cliquer pour revenir sur Claude.`
                : 'Claude Code. Cliquer pour passer en mode réduit (AGY).'
            }
          >
            {isAgy ? 'Mode réduit' : 'Claude'}
          </button>
        )}
      </div>

      {/* Connexion et acces aux volets */}
      <div className="flex items-center gap-2">
        <span
          className={`pill ${connected ? 'text-ok-300 border-ok-600/50' : connectionState === 'connecting' ? 'text-alert-300 border-alert-600/50' : 'text-danger-300 border-danger-600/50'}`}
          title={connected ? 'Connecté au daemon nestord' : connectionState === 'connecting' ? 'Connexion au daemon…' : 'Daemon injoignable'}
        >
          <span className={`w-2 h-2 rounded-full ${connected ? 'bg-ok-400' : connectionState === 'connecting' ? 'bg-alert-400 nestor-breathe' : 'bg-danger-400'}`} />
          <span className="hidden lg:inline">{connected ? 'daemon' : connectionState === 'connecting' ? 'connexion' : 'hors ligne'}</span>
        </span>
        {onOpenDashboard && (
          <button type="button" onClick={onOpenDashboard} title="Tableau de bord" aria-label="Tableau de bord" className="btn btn-icon sm:w-auto sm:px-3 xl:hidden">
            <LayoutDashboard className="w-4 h-4" />
            <span className="hidden sm:inline text-[13px]">Tableau</span>
          </button>
        )}
        {onOpenSettings && (
          <button type="button" onClick={onOpenSettings} title="Réglages" aria-label="Réglages" className="btn btn-icon sm:w-auto sm:px-3">
            <SlidersHorizontal className="w-4 h-4" />
            <span className="hidden sm:inline text-[13px]">Réglages</span>
          </button>
        )}
        {onToggleConsole && (
          <button
            type="button"
            onClick={onToggleConsole}
            aria-label="Console des outils"
            className={`btn btn-icon sm:w-auto sm:px-3 relative ${isConsoleOpen ? 'border-brass-500 text-brass-300' : ''}`}
            title={isConsoleOpen ? 'Masquer la console des outils' : 'Afficher la console des outils'}
          >
            <Terminal className="w-4 h-4" />
            <span className="hidden sm:inline text-[13px]">Console</span>
            {runningToolsCount > 0 && (
              <span className="absolute -top-1.5 -right-1.5 min-w-5 h-5 px-1 rounded-full bg-alert-400 text-ink-950 text-[11px] font-semibold flex items-center justify-center">
                {runningToolsCount}
              </span>
            )}
          </button>
        )}
      </div>
    </header>
  );
};
