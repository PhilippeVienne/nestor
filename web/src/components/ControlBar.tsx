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

  const handleSubmit = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
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

  const canBargeIn = status === 'speaking' || status === 'thinking';

  return (
    <div className="w-full max-w-3xl mx-auto px-4 py-2.5 flex flex-col items-center">
      {/* Sleek Unified Control Capsule */}
      <div className="w-full relative flex items-center gap-2 bg-slate-900/80 border border-slate-800/80 rounded-2xl px-2.5 py-1.5 shadow-lg backdrop-blur-xl transition-all focus-within:border-cyan-500/40 focus-within:bg-slate-900/95">
        {/* Subtle dynamic voice wave border when speaking or listening */}
        {(isMicActive || status === 'speaking') && (
          <div
            className="absolute -bottom-[1px] left-4 right-4 h-[2px] rounded-full transition-all duration-75 overflow-hidden"
            style={{
              opacity: audioLevels.rms > 0.01 ? 1 : 0.2,
            }}
          >
            <div
              className={`h-full transition-all duration-75 ${
                status === 'speaking'
                  ? 'bg-amber-400'
                  : 'bg-cyan-400'
              }`}
              style={{
                width: `${Math.min(100, Math.max(8, audioLevels.rms * 120))}%`,
                margin: '0 auto',
              }}
            />
          </div>
        )}

        {/* Toggle Microphone Capture Button */}
        <button
          type="button"
          onClick={onToggleMic}
          className={`p-2.5 rounded-xl transition-all duration-200 shrink-0 flex items-center justify-center ${
            isMicActive
              ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 shadow-[0_0_12px_rgba(16,185,129,0.25)]'
              : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800/60'
          }`}
          title={isMicActive ? 'Désactiver le microphone' : 'Activer le microphone web'}
        >
          {isMicActive ? (
            <Mic className="w-4 h-4 animate-pulse text-emerald-400" />
          ) : (
            <MicOff className="w-4 h-4" />
          )}
        </button>

        {/* Text Input Form */}
        <form onSubmit={handleSubmit} className="flex-1 flex items-center min-w-0">
          <input
            ref={inputRef}
            type="text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={
              isMicActive
                ? 'Micro actif... vous pouvez parler ou écrire'
                : 'Formulez votre consigne à Nestor...'
            }
            className="w-full bg-transparent border-none text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:ring-0 px-2 py-1 font-sans"
          />
        </form>

        {/* Right Action Buttons */}
        <div className="flex items-center gap-1 shrink-0">
          {/* Barge-in Stop Button (Contextual: only when speaking or thinking) */}
          {canBargeIn && (
            <button
              type="button"
              onClick={onBargeIn}
              className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-300 border border-rose-500/40 text-xs font-mono transition-all animate-pulse shadow-sm"
              title="Interrompre Nestor immédiatement (Échap)"
            >
              <Square className="w-3 h-3 fill-current" />
              <span className="text-[11px] hidden sm:inline">Stop</span>
            </button>
          )}

          {/* Toggle Speaker Output Button */}
          <button
            type="button"
            onClick={onToggleSpeaker}
            className={`p-2 rounded-lg transition-colors ${
              isSpeakerActive
                ? 'text-slate-400 hover:text-slate-200'
                : 'text-slate-600 hover:text-slate-400'
            }`}
            title={isSpeakerActive ? 'Couper la voix' : 'Activer la voix'}
          >
            {isSpeakerActive ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
          </button>

          {/* Send text button */}
          <button
            type="button"
            onClick={() => handleSubmit()}
            disabled={!text.trim()}
            className="p-2 rounded-xl bg-cyan-600 hover:bg-cyan-500 text-white disabled:opacity-20 disabled:hover:bg-cyan-600 transition-all shadow-md shrink-0"
            title="Envoyer (Entrée)"
          >
            <Send className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Subtle Hint */}
      <div className="flex items-center gap-4 text-[10px] text-slate-500 font-sans mt-1">
        <span>Échap pour interrompre</span>
        <span>•</span>
        <span>Entrée pour envoyer</span>
      </div>
    </div>
  );
};

