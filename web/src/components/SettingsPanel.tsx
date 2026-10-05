import React, { useEffect, useState } from 'react';
import { X } from 'lucide-react';
import type { NestorSettings } from '../types';

interface SettingsPanelProps {
  isOpen: boolean;
  onClose: () => void;
  settings: NestorSettings | null;
  onChange: (patch: Partial<NestorSettings>) => void;
  /** Niveau moyen de la derniere interruption vocale, pour caler l'energie minimale. */
  lastInterruptRms?: number;
}

const JUDGE_MODELS = ['qwen2.5:1.5b', 'llama3.2:3b', 'llama3.2:1b'];

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <section className="flex flex-col gap-3 rounded-xl border border-slate-800 bg-slate-900/50 p-4">
    <h3 className="text-[11px] font-mono font-semibold tracking-[0.14em] text-slate-400 uppercase">{title}</h3>
    {children}
  </section>
);

const Toggle: React.FC<{
  label: string;
  hint: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}> = ({ label, hint, checked, onChange }) => (
  <label className="flex items-start gap-3 min-h-11 cursor-pointer">
    <input
      type="checkbox"
      className="mt-1 w-5 h-5 accent-cyan-400 shrink-0"
      checked={checked}
      onChange={(e) => onChange(e.target.checked)}
    />
    <span>
      <span className="block font-semibold text-slate-100">{label}</span>
      <span className="block text-[13px] text-slate-400">{hint}</span>
    </span>
  </label>
);

/** Curseur : la valeur suit le geste, mais n'est envoyee au daemon qu'au relachement. */
const Range: React.FC<{
  id: string;
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (v: number) => string;
  onCommit: (v: number) => void;
}> = ({ id, label, hint, value, min, max, step, format, onCommit }) => {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (draft !== value) onCommit(draft);
  };
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex justify-between gap-3">
        <label htmlFor={id} className="text-slate-200">
          {label}
        </label>
        <span className="font-mono text-cyan-300">{format(draft)}</span>
      </div>
      <input
        id={id}
        type="range"
        className="w-full h-7 accent-cyan-400"
        min={min}
        max={max}
        step={step}
        value={draft}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      {hint && <span className="text-[13px] text-slate-400">{hint}</span>}
    </div>
  );
};

/** Champ numerique valide a la sortie du champ (ou sur Entree). */
const NumberField: React.FC<{
  id: string;
  label: string;
  value: number;
  onCommit: (v: number) => void;
}> = ({ id, label, value, onCommit }) => {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const parsed = Number(draft);
    if (Number.isFinite(parsed) && parsed !== value) onCommit(parsed);
    else setDraft(String(value));
  };
  return (
    <div className="flex-1 min-w-[130px] flex flex-col gap-1.5">
      <label htmlFor={id} className="text-slate-200">
        {label}
      </label>
      <input
        id={id}
        type="number"
        min={1}
        max={100}
        className="h-11 px-3 rounded-lg border border-slate-700 bg-slate-950 text-slate-100"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === 'Enter' && commit()}
      />
    </div>
  );
};

/** Ecran « Reglages » : chaque changement est applique a chaud et enregistre par le daemon. */
export const SettingsPanel: React.FC<SettingsPanelProps> = ({ isOpen, onClose, settings, onChange, lastInterruptRms }) => {
  if (!isOpen) return null;

  const models = settings && !JUDGE_MODELS.includes(settings.judge_model)
    ? [settings.judge_model, ...JUDGE_MODELS]
    : JUDGE_MODELS;

  return (
    <div className="absolute inset-0 z-30 flex justify-end select-text">
      <button type="button" aria-label="Fermer les réglages" className="flex-1 bg-black/50" onClick={onClose} />
      <aside
        aria-label="Réglages"
        className="w-full sm:w-[440px] h-full overflow-y-auto bg-[#0b111e] border-l border-slate-800 p-4 sm:p-5 flex flex-col gap-4 text-sm"
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="text-lg font-semibold text-slate-100">Réglages</h2>
          <button
            type="button"
            aria-label="Fermer"
            onClick={onClose}
            className="w-11 h-11 inline-flex items-center justify-center rounded-lg border border-slate-700 text-slate-300 hover:text-white hover:border-slate-500"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {!settings ? (
          <p className="text-slate-400">En attente des réglages du daemon : nestord n'est pas connecté.</p>
        ) : (
          <>
            <Section title="Écoute et interruption">
              <Toggle
                label="Interruption à la voix"
                hint="Parler par-dessus Nestor coupe sa réponse."
                checked={settings.voice_barge_in}
                onChange={(v) => onChange({ voice_barge_in: v })}
              />
              <Toggle
                label="Annulation d'écho côté serveur"
                hint="Retire la voix de Nestor du micro pendant qu'il parle."
                checked={settings.aec}
                onChange={(v) => onChange({ aec: v })}
              />
              <Toggle
                label="Fin de tour intelligente"
                hint="Prototype Smart Turn. Désactivé : silence fixe de 700 ms."
                checked={settings.smart_turn}
                onChange={(v) => onChange({ smart_turn: v })}
              />
              <div className="border-t border-slate-800 pt-3 flex flex-col gap-4">
                <Range
                  id="barge-threshold"
                  label="Seuil de parole"
                  hint="Plus bas : interruption plus facile, mais plus sensible à l'écho."
                  value={settings.barge_threshold}
                  min={0.3}
                  max={0.95}
                  step={0.05}
                  format={(v) => v.toFixed(2)}
                  onCommit={(v) => onChange({ barge_threshold: v })}
                />
                <Range
                  id="barge-duration"
                  label="Durée de parole exigée"
                  value={settings.barge_min_speech_ms}
                  min={100}
                  max={1000}
                  step={50}
                  format={(v) => `${v} ms`}
                  onCommit={(v) => onChange({ barge_min_speech_ms: v })}
                />
                <Range
                  id="barge-rms"
                  label="Énergie minimale de la voix"
                  hint={
                    lastInterruptRms !== undefined
                      ? `Dernière interruption détectée : niveau moyen ${lastInterruptRms.toFixed(3)}.`
                      : 'Aucune interruption vocale détectée depuis la connexion.'
                  }
                  value={settings.barge_min_rms}
                  min={0}
                  max={0.05}
                  step={0.002}
                  format={(v) => v.toFixed(3)}
                  onCommit={(v) => onChange({ barge_min_rms: v })}
                />
              </div>
            </Section>

            <Section title="Mot-clé">
              <Toggle
                label="Exiger « Hey Nestor » en veille"
                hint="Désactivé : Nestor répond à tout ce qu'il entend."
                checked={settings.wake_word_enabled}
                onChange={(v) => onChange({ wake_word_enabled: v })}
              />
              <Range
                id="wake-window"
                label="Fenêtre de dialogue sans mot-clé"
                value={settings.wake_timeout_secs}
                min={5}
                max={120}
                step={5}
                format={(v) => `${v} s`}
                onCommit={(v) => onChange({ wake_timeout_secs: v })}
              />
            </Section>

            <Section title="Conscience (juge local)">
              <div className="flex flex-col gap-1.5">
                <label htmlFor="judge-model" className="text-slate-200">
                  Modèle
                </label>
                <select
                  id="judge-model"
                  className="h-11 px-3 rounded-lg border border-slate-700 bg-slate-950 text-slate-100"
                  value={settings.judge_model}
                  onChange={(e) => onChange({ judge_model: e.target.value })}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
                <span className="text-[13px] text-slate-400">
                  Batterie de 13 phrases : qwen2.5:1.5b sans erreur, llama3.2:1b bloque une phrase saine, llama3.2:3b
                  laisse passer un danger.
                </span>
              </div>
              <div className="flex flex-wrap gap-3">
                <NumberField
                  id="judge-confirm"
                  label="Confirmation à partir de"
                  value={settings.judge_confirm_threshold}
                  onCommit={(v) => onChange({ judge_confirm_threshold: v })}
                />
                <NumberField
                  id="judge-reject"
                  label="Refus à partir de"
                  value={settings.judge_reject_threshold}
                  onCommit={(v) => onChange({ judge_reject_threshold: v })}
                />
              </div>
            </Section>

            <p className="text-[13px] text-slate-400">
              Chaque changement s'applique tout de suite et est enregistré dans <span className="font-mono">ui-settings.toml</span>.
              Les mots d'activation et la voix se règlent encore dans <span className="font-mono">config.toml</span>.
            </p>
          </>
        )}
      </aside>
    </div>
  );
};
