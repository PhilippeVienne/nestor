/**
 * Adresse du daemon et preuves d'acces memorisees par le navigateur.
 *
 * Deux preuves sont acceptees par `/ws` : une session ouverte par passkey (elle
 * ne vaut que pour l'onglet) ou le jeton de `nestord onboard` (memorise dans ce
 * navigateur). `VITE_NESTOR_TOKEN` fournit un jeton au build, pour le developpement.
 */

const TOKEN_STORAGE_KEY = 'nestor_token';
const SESSION_STORAGE_KEY = 'nestor_session';

/** Adresse du WebSocket du daemon ; `VITE_NESTOR_WS_URL` remplace la valeur par defaut. */
export const DEFAULT_WS_URL =
  (import.meta.env.VITE_NESTOR_WS_URL as string | undefined) || 'ws://127.0.0.1:8340/ws';

export type CredentialKind = 'session' | 'token' | 'build';

function read(storage: () => Storage, key: string): string {
  try {
    return storage().getItem(key) ?? '';
  } catch {
    // stockage indisponible (navigation privee...)
    return '';
  }
}

function write(storage: () => Storage, key: string, value: string) {
  try {
    if (value) storage().setItem(key, value);
    else storage().removeItem(key);
  } catch {
    // stockage indisponible : la preuve ne vaudra pas au-dela de cette page
  }
}

const tab = () => window.sessionStorage;
const browser = () => window.localStorage;

// Copie en memoire : sert quand le stockage du navigateur est indisponible.
let memory: { kind: CredentialKind; token: string } | null = null;

/** Preuve presentee au daemon, par priorite : session de l'onglet, jeton saisi, jeton du build. */
export function readCredential(): { kind: CredentialKind; token: string } | null {
  const session = read(tab, SESSION_STORAGE_KEY);
  if (session) return { kind: 'session', token: session };
  const stored = read(browser, TOKEN_STORAGE_KEY);
  if (stored) return { kind: 'token', token: stored };
  if (memory) return memory;
  const build = (import.meta.env.VITE_NESTOR_TOKEN as string | undefined) ?? '';
  return build ? { kind: 'build', token: build } : null;
}

/** Session ouverte par passkey : memorisee pour l'onglet. */
export function storeSession(token: string) {
  memory = { kind: 'session', token };
  write(tab, SESSION_STORAGE_KEY, token);
}

/** Jeton d'acces saisi a la main : memorise dans ce navigateur, et prioritaire sur une ancienne session. */
export function storeAccessToken(token: string) {
  memory = { kind: 'token', token };
  write(tab, SESSION_STORAGE_KEY, '');
  write(browser, TOKEN_STORAGE_KEY, token);
}

/** Oublie la session de l'onglet (expiree, ou refusee par le daemon). */
export function clearSession() {
  if (memory?.kind === 'session') memory = null;
  write(tab, SESSION_STORAGE_KEY, '');
}

/** Deconnexion : oublie la session de l'onglet et le jeton memorise. */
export function clearCredentials() {
  memory = null;
  write(tab, SESSION_STORAGE_KEY, '');
  write(browser, TOKEN_STORAGE_KEY, '');
}

/** Ajoute la preuve d'acces a l'adresse du WebSocket (`?token=`). */
export function withCredential(url: string): string {
  const credential = readCredential();
  if (!credential) return url;
  const separator = url.includes('?') ? '&' : '?';
  return `${url}${separator}token=${encodeURIComponent(credential.token)}`;
}
