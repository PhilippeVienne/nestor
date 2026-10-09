import React, { useState } from 'react';
import { KeyRound, Loader2, ServerOff, ShieldAlert, type LucideIcon } from 'lucide-react';
import { passkeysSupported } from '../auth/passkey';
import type { Auth } from '../auth/useAuth';
import { TokenField } from './TokenField';

const FOCUS = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brass-300';
const PRIMARY = `btn btn-primary w-full min-h-12 text-center ${FOCUS}`;
const QUIET = `self-start min-h-11 inline-flex items-center rounded-lg text-[13px] text-ivory-500 underline-offset-4 hover:text-brass-300 hover:underline ${FOCUS}`;

/** Adresse par defaut de l'interface, celle que `nestord onboard --passkey` suppose sans `--ui`. */
const DEFAULT_UI_ORIGIN = 'http://localhost:5173';

const Code: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <code className="font-mono text-[13px] text-brass-300 [overflow-wrap:anywhere]">{children}</code>
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
      <pre className="m-0 rounded-lg border border-ink-800 bg-ink-950 p-3 font-mono text-[13px] text-brass-300 whitespace-pre-wrap [overflow-wrap:anywhere]">
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
          <p className="text-ivory-500 m-0">Un nouvel essai a lieu automatiquement toutes les 3 secondes.</p>
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
          <p className="text-ivory-500 m-0">
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
          <p className="text-ivory-500 m-0">
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
          <p className="text-ivory-500 m-0">
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
        <p className="text-ivory-500 m-0">
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
    <div className="fixed inset-0 z-50 overflow-y-auto bg-ink-950 text-ivory-100 font-sans">
      <div className="absolute inset-0 pointer-events-none opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(207,165,82,0.14),transparent_70%)]" />
      <main className="relative min-h-full flex items-center justify-center p-4">
        <div
          key={waiting ? 'wait' : 'card'}
          className={`w-full max-w-md card p-5 sm:p-7 gap-5 text-sm ${
            waiting ? 'nestor-appear-late' : ''
          }`}
        >
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-full overflow-hidden border border-brass-500/50 shrink-0 bg-ink-900">
              <img src="/nestor-logo.png" alt="" className="w-full h-full object-cover" />
            </div>
            <span className="font-display text-[22px] text-ivory-50">Nestor</span>
          </div>

          {auth.notice && !waiting && (
            <p role="status" className="m-0 rounded-lg border border-alert-600/50 bg-alert-600/10 px-3 py-2 text-alert-300">
              {auth.notice}
            </p>
          )}

          <div className="flex flex-col gap-3" aria-live="polite" aria-busy={waiting}>
            <h1 className="flex items-center gap-2.5 text-[22px] text-ivory-50 m-0">
              <Icon
                className={`w-6 h-6 shrink-0 ${view.tone === 'warn' ? 'text-alert-300' : 'text-brass-300'} ${
                  waiting ? 'animate-spin' : ''
                }`}
                aria-hidden="true"
              />
              {view.title}
            </h1>
            <div className="flex flex-col gap-2 text-[15px] leading-relaxed text-ivory-300 break-words [&_p]:m-0">{view.body}</div>
          </div>

          {auth.error && !waiting && (
            <p role="alert" className="m-0 rounded-lg border border-danger-600/50 bg-danger-600/10 px-3 py-2 text-danger-300">
              {auth.error}
            </p>
          )}

          {view.action && (
            <div key={view.key} className="flex flex-col gap-3">
              {view.action}
            </div>
          )}

          {(view.token || canSkipEnroll) && (
            <div className="flex flex-col border-t border-ink-800 pt-2">
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
