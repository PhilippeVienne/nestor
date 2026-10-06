/**
 * Connexion par passkey (WebAuthn) aupres de nestord.
 *
 * Le daemon fournit les options des ceremonies et verifie les reponses
 * (`nestord/src/passkey.rs`). La cle privee reste dans l'authentificateur :
 * ce module ne fait que transporter des octets, encodes en base64url.
 */

export interface AuthStatus {
  /** Passkeys enregistrees pour le domaine de cette page. */
  passkeys: number;
  /** Faux si la page est ouverte sur une adresse IP : une passkey exige un nom de domaine. */
  domain_ok: boolean;
  auth_required?: boolean;
  detail?: string;
}

/** Adresse HTTP du daemon, deduite de celle du WebSocket (`ws://hote:port/ws`). */
export function daemonHttpBase(wsUrl: string): string {
  const url = new URL(wsUrl);
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  url.pathname = '';
  url.search = '';
  return url.toString().replace(/\/$/, '');
}

function fromBase64Url(value: string): ArrayBuffer {
  const base64 = value.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

function toBase64Url(buffer: ArrayBuffer): string {
  let binary = '';
  for (const byte of new Uint8Array(buffer)) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

async function call<T>(base: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`${base}${path}`, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error((data as { error?: string }).error ?? `erreur ${response.status}`);
  }
  return data as T;
}

export function passkeysSupported(): boolean {
  return typeof window !== 'undefined' && 'PublicKeyCredential' in window && !!navigator.credentials;
}

export function fetchAuthStatus(base: string): Promise<AuthStatus> {
  return call<AuthStatus>(base, '/auth/status');
}

interface Ceremony<T> {
  id: string;
  options: { publicKey: T };
}

type CredentialDescriptorJson = { id: string; type: PublicKeyCredentialType; transports?: AuthenticatorTransport[] };

/** Cree une passkey avec le code d'enrolement du lien, et retourne un jeton de session. */
export async function enrollPasskey(base: string, code: string): Promise<string> {
  type CreationJson = Omit<PublicKeyCredentialCreationOptions, 'challenge' | 'user' | 'excludeCredentials'> & {
    challenge: string;
    user: { id: string; name: string; displayName: string };
    excludeCredentials?: CredentialDescriptorJson[];
  };
  const started = await call<Ceremony<CreationJson>>(base, '/auth/register/options', { code });
  const pk = started.options.publicKey;
  const credential = (await navigator.credentials.create({
    publicKey: {
      ...pk,
      challenge: fromBase64Url(pk.challenge),
      user: { ...pk.user, id: fromBase64Url(pk.user.id) },
      excludeCredentials: pk.excludeCredentials?.map((c) => ({ ...c, id: fromBase64Url(c.id) })),
    },
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error('création de la passkey annulée');

  const response = credential.response as AuthenticatorAttestationResponse;
  const finished = await call<{ token: string }>(base, '/auth/register/finish', {
    id: started.id,
    code,
    credential: {
      id: credential.id,
      rawId: toBase64Url(credential.rawId),
      type: credential.type,
      response: {
        attestationObject: toBase64Url(response.attestationObject),
        clientDataJSON: toBase64Url(response.clientDataJSON),
        transports: response.getTransports?.() ?? [],
      },
      extensions: credential.getClientExtensionResults(),
    },
  });
  return finished.token;
}

/** Prouve la detention de la passkey et retourne un jeton de session. */
export async function loginWithPasskey(base: string): Promise<string> {
  type RequestJson = Omit<PublicKeyCredentialRequestOptions, 'challenge' | 'allowCredentials'> & {
    challenge: string;
    allowCredentials?: CredentialDescriptorJson[];
  };
  const started = await call<Ceremony<RequestJson>>(base, '/auth/login/options', {});
  const pk = started.options.publicKey;
  const credential = (await navigator.credentials.get({
    publicKey: {
      ...pk,
      challenge: fromBase64Url(pk.challenge),
      allowCredentials: pk.allowCredentials?.map((c) => ({ ...c, id: fromBase64Url(c.id) })),
    },
  })) as PublicKeyCredential | null;
  if (!credential) throw new Error('connexion par passkey annulée');

  const response = credential.response as AuthenticatorAssertionResponse;
  const finished = await call<{ token: string }>(base, '/auth/login/finish', {
    id: started.id,
    credential: {
      id: credential.id,
      rawId: toBase64Url(credential.rawId),
      type: credential.type,
      response: {
        authenticatorData: toBase64Url(response.authenticatorData),
        clientDataJSON: toBase64Url(response.clientDataJSON),
        signature: toBase64Url(response.signature),
        userHandle: response.userHandle ? toBase64Url(response.userHandle) : null,
      },
      extensions: credential.getClientExtensionResults(),
    },
  });
  return finished.token;
}
