import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';

const FOCUS = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-300';

interface TokenFieldProps {
  /** Identifiant unique du champ (le composant sert sur l'ecran de connexion et dans les reglages). */
  id: string;
  onSubmit: (token: string) => void;
  /** Champ deplie d'emblee, quand le jeton est la seule voie possible. */
  defaultOpen?: boolean;
  disabled?: boolean;
}

/** Saisie d'un jeton d'acces, en second plan : un lien discret deplie le champ. */
export const TokenField: React.FC<TokenFieldProps> = ({ id, onSubmit, defaultOpen = false, disabled = false }) => {
  const [open, setOpen] = useState(defaultOpen);
  const [draft, setDraft] = useState('');
  const Chevron = open ? ChevronDown : ChevronRight;

  return (
    <div className="flex flex-col gap-2">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={`${id}-form`}
        onClick={() => setOpen(!open)}
        className={`self-start min-h-11 inline-flex items-center gap-1.5 rounded-lg text-[13px] text-slate-400 underline-offset-4 hover:text-cyan-200 hover:underline ${FOCUS}`}
      >
        <Chevron className="w-4 h-4 shrink-0" aria-hidden="true" />
        Utiliser un jeton d'accès
      </button>
      {open && (
        <form
          id={`${id}-form`}
          className="flex flex-col gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            onSubmit(draft);
          }}
        >
          <div className="flex gap-2">
            <label htmlFor={id} className="sr-only">
              Jeton d'accès
            </label>
            <input
              id={id}
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="Jeton d'accès"
              className={`flex-1 min-w-0 h-11 px-3 rounded-lg border border-slate-700 bg-slate-950 text-slate-100 placeholder-slate-500 ${FOCUS}`}
            />
            <button
              type="submit"
              disabled={disabled || !draft.trim()}
              className={`h-11 px-4 rounded-lg border border-slate-600 bg-slate-900 text-slate-100 font-semibold hover:border-cyan-500/60 hover:text-cyan-50 disabled:opacity-50 shrink-0 ${FOCUS}`}
            >
              Utiliser
            </button>
          </div>
          <p className="text-[13px] text-slate-400">
            Jeton affiché une seule fois par <span className="font-mono text-slate-200">nestord onboard</span>. Il est
            mémorisé dans ce navigateur jusqu'à la déconnexion.
          </p>
        </form>
      )}
    </div>
  );
};
