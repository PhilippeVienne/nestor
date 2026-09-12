import React, { useState, useEffect, useRef } from 'react';
import { Send, Square, Mic, MicOff, Volume2, VolumeX } from 'lucide-react';
import type { DaemonStatus, AudioLevels } from '../types';

interface ControlBarProps {
  status: DaemonStatus;
  audioLevels: AudioLevels;
  isMicActive: boolean;
  onToggleMic: () => void;
  isSpeakerActive: boolean;
  onToggleSpeaker: () => void;
  onBargeIn: () => void;
  onSendText: (content: string) => void;
}

export const ControlBar: React.FC<ControlBarProps> = ({
  status,
  audioLevels,
  isMicActive,
  onToggleMic,
  isSpeakerActive,
  onToggleSpeaker,
  onBargeIn,
  onSendText,
}) => {
  const [text, setText] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!text.trim()) return;
    onSendText(text);
    setText('');
  };

  // Keyboard shortcut: Escape triggers barge-in
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onBargeIn();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onBargeIn]);

  const canBargeIn = status === 'speaking' || status === 'thinking' || status === 'listening';

  return (
    <div className="w-full max-w-5xl mx-auto px-4 py-3">
      {/* Real-time audio VU-meter & Audio stream info */}
      <div className="flex items-center gap-3 mb-2 px-2 text-[11px] font-mono text-slate-400">
        <div className="flex items-center gap-1.5 shrink-0">
          {status === 'speaking' ? (
            <Volume2 className="w-3.5 h-3.5 text-amber-400 animate-pulse" />
          ) : isMicActive ? (
            <Mic className="w-3.5 h-3.5 text-emerald-400 animate-pulse" />
          ) : (
            <MicOff className="w-3.5 h-3.5 text-slate-500" />
          )}
          <span className="capitalize text-slate-300">
            {status === 'speaking'
              ? 'Sortie Audio Web (Kokoro TTS)'
              : isMicActive
              ? 'Micro Web distant (16 kHz PCM)'
              : 'Micro désactivé'}
          </span>
        </div>

        {/* Dynamic VU meter */}
        <div className="flex-1 h-2 bg-slate-900 rounded-full overflow-hidden border border-slate-800 p-0.5 flex items-center">
          <div
            className={`h-full rounded-full transition-all duration-75 ${
              status === 'speaking'
                ? 'bg-gradient-to-r from-amber-500 to-yellow-300'
                : 'bg-gradient-to-r from-cyan-500 via-emerald-400 to-rose-500'
            }`}
            style={{
              width: `${Math.min(100, Math.max(4, audioLevels.rms * 100))}%`,
              opacity: audioLevels.rms > 0.01 ? 1 : 0.25,
            }}
          />
        </div>

        <div className="shrink-0 flex items-center gap-2 text-[10px]">
          <span>
            RMS: <strong className="text-slate-200">{(audioLevels.rms).toFixed(2)}</strong>
          </span>
          <span>
            Peak: <strong className="text-slate-200">{(audioLevels.peak).toFixed(2)}</strong>
          </span>
        </div>
      </div>

      {/* Main input & Remote Audio controls */}
      <div className="flex items-center gap-2">
        {/* Toggle Microphone Capture Button */}
        <button
          type="button"
          onClick={onToggleMic}
          className={`flex items-center gap-2 px-3.5 py-3 rounded-xl font-mono text-xs font-semibold tracking-wider transition-all duration-200 border shrink-0 shadow-lg ${
            isMicActive
              ? 'bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-300 border-emerald-500/40 shadow-[0_0_15px_rgba(16,185,129,0.25)]'
              : 'bg-slate-900/80 hover:bg-slate-800 text-slate-400 border-slate-800 hover:border-slate-700'
          }`}
          title={isMicActive ? 'Couper le microphone web' : 'Activer la capture micro dans le navigateur'}
        >
          {isMicActive ? (
            <>
              <span className="w-2 h-2 rounded-full bg-emerald-400 animate-pulse" />
              <Mic className="w-4 h-4 text-emerald-400" />
              <span className="hidden sm:inline">Micro Actif</span>
            </>
          ) : (
            <>
              <MicOff className="w-4 h-4 text-slate-500" />
              <span className="hidden sm:inline">Activer Micro</span>
            </>
          )}
        </button>

        {/* Toggle Speaker Output Button */}
        <button
          type="button"
          onClick={onToggleSpeaker}
          className={`p-3 rounded-xl font-mono text-xs transition-all duration-200 border shrink-0 ${
            isSpeakerActive
              ? 'bg-slate-900/80 text-cyan-400 border-slate-800 hover:border-cyan-500/40'
              : 'bg-slate-900/40 text-slate-600 border-slate-800'
          }`}
          title={isSpeakerActive ? 'Désactiver la voix audio' : 'Activer la voix audio dans le navigateur'}
        >
          {isSpeakerActive ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
        </button>

        {/* Barge-in Button */}
        <button
          type="button"
          onClick={onBargeIn}
          className={`flex items-center gap-2 px-3.5 py-3 rounded-xl font-mono text-xs font-semibold tracking-wider transition-all duration-200 border shrink-0 shadow-lg ${
            canBargeIn
              ? 'bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border-rose-500/40 hover:border-rose-400 shadow-[0_0_15px_rgba(244,63,94,0.2)] animate-pulse'
              : 'bg-slate-900/60 text-slate-500 border-slate-800 hover:text-slate-400'
          }`}
          title="Interrompre immédiatement la parole ou l'inférence de Nestor (Touche Échap)"
        >
          <Square className="w-4 h-4 fill-current" />
          <span className="hidden md:inline">Interrompre</span>
          <kbd className="hidden lg:inline-block px-1.5 py-0.5 rounded bg-black/40 text-[9px] text-slate-400 border border-white/10">
            Échap
          </kbd>
        </button>

        {/* Fallback Text Input Form */}
        <form onSubmit={handleSubmit} className="flex-1 flex items-center gap-2">
          <div className="relative flex-1">
            <input
              ref={inputRef}
              type="text"
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="Parler dans le micro ou taper votre commande..."
              className="w-full px-4 py-3 rounded-xl bg-slate-900/80 border border-slate-800 text-slate-100 placeholder-slate-500 text-sm focus:outline-none focus:border-cyan-500/60 focus:ring-1 focus:ring-cyan-500/30 transition-all font-sans"
            />
          </div>

          {/* Send text button */}
          <button
            type="submit"
            disabled={!text.trim()}
            className="px-4 py-3 rounded-xl bg-gradient-to-r from-cyan-600 to-blue-600 hover:from-cyan-500 hover:to-blue-500 text-white font-medium text-xs disabled:opacity-30 disabled:cursor-not-allowed transition-all duration-200 shadow-[0_0_15px_rgba(6,182,212,0.25)] flex items-center gap-1.5 shrink-0"
            title="Envoyer à Nestor (Entrée)"
          >
            <Send className="w-4 h-4" />
            <span className="hidden sm:inline">Envoyer</span>
          </button>
        </form>
      </div>
    </div>
  );
};
