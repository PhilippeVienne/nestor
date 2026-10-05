import React from 'react';
import type { NestorSettings, VoiceMeter } from '../types';

interface VoiceHudProps {
  meter: VoiceMeter;
  settings: NestorSettings | null;
}

/** Echelle perceptive : un RMS de parole normale (~0,05) remplit environ la moitie de la jauge. */
function levelPercent(rms: number): number {
  return Math.max(0, Math.min(100, Math.sqrt(rms / 0.2) * 100));
}

const Chip: React.FC<{ active: boolean; children: React.ReactNode }> = ({ active, children }) => (
  <span
    className={`px-2 py-0.5 rounded-md text-[10px] sm:text-[11px] font-mono border whitespace-nowrap ${
      active
        ? 'bg-cyan-950/60 border-cyan-500/30 text-cyan-300'
        : 'bg-slate-900/60 border-slate-700/60 text-slate-400'
    }`}
  >
    {children}
  </span>
);

/**
 * Bloc Voix : ce que le daemon entend (niveau micro, probabilite de parole) face au
 * seuil d'interruption, pour regler celle-ci sans lire les journaux.
 */
export const VoiceHud: React.FC<VoiceHudProps> = ({ meter, settings }) => {
  const threshold = settings?.barge_threshold ?? 0.75;
  const minRms = settings?.barge_min_rms ?? 0.012;
  const speechDetected = meter.vad >= threshold;

  return (
    <div className="shrink-0 px-3 sm:px-6 py-2 border-y border-slate-800/60 bg-slate-950/60 flex flex-wrap items-center gap-x-6 gap-y-2 text-xs">
      <div className="flex-1 min-w-[150px] flex flex-col gap-1">
        <div className="flex justify-between text-slate-400">
          <span>Niveau micro</span>
          <span className="font-mono text-slate-300">{meter.rms.toFixed(3)}</span>
        </div>
        <div
          className="relative h-1.5 rounded-full bg-slate-800"
          role="meter"
          aria-label="Niveau du micro"
          aria-valuemin={0}
          aria-valuemax={0.2}
          aria-valuenow={Number(meter.rms.toFixed(3))}
        >
          <div className="h-1.5 rounded-full bg-cyan-400 transition-[width] duration-100" style={{ width: `${levelPercent(meter.rms)}%` }} />
          <div className="absolute -top-1 w-0.5 h-3.5 bg-amber-400" style={{ left: `${levelPercent(minRms)}%` }} title="Énergie minimale d'une interruption" />
        </div>
      </div>

      <div className="flex-1 min-w-[150px] flex flex-col gap-1">
        <div className="flex justify-between text-slate-400">
          <span>Parole détectée</span>
          <span className={`font-mono ${speechDetected ? 'text-cyan-300' : 'text-slate-300'}`}>
            {meter.vad.toFixed(2)} / seuil {threshold.toFixed(2)}
          </span>
        </div>
        <div
          className="relative h-1.5 rounded-full bg-slate-800"
          role="meter"
          aria-label="Probabilité de parole"
          aria-valuemin={0}
          aria-valuemax={1}
          aria-valuenow={Number(meter.vad.toFixed(2))}
        >
          <div className="h-1.5 rounded-full bg-cyan-400 transition-[width] duration-100" style={{ width: `${meter.vad * 100}%` }} />
          <div className="absolute -top-1 w-0.5 h-3.5 bg-amber-400" style={{ left: `${threshold * 100}%` }} title="Seuil d'interruption" />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <Chip active={!!settings?.voice_barge_in}>Interruption vocale {settings?.voice_barge_in ? 'active' : 'coupée'}</Chip>
        <Chip active={!!settings?.aec}>Annulation d'écho {settings?.aec ? 'active' : 'coupée'}</Chip>
        <Chip active={!!settings?.smart_turn}>Fin de tour : {settings?.smart_turn ? 'modèle' : 'silence fixe'}</Chip>
        {meter.lastInterruptRms !== undefined && (
          <Chip active={false}>Dernière interruption : {meter.lastInterruptRms.toFixed(3)}</Chip>
        )}
      </div>
    </div>
  );
};
