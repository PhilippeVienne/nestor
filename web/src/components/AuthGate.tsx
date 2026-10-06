import React, { useCallback, useEffect, useState } from 'react';
import { KeyRound } from 'lucide-react';
import type { ConnectionState } from '../types';
import {
  daemonHttpBase,
  enrollPasskey,
  fetchAuthStatus,
  loginWithPasskey,
  passkeysSupported,
  type AuthStatus,
} from '../auth/passkey';

interface AuthGateProps {
  wsUrl: string;
  connectionState: ConnectionState;
  /** Jeton de session obtenu : l'interface se reconnecte avec. */
  onSession: (token: string) => void;
}

/** Code d'enrolement porte par le lien de `nestord onboard --passkey` (`?enroll=…`). */
function readEnrollCode(): string | null {
  return new URLSearchParams(window.location.search).get('enroll');
}

/**
 * Bandeau d'authentification : enrolement d'une passkey a l'ouverture d'un lien
 * d'onboarding, puis connexion par passkey quand le daemon exige une session.
 */
export const AuthGate: React.FC<AuthGateProps> = ({ wsUrl, connectionState, onSession }) => {
  const base = daemonHttpBase(wsUrl);
  const [enrollCode, setEnrollCode] = useState<string | null>(() => readEnrollCode());
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const connected = connectionState === 'connected';

  // Interroge le daemon tant que l'interface n'est pas connectee.
  useEffect(() => {
    if (connected && !enrollCode) return;
    let cancelled = false;
    const refresh = () =>
      fetchAuthStatus(base)
        .then((s) => !cancelled && setStatus(s))
        .catch(() => !cancelled && setStatus(null));
    refresh();
    const timer = window.setInterval(refresh, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [base, connected, enrollCode]);

  const run = useCallback(
    async (action: () => Promise<string>, afterSuccess?: () => void) => {
      setBusy(true);
      setError(null);
      try {
        onSession(await action());
        afterSuccess?.();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(false);
      }
    },
    [onSession]
  );

  const enroll = () =>
    run(
      () => enrollPasskey(base, enrollCode ?? ''),
      () => {
        // Le code est consomme : on le retire de l'adresse.
        const url = new URL(window.location.href);
        url.searchParams.delete('enroll');
        window.history.replaceState(null, '', url.toString());
        setEnrollCode(null);
      }
    );
  const login = () => run(() => loginWithPasskey(base));

  const wantsLogin = !connected && !!status && status.passkeys > 0;
  if (!enrollCode && !wantsLogin) return null;

  const localhostUrl = `${window.location.protocol}//localhost${window.location.port ? `:${window.location.port}` : ''}${window.location.pathname}${window.location.search}`;
  const domainProblem = status && !status.domain_ok;

  return (
    <div className="shrink-0 z-20 border-b border-cyan-500/40 bg-cyan-950/40 px-3 sm:px-6 py-3 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm select-text">
      <KeyRound className="w-5 h-5 text-cyan-300 shrink-0" />
      <div className="flex-1 min-w-[220px]">
        <div className="font-semibold text-cyan-100">
          {enrollCode ? 'Enregistrer une passkey pour ce navigateur' : 'Connexion à Nestor par passkey'}
        </div>
        {!passkeysSupported() ? (
          <div className="text-amber-200">Ce navigateur ne prend pas en charge les passkeys.</div>
        ) : domainProblem ? (
          <div className="text-amber-200">
            Une passkey est liée à un nom de domaine. Ouvrez l'interface sur{' '}
            <a className="underline text-amber-100" href={localhostUrl}>
              {localhostUrl}
            </a>{' '}
            plutôt que sur une adresse IP.
          </div>
        ) : (
          <div className="text-slate-200">
            {enrollCode
              ? "Le lien d'onboarding autorise cet enregistrement une seule fois. Votre navigateur va vous demander de créer la passkey."
              : 'Le daemon demande votre passkey pour ouvrir la session.'}
          </div>
        )}
        {error && <div className="text-rose-300 break-words">{error}</div>}
      </div>
      {passkeysSupported() && !domainProblem && (
        <button
          type="button"
          disabled={busy}
          onClick={enrollCode ? enroll : login}
          className="h-11 px-5 rounded-lg border border-cyan-400 bg-cyan-900/60 text-cyan-50 font-semibold hover:bg-cyan-800/70 disabled:opacity-60 transition-colors"
        >
          {busy ? 'En cours…' : enrollCode ? 'Créer la passkey' : 'Se connecter'}
        </button>
      )}
    </div>
  );
};
