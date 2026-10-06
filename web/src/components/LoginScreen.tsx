import React, { useState } from 'react';
import { KeyRound, Loader2, ServerOff, ShieldAlert, type LucideIcon } from 'lucide-react';
import { passkeysSupported } from '../auth/passkey';
import type { Auth } from '../auth/useAuth';
import { TokenField } from './TokenField';

const FOCUS = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-300';
const PRIMARY = `w-full min-h-12 px-5 py-2 inline-flex items-center justify-center gap-2 rounded-lg border border-cyan-400 bg-cyan-900/60 text-cyan-50 font-semibold text-center hover:bg-cyan-800/70 disabled:opacity-60 transition-colors ${FOCUS}`;
const QUIET = `self-start min-h-11 inline-flex items-center rounded-lg text-[13px] text-slate-400 underline-offset-4 hover:text-cyan-200 hover:underline ${FOCUS}`;

/** Adresse par defaut de l'interface, celle que `nestord onboard --passkey` suppose sans `--ui`. */
const DEFAULT_UI_ORIGIN = 'http://localhost:5173';

const Code: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <code className="font-mono text-[13px] text-cyan-200 [overflow-wrap:anywhere]">{children}</code>
);

/** Commande a lancer en console, avec un bouton de copie quand le navigateur le permet. */
const Command: React.FC<{ command: string; primary: boolean }> = ({ command, primary }) => {
  const [copied, setCopied] = useState(false);
  const canCopy = typeof navigator !== 'undefined' && !!navigator.clipboard;
  const copy = () => {
    navigator.clipboard
      .writeText(command)
      .then(() => setCopied(true))
      .catch(() => setCopied(false));
  };
  return (
    <>
      <pre className="rounded-lg border border-slate-800 bg-black/40 p-3 font-mono text-[13px] text-cyan-200 whitespace-pre-wrap [overflow-wrap:anywhere]">
        {command}
      </pre>
      {canCopy && (
        <button type="button" autoFocus={primary} onClick={copy} className={PRIMARY}>
          {copied ? 'Commande copiée' : 'Copier la commande'}
        </button>
      )}
      <span className="sr-only" aria-live="polite">
        {copied ? 'Commande copiée dans le presse-papiers.' : ''}
      </span>
    </>
  );
};

interface View {
  key: string;
  icon: LucideIcon;
  tone?: 'warn';
  title: string;
  body: React.ReactNode;
  action?: React.ReactNode;
  /** Saisie d'un jeton : absente, en lien discret, ou depliee quand c'est la seule voie. */
  token?: 'link' | 'open';
}

function buildView(auth: Auth): View {
  const { phase, status, enrollCode, busy } = auth;

  if (phase === 'checking' || phase === 'opening' || phase === 'ready') {
    return {
      key: 'wait',
      icon: Loader2,
      title: phase === 'checking' ? "Vérification de l'accès" : 'Ouverture de la session',
      body: phase === 'checking' ? 'Nestor interroge le daemon…' : 'Le daemon vérifie votre accès…',
    };
  }

  if (phase === 'unreachable' || !status) {
    return {
      key: 'unreachable',
      icon: ServerOff,
      tone: 'warn',
      title: 'Daemon injoignable',
      body: (
        <>
          <p>
            Nestor ne répond pas à l'adresse <Code>{auth.daemonUrl}</Code>. Vérifiez que <Code>nestord</Code> est
            lancé
            {window.location.origin !== DEFAULT_UI_ORIGIN && (
              <>
                {' '}
                et que <Code>{window.location.origin}</Code> figure dans <Code>allowed_origins</Code>
              </>
            )}
            .
          </p>
          <p className="text-slate-400">Un nouvel essai a lieu automatiquement toutes les 3 secondes.</p>
        </>
      ),
      action: (
        <button type="button" autoFocus onClick={auth.retry} className={PRIMARY}>
          Réessayer
        </button>
      ),
    };
  }

  const { protocol, hostname, port, pathname, search, origin } = window.location;
  const title = enrollCode ? 'Enregistrer une passkey' : 'Se connecter avec votre passkey';

  // Page ouverte sur une adresse IP : une passkey exige un nom de domaine.
  if (!status.domain_ok) {
    const localhostUrl = `${protocol}//localhost${port ? `:${port}` : ''}${pathname}${search}`;
    const loopback = /^127\./.test(hostname) || hostname === '[::1]';
    return {
      key: 'domain',
      icon: ShieldAlert,
      tone: 'warn',
      title,
      body: (
        <>
          <p>
            Une passkey est liée à un nom de domaine : elle ne peut pas servir sur l'adresse IP{' '}
            <Code>{hostname}</Code>.
          </p>
          <p className="text-slate-400">
            {loopback
              ? 'La même page est disponible sur localhost.'
              : "Sur l'ordinateur qui héberge Nestor, ouvrez la page sur localhost. Depuis un autre appareil, il faut une adresse en HTTPS avec un nom de domaine, déclarée dans allowed_origins."}
          </p>
        </>
      ),
      action: (
        <a href={localhostUrl} className={PRIMARY}>
          Ouvrir sur localhost
        </a>
      ),
      token: status.auth_required ? 'link' : undefined,
    };
  }

  if (!passkeysSupported()) {
    return {
      key: 'unsupported',
      icon: ShieldAlert,
      tone: 'warn',
      title,
      body: (
        <>
          <p>Ce navigateur ne prend pas en charge les passkeys (WebAuthn).</p>
          <p className="text-slate-400">
            Ouvrez Nestor dans un navigateur récent
            {status.auth_required ? ", ou utilisez un jeton d'accès." : '.'}
          </p>
        </>
      ),
      token: status.auth_required ? 'open' : undefined,
    };
  }

  if (enrollCode) {
    return {
      key: 'enroll',
      icon: KeyRound,
      title,
      body: (
        <>
          <p>
            Ce lien autorise l'enregistrement d'une passkey pour <Code>{status.rp_id ?? hostname}</Code>, une seule
            fois.
          </p>
          <p className="text-slate-400">
            Votre navigateur vous demandera de la créer (empreinte, code de l'appareil ou clé de sécurité). Elle
            servira ensuite à ouvrir Nestor depuis ce navigateur.
          </p>
        </>
      ),
      action: (
        <button type="button" autoFocus disabled={busy} onClick={auth.enroll} className={PRIMARY}>
          {busy && <Loader2 className="w-4 h-4 animate-spin" aria-hidden="true" />}
          {busy ? 'En attente du navigateur…' : 'Créer la passkey'}
        </button>
      ),
      token: status.auth_required ? 'link' : undefined,
    };
  }

  if (status.passkeys > 0) {
    return {
      key: 'login',
      icon: KeyRound,
      title,
      body: (
        <p>
          Le daemon demande votre passkey pour ouvrir Nestor. Votre navigateur vous la demandera après le clic.
        </p>
      ),
      action: (
        <button type="button" autoFocus disabled={busy} onClick={auth.login} className={PRIMARY}>
          {busy && <Loader2 className="w-4 h-4 animate-spin" aria-hidden="true" />}
          {busy ? 'En attente du navigateur…' : 'Se connecter'}
        </button>
      ),
      token: 'link',
    };
  }

  // Authentification exigee, mais aucune passkey pour ce domaine.
  const command = `nestord onboard --passkey${origin === DEFAULT_UI_ORIGIN ? '' : ` --ui ${origin}`}`;
  const canCopy = typeof navigator !== 'undefined' && !!navigator.clipboard;
  return {
    key: 'no-passkey',
    icon: KeyRound,
    title: 'Aucune passkey pour cette adresse',
    body: (
      <>
        <p>
          Le daemon exige une authentification, mais aucune passkey n'est enregistrée pour <Code>{status.rp_id ?? hostname}</Code>.
        </p>
        <p className="text-slate-400">
          Pour en créer une, lancez cette commande sur l'ordinateur qui héberge Nestor, puis ouvrez dans ce navigateur
          le lien qu'elle affiche (valable 10 minutes, une seule fois)
          {origin === DEFAULT_UI_ORIGIN ? '.' : " ; l'option --ui indique l'adresse de cette page."}
        </p>
      </>
    ),
    action: <Command command={command} primary />,
    token: canCopy ? 'link' : 'open',
  };
}

/**
 * Ecran de connexion plein ecran, franchi avant d'entrer dans l'application :
 * un etat, un message, une action principale. Aucune invite WebAuthn ne part
 * sans un clic de l'utilisateur.
 */
export const LoginScreen: React.FC<{ auth: Auth }> = ({ auth }) => {
  const view = buildView(auth);
  const waiting = view.key === 'wait';
  const Icon = view.icon;
  const canSkipEnroll =
    !!auth.enrollCode && auth.phase === 'login' && !!auth.status && (!auth.status.auth_required || !!auth.credentialKind);

  return (
    <div className="fixed inset-0 z-50 overflow-y-auto bg-[#080a0f] text-slate-100 font-sans">
      <div className="absolute inset-0 pointer-events-none opacity-25 bg-[radial-gradient(ellipse_at_top,rgba(6,182,212,0.2),transparent_70%)]" />
      <main className="relative min-h-full flex items-center justify-center p-4">
        <div
          key={waiting ? 'wait' : 'card'}
          className={`w-full max-w-md rounded-2xl border border-slate-800 bg-[#0b111e] p-5 sm:p-7 flex flex-col gap-5 text-sm shadow-[0_0_40px_rgba(6,182,212,0.08)] ${
            waiting ? 'nestor-appear-late' : ''
          }`}
        >
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl overflow-hidden border border-cyan-500/30 shadow-[0_0_12px_rgba(6,182,212,0.3)] shrink-0 bg-[#030c28]">
              <img src="/nestor-logo.png" alt="" className="w-full h-full object-cover" />
            </div>
            <span className="text-sm font-semibold tracking-wider text-slate-100 font-mono">NESTOR</span>
          </div>

          {auth.notice && !waiting && (
            <p role="status" className="rounded-lg border border-amber-500/40 bg-amber-950/40 px-3 py-2 text-amber-100">
              {auth.notice}
            </p>
          )}

          <div className="flex flex-col gap-3" aria-live="polite" aria-busy={waiting}>
            <h1 className="flex items-center gap-2.5 text-xl font-semibold text-slate-50 m-0">
              <Icon
                className={`w-6 h-6 shrink-0 ${view.tone === 'warn' ? 'text-amber-300' : 'text-cyan-300'} ${
                  waiting ? 'animate-spin' : ''
                }`}
                aria-hidden="true"
              />
              {view.title}
            </h1>
            <div className="flex flex-col gap-2 text-[15px] leading-relaxed text-slate-200 break-words">{view.body}</div>
          </div>

          {auth.error && !waiting && (
            <p role="alert" className="rounded-lg border border-rose-500/40 bg-rose-950/40 px-3 py-2 text-rose-100">
              {auth.error}
            </p>
          )}

          {view.action && (
            <div key={view.key} className="flex flex-col gap-3">
              {view.action}
            </div>
          )}

          {(view.token || canSkipEnroll) && (
            <div className="flex flex-col border-t border-slate-800 pt-2">
              {view.token && (
                <TokenField
                  key={view.token}
                  id="login-token"
                  onSubmit={auth.submitToken}
                  defaultOpen={view.token === 'open'}
                  disabled={auth.busy}
                />
              )}
              {canSkipEnroll && (
                <button type="button" onClick={auth.skipEnroll} disabled={auth.busy} className={QUIET}>
                  Continuer sans enregistrer de passkey
                </button>
              )}
            </div>
          )}
        </div>
      </main>
    </div>
  );
};
