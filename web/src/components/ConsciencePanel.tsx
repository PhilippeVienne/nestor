import React from 'react';
import type { JudgeItem } from '../types';

interface ConsciencePanelProps {
  judgements: JudgeItem[];
  model?: string;
  onResolve: (id: number, approve: boolean) => void;
}

const DECISION_LABEL: Record<JudgeItem['decision'], string> = {
  allow: 'Autorisé',
  confirm: 'Confirmation requise',
  deny: 'Refusé',
};

const DECISION_COLOR: Record<JudgeItem['decision'], string> = {
  allow: 'text-cyan-300',
  confirm: 'text-amber-300',
  deny: 'text-rose-300',
};

function statusLabel(item: JudgeItem): string {
  if (item.resolved === 'approved') return 'Confirmation approuvée';
  if (item.resolved === 'refused') return 'Confirmation refusée';
  if (item.decision === 'confirm' && !item.pending && item.source === 'mission') return 'Confirmation demandée (mission)';
  return DECISION_LABEL[item.decision];
}

/** Boutons de reponse a une confirmation demandee par le juge. */
export const JudgeActions: React.FC<{ id: number; onResolve: (id: number, approve: boolean) => void }> = ({
  id,
  onResolve,
}) => (
  <div className="flex flex-wrap gap-2">
    <button
      type="button"
      onClick={() => onResolve(id, true)}
      className="flex-1 min-w-[110px] h-11 rounded-lg border border-cyan-400 bg-cyan-900/60 text-cyan-50 font-semibold hover:bg-cyan-800/70 transition-colors"
    >
      Approuver
    </button>
    <button
      type="button"
      onClick={() => onResolve(id, false)}
      className="flex-1 min-w-[110px] h-11 rounded-lg border border-slate-600 text-slate-200 font-semibold hover:border-slate-400 transition-colors"
    >
      Refuser
    </button>
  </div>
);

/** Fil des decisions du juge de conscience, les confirmations en attente en tete. */
export const ConsciencePanel: React.FC<ConsciencePanelProps> = ({ judgements, model, onResolve }) => {
  const ordered = [...judgements].sort((a, b) => Number(b.pending) - Number(a.pending));
  return (
    <div className="flex-1 overflow-y-auto p-3 space-y-2.5 text-sm">
      <div className="flex items-baseline justify-between gap-3 text-[11px] font-mono text-slate-400">
        <span className="uppercase tracking-wider">Juge local</span>
        <span>{model ?? '—'}</span>
      </div>
      {ordered.length === 0 ? (
        <p className="text-xs text-slate-500 py-6 text-center">
          Aucune décision pour l'instant. Chaque demande et chaque mission passe ici avant d'être exécutée.
        </p>
      ) : (
        ordered.map((item) => (
          <div
            key={item.id}
            className={`rounded-lg border p-3 flex flex-col gap-2 ${
              item.pending ? 'border-amber-500/50 bg-amber-950/20' : 'border-slate-800 bg-slate-900/40'
            }`}
          >
            <div className="flex items-baseline justify-between gap-3">
              <span className={`font-semibold ${DECISION_COLOR[item.decision]}`}>{statusLabel(item)}</span>
              <span className="font-mono text-[11px] text-slate-400 shrink-0">
                {item.score !== undefined ? item.score : '—'}
                {item.category ? ` · ${item.category}` : ''}
              </span>
            </div>
            <div className="text-slate-200 break-words">{item.text}</div>
            {item.rationale && item.decision !== 'allow' && (
              <div className="text-[13px] text-slate-400 break-words">{item.rationale}</div>
            )}
            <div className="text-[10px] font-mono text-slate-500">
              {item.source === 'mission' ? 'Mission' : 'Demande'} · {item.timestamp.toLocaleTimeString()}
            </div>
            {item.pending && <JudgeActions id={item.id} onResolve={onResolve} />}
          </div>
        ))
      )}
    </div>
  );
};
