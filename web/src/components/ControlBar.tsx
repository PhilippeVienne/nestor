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

  // Echap interrompt Nestor.
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onBargeIn();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onBargeIn]);

  const canBargeIn = status === 'speaking' || status === 'thinking';

  return (
    <div className="w-full max-w-3xl mx-auto px-4 py-3 flex flex-col items-center gap-1.5">
      <div className="w-full relative flex items-center gap-1.5 rounded-2xl border border-ink-700 bg-ink-900/90 px-2 py-1.5 transition-colors focus-within:border-brass-500">
        {/* Trait de niveau sous la capsule : ce que le micro entend, ou la voix de Nestor */}
        {(isMicActive || status === 'speaking') && (
          <div className="absolute -bottom-px left-6 right-6 h-[2px] rounded-full overflow-hidden" style={{ opacity: audioLevels.rms > 0.01 ? 1 : 0.15 }}>
            <div
              className={`h-full mx-auto transition-all duration-75 ${status === 'speaking' ? 'bg-brass-400' : 'bg-listen-400'}`}
              style={{ width: `${Math.min(100, Math.max(8, audioLevels.rms * 120))}%` }}
            />
          </div>
        )}

        <button
          type="button"
          onClick={onToggleMic}
          className={`btn btn-icon min-h-10 w-10 rounded-xl ${isMicActive ? 'border-listen-600 text-listen-300 bg-listen-600/15' : 'btn-quiet'}`}
          title={isMicActive ? 'Couper le micro' : 'Activer le micro'}
          aria-label={isMicActive ? 'Couper le micro' : 'Activer le micro'}
          aria-pressed={isMicActive}
        >
          {isMicActive ? <Mic className="w-4 h-4" /> : <MicOff className="w-4 h-4" />}
        </button>

        <form onSubmit={handleSubmit} className="flex-1 flex items-center min-w-0">
          <input
            ref={inputRef}
            type="text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={isMicActive ? 'Parlez, ou écrivez à Nestor…' : 'Écrivez à Nestor…'}
            className="w-full bg-transparent border-none text-[15px] text-ivory-100 placeholder-ivory-700 focus:outline-none px-2 py-1"
            aria-label="Message à Nestor"
          />
        </form>

        <div className="flex items-center gap-1 shrink-0">
          {canBargeIn && (
            <button
              type="button"
              onClick={onBargeIn}
              className="btn min-h-10 px-3 rounded-xl border-danger-600/60 text-danger-300 bg-danger-600/10 hover:bg-danger-600/20"
              title="Interrompre Nestor (Échap)"
            >
              <Square className="w-3 h-3 fill-current" />
              <span className="hidden sm:inline text-[13px]">Interrompre</span>
            </button>
          )}
          <button
            type="button"
            onClick={onToggleSpeaker}
            className={`btn btn-quiet btn-icon min-h-10 w-10 rounded-xl ${isSpeakerActive ? '' : 'text-ivory-700'}`}
            title={isSpeakerActive ? 'Couper la voix' : 'Activer la voix'}
            aria-label={isSpeakerActive ? 'Couper la voix' : 'Activer la voix'}
            aria-pressed={isSpeakerActive}
          >
            {isSpeakerActive ? <Volume2 className="w-4 h-4" /> : <VolumeX className="w-4 h-4" />}
          </button>
          <button
            type="button"
            onClick={() => handleSubmit()}
            disabled={!text.trim()}
            className="btn btn-primary btn-icon min-h-10 w-10 rounded-xl"
            title="Envoyer (Entrée)"
            aria-label="Envoyer"
          >
            <Send className="w-4 h-4" />
          </button>
        </div>
      </div>
      <p className="m-0 text-[11px] text-ivory-700">Échap pour interrompre · Entrée pour envoyer</p>
    </div>
  );
};
