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

const Meter: React.FC<{ label: string; value: string; percent: number; mark: number; markTitle: string; ariaMax: number; ariaNow: number; hot: boolean }> = ({
  label,
  value,
  percent,
  mark,
  markTitle,
  ariaMax,
  ariaNow,
  hot,
}) => (
  <div className="flex-1 min-w-[150px] flex flex-col gap-1">
    <div className="flex justify-between text-[12px] text-ivory-500">
      <span>{label}</span>
      <span className={`font-mono tabular-nums ${hot ? 'text-listen-300' : 'text-ivory-300'}`}>{value}</span>
    </div>
    <div className="relative h-1.5 rounded-full bg-ink-700" role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={ariaMax} aria-valuenow={ariaNow}>
      <div className="h-1.5 rounded-full bg-listen-400 transition-[width] duration-100" style={{ width: `${percent}%` }} />
      <div className="absolute -top-1 w-0.5 h-3.5 bg-brass-400" style={{ left: `${mark}%` }} title={markTitle} />
    </div>
  </div>
);

/**
 * Bloc Voix : ce que le daemon entend (niveau micro, probabilite de parole) face au
 * seuil d'interruption, pour regler celle-ci sans lire les journaux.
 */
export const VoiceHud: React.FC<VoiceHudProps> = ({ meter, settings }) => {
  const threshold = settings?.barge_threshold ?? 0.75;
  const minRms = settings?.barge_min_rms ?? 0.012;
  const speechDetected = meter.vad >= threshold;
  const chips = [
    settings?.voice_barge_in ? 'interruption à la voix' : 'interruption coupée',
    settings?.aec ? 'écho annulé' : 'sans annulation d’écho',
    settings?.smart_turn ? 'fin de tour par modèle' : 'fin de tour sur silence',
  ];

  return (
    <div className="shrink-0 px-4 sm:px-6 py-2 border-y border-ink-800 bg-ink-900/40 flex flex-wrap items-center gap-x-6 gap-y-2">
      <Meter
        label="Micro"
        value={meter.rms.toFixed(3)}
        percent={levelPercent(meter.rms)}
        mark={levelPercent(minRms)}
        markTitle="Énergie minimale d'une interruption"
        ariaMax={0.2}
        ariaNow={Number(meter.rms.toFixed(3))}
        hot={false}
      />
      <Meter
        label="Parole"
        value={`${meter.vad.toFixed(2)} / ${threshold.toFixed(2)}`}
        percent={meter.vad * 100}
        mark={threshold * 100}
        markTitle="Seuil d'interruption"
        ariaMax={1}
        ariaNow={Number(meter.vad.toFixed(2))}
        hot={speechDetected}
      />
      <p className="m-0 text-[12px] text-ivory-700">
        {chips.join(' · ')}
        {meter.lastInterruptRms !== undefined ? ` · dernière interruption ${meter.lastInterruptRms.toFixed(3)}` : ''}
      </p>
    </div>
  );
};
