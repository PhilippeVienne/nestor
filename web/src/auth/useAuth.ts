import { useCallback, useEffect, useRef, useState } from 'react';
import {
  closeSession,
  daemonHttpBase,
  describeAuthError,
  enrollPasskey,
  fetchAuthStatus,
  loginWithPasskey,
  type AuthStatus,
} from './passkey';
import {
  DEFAULT_WS_URL,
  clearCredentials,
  clearSession,
  readCredential,
  storeAccessToken,
  storeSession,
  type CredentialKind,
} from './session';

/**
 * Etape de l'acces au daemon :
 * - `checking` : premiere interrogation de `/auth/status` ;
 * - `unreachable` : le daemon ne repond pas (nouvel essai automatique) ;
 * - `login` : une preuve d'acces est attendue de l'utilisateur ;
 * - `opening` : une preuve existe, le WebSocket la presente au daemon ;
 * - `ready` : l'application est ouverte.
 */
export type AuthPhase = 'checking' | 'unreachable' | 'login' | 'opening' | 'ready';

const RETRY_MS = 3000;
const REFRESH_MS = 5000;

/** Code d'enrolement porte par le lien de `nestord onboard --passkey` (`?enroll=…`). */
function readEnrollCode(): string | null {
  return new URLSearchParams(window.location.search).get('enroll') || null;
}

function dropEnrollCodeFromUrl() {
  const url = new URL(window.location.href);
  url.searchParams.delete('enroll');
  window.history.replaceState(null, '', url.toString());
}

/**
 * Etat d'authentification de l'interface. L'application (et son WebSocket) n'est
 * montee qu'en phase `opening` ou `ready` : rien ne se connecte avant.
 */
export function useAuth(wsUrl: string = DEFAULT_WS_URL) {
  const base = daemonHttpBase(wsUrl);
  const [phase, setPhase] = useState<AuthPhase>('checking');
  const [status, setStatus] = useState<AuthStatus | null>(null);
  const [enrollCode, setEnrollCode] = useState<string | null>(readEnrollCode);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Phrase expliquant un retour a l'ecran de connexion (session expiree, deconnexion...). */
  const [notice, setNotice] = useState<string | null>(null);
  /** Change a chaque nouvelle preuve d'acces : l'application est remontee avec elle. */
  const [epoch, setEpoch] = useState(0);
  const [credentialKind, setCredentialKind] = useState<CredentialKind | null>(() => readCredential()?.kind ?? null);

  const phaseRef = useRef(phase);
  const enrollRef = useRef(enrollCode);
  useEffect(() => {
    phaseRef.current = phase;
    enrollRef.current = enrollCode;
  }, [phase, enrollCode]);
  // Vrai apres un refus du daemon ou une deconnexion : la preuve memorisee n'est plus
  // presentee d'office, il faut un geste de l'utilisateur.
  const heldRef = useRef(false);
  const refusalsRef = useRef(0);
  const checkSeqRef = useRef(0);

  const go = useCallback((next: AuthPhase) => {
    phaseRef.current = next;
    setPhase(next);
  }, []);

  /** Interroge le daemon et en deduit l'etape, sans jamais quitter l'application ouverte. */
  const check = useCallback(async () => {
    const seq = ++checkSeqRef.current;
    let next: AuthStatus | null = null;
    try {
      next = await fetchAuthStatus(base);
    } catch {
      // daemon injoignable, ou origine de la page refusee
    }
    if (seq !== checkSeqRef.current) return;
    const current = phaseRef.current;
    if (current === 'opening' || current === 'ready') return;
    if (!next) {
      go('unreachable');
      return;
    }
    setStatus(next);
    if (enrollRef.current) go('login');
    else if (!next.auth_required) go('ready');
    else if (!heldRef.current && readCredential()) go('opening');
    else go('login');
  }, [base, go]);

  // Premiere verification, nouveaux essais tant que le daemon est injoignable, et
  // rafraichissement discret de l'ecran de connexion (une passkey vient d'etre creee...).
  useEffect(() => {
    if (phase === 'opening' || phase === 'ready' || busy) return;
    const run = () => void check();
    if (phase === 'checking') {
      const first = window.setTimeout(run, 0);
      return () => window.clearTimeout(first);
    }
    const timer = window.setInterval(run, phase === 'unreachable' ? RETRY_MS : REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [phase, busy, check]);

  /** Presente une nouvelle preuve au daemon : l'application se monte et ouvre le WebSocket. */
  const open = useCallback(() => {
    heldRef.current = false;
    refusalsRef.current = 0;
    checkSeqRef.current++;
    setCredentialKind(readCredential()?.kind ?? null);
    setError(null);
    setNotice(null);
    setEpoch((n) => n + 1);
    go('opening');
  }, [go]);

  const ceremony = useCallback(
    async (step: 'enroll' | 'login') => {
      // Une verification en cours ne doit pas changer d'ecran pendant l'invite du navigateur.
      checkSeqRef.current++;
      setBusy(true);
      setError(null);
      try {
        const token =
          step === 'enroll' ? await enrollPasskey(base, enrollRef.current ?? '') : await loginWithPasskey(base);
        storeSession(token);
        if (step === 'enroll') {
          // Le code est consomme : on le retire de l'adresse.
          dropEnrollCodeFromUrl();
          enrollRef.current = null;
          setEnrollCode(null);
        }
        open();
      } catch (err) {
        setError(describeAuthError(err, step));
      } finally {
        setBusy(false);
      }
    },
    [base, open]
  );

  const enroll = useCallback(() => ceremony('enroll'), [ceremony]);
  const login = useCallback(() => ceremony('login'), [ceremony]);

  /** Jeton d'acces saisi a la main (repli, application mobile). */
  const submitToken = useCallback(
    (token: string) => {
      const trimmed = token.trim();
      if (!trimmed) return;
      storeAccessToken(trimmed);
      open();
    },
    [open]
  );

  /** Ouvre l'application sans creer la passkey du lien (possible tant qu'aucune authentification n'est exigee). */
  const skipEnroll = useCallback(() => {
    enrollRef.current = null;
    setEnrollCode(null);
    setError(null);
    go('checking');
  }, [go]);

  const retry = useCallback(() => {
    setError(null);
    go('checking');
  }, [go]);

  /** Efface la session de l'onglet et le jeton memorise, puis revient a l'ecran de connexion. */
  const logout = useCallback(() => {
    // La session est aussi fermee cote daemon : son jeton ne doit plus rien ouvrir.
    const held = readCredential();
    if (held?.kind === 'session') void closeSession(base, held.token);
    clearCredentials();
    heldRef.current = true;
    setCredentialKind(readCredential()?.kind ?? null);
    setError(null);
    setNotice('Vous êtes déconnecté.');
    go('checking');
  }, [go, base]);

  /** Le WebSocket est ouvert : la preuve est acceptee. */
  const onSocketOpen = useCallback(() => {
    refusalsRef.current = 0;
    if (phaseRef.current === 'opening') go('ready');
  }, [go]);

  /**
   * Le WebSocket s'est ferme sans s'ouvrir. Si le daemon repond et exige une
   * authentification, c'est un refus : retour a l'ecran de connexion, ce qui demonte
   * l'application et arrete ses tentatives de reconnexion.
   */
  const onSocketRefused = useCallback(async () => {
    const seq = ++checkSeqRef.current;
    let next: AuthStatus | null = null;
    try {
      next = await fetchAuthStatus(base);
    } catch {
      // daemon injoignable
    }
    if (seq !== checkSeqRef.current) return;
    const current = phaseRef.current;
    if (current !== 'opening' && current !== 'ready') return;
    if (!next) {
      // Application deja ouverte : elle affiche « Déconnecté » et retente seule.
      if (current === 'opening') go('unreachable');
      return;
    }
    setStatus(next);
    if (!next.auth_required) return;
    // Application ouverte : un second refus est attendu, pour ne pas confondre avec un
    // daemon qui redemarre entre la tentative et la verification.
    refusalsRef.current += 1;
    if (current === 'ready' && refusalsRef.current < 2) return;

    const refused = readCredential();
    heldRef.current = true;
    clearSession();
    setCredentialKind(readCredential()?.kind ?? null);
    setNotice(
      current === 'opening' && refused?.kind !== 'session'
        ? "Ce jeton d'accès n'est pas accepté par le daemon."
        : refused
        ? "Votre session n'est plus valable : elle a expiré ou le daemon a redémarré. Reconnectez-vous pour continuer."
        : 'Le daemon exige désormais une authentification.'
    );
    go('login');
  }, [base, go]);

  return {
    phase,
    status,
    daemonUrl: base,
    enrollCode,
    busy,
    error,
    notice,
    epoch,
    credentialKind,
    enroll,
    login,
    submitToken,
    skipEnroll,
    retry,
    logout,
    onSocketOpen,
    onSocketRefused,
  };
}

export type Auth = ReturnType<typeof useAuth>;
