import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';

const FOCUS = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brass-300';

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
        className={`self-start min-h-11 inline-flex items-center gap-1.5 rounded-lg text-[13px] text-ivory-500 underline-offset-4 hover:text-brass-300 hover:underline ${FOCUS}`}
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
              className="field flex-1 min-w-0"
            />
            <button
              type="submit"
              disabled={disabled || !draft.trim()}
              className="btn shrink-0"
            >
              Utiliser
            </button>
          </div>
          <p className="m-0 text-[13px] text-ivory-500">
            Jeton affiché une seule fois par <span className="font-mono text-ivory-100">nestord onboard</span>. Il est
            mémorisé dans ce navigateur jusqu'à la déconnexion.
          </p>
        </form>
      )}
    </div>
  );
};
