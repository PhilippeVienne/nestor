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
  allow: 'text-ok-300',
  confirm: 'text-alert-300',
  deny: 'text-danger-300',
};

function statusLabel(item: JudgeItem): string {
  if (item.resolved === 'approved') return 'Confirmation approuvée';
  if (item.resolved === 'refused') return 'Confirmation refusée';
  if (item.decision === 'confirm' && !item.pending && item.source === 'mission') return 'Confirmation demandée (mission)';
  return DECISION_LABEL[item.decision];
}

/** Boutons de reponse a une confirmation demandee par le juge. */
export const JudgeActions: React.FC<{ id: number; onResolve: (id: number, approve: boolean) => void }> = ({ id, onResolve }) => (
  <div className="flex flex-wrap gap-2">
    <button type="button" onClick={() => onResolve(id, true)} className="btn btn-primary flex-1 min-w-[110px]">
      Approuver
    </button>
    <button type="button" onClick={() => onResolve(id, false)} className="btn flex-1 min-w-[110px]">
      Refuser
    </button>
  </div>
);

/** Fil des decisions du juge de conscience, les confirmations en attente en tete. */
export const ConsciencePanel: React.FC<ConsciencePanelProps> = ({ judgements, model, onResolve }) => {
  const ordered = [...judgements].sort((a, b) => Number(b.pending) - Number(a.pending));
  return (
    <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-2.5 text-sm">
      {model !== undefined && (
        <div className="flex items-baseline justify-between gap-3 text-[12px] text-ivory-500">
          <span>Juge local</span>
          <span className="font-mono">{model}</span>
        </div>
      )}
      {ordered.length === 0 ? (
        <p className="m-0 text-[13px] text-ivory-700 py-6 text-center">
          Aucune décision pour l'instant. Chaque demande et chaque mission passe ici avant d'être exécutée.
        </p>
      ) : (
        ordered.map((item) => (
          <div
            key={item.id}
            className={`rounded-xl border p-3 flex flex-col gap-2 ${
              item.pending ? 'border-alert-600/60 bg-alert-600/10' : 'border-ink-800 bg-ink-900/50'
            }`}
          >
            <div className="flex items-baseline justify-between gap-3">
              <span className={`font-medium ${DECISION_COLOR[item.decision]}`}>{statusLabel(item)}</span>
              <span className="font-mono text-[11px] text-ivory-500 shrink-0 tabular-nums">
                {item.score !== undefined ? item.score : '—'}
                {item.category ? ` · ${item.category}` : ''}
              </span>
            </div>
            <div className="text-ivory-100 break-words">{item.text}</div>
            {item.rationale && item.decision !== 'allow' && <div className="text-[13px] text-ivory-500 break-words">{item.rationale}</div>}
            <div className="text-[11px] text-ivory-700">
              {item.source === 'mission' ? 'Mission' : 'Demande'} · {item.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            </div>
            {item.pending && <JudgeActions id={item.id} onResolve={onResolve} />}
          </div>
        ))
      )}
    </div>
  );
};
