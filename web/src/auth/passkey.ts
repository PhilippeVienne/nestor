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
  /** Domaine auquel les passkeys de cette page sont liees. */
  rp_id?: string;
  detail?: string;
}

/** Reponse d'erreur du daemon (le message est en francais sans accents, pour les journaux). */
export class DaemonError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = 'DaemonError';
    this.status = status;
  }
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
    throw new DaemonError(response.status, (data as { error?: string }).error ?? `erreur ${response.status}`);
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
  if (!credential) throw new DOMException('création de la passkey annulée', 'NotAllowedError');

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
  if (!credential) throw new DOMException('connexion par passkey annulée', 'NotAllowedError');

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

/** Traduit l'echec d'une ceremonie en une phrase lisible, sans le texte brut du navigateur. */
export function describeAuthError(error: unknown, step: 'enroll' | 'login'): string {
  if (error instanceof DaemonError) {
    if (error.status === 503) return "Ce daemon ne propose pas l'authentification par passkey.";
    if (step === 'enroll') {
      if (error.status === 403 && error.message.includes('lien')) {
        return "Ce lien d'enrôlement n'est plus valable : il a déjà servi ou il a expiré. Relancez la commande pour en obtenir un nouveau.";
      }
      if (error.status === 403) return "Le daemon a refusé cette passkey. Recommencez l'enregistrement.";
    } else {
      if (error.status === 404) return "Aucune passkey n'est enregistrée pour cette adresse.";
      if (error.status === 403) return "Le daemon n'a pas reconnu cette passkey.";
    }
    if (error.status === 400) return 'La demande a expiré avant sa validation. Recommencez.';
    return `Le daemon a répondu par une erreur (code ${error.status}). Recommencez.`;
  }
  if (error instanceof DOMException) {
    switch (error.name) {
      case 'NotAllowedError':
      case 'AbortError':
        return step === 'enroll'
          ? "L'enregistrement a été annulé ou a expiré. Aucune passkey n'a été créée."
          : "La connexion a été annulée ou a expiré. Vous pouvez réessayer.";
      case 'InvalidStateError':
        return 'Cet appareil possède déjà une passkey pour Nestor. Connectez-vous avec elle.';
      case 'SecurityError':
        return 'Le navigateur refuse une passkey pour cette adresse (domaine ou connexion non sécurisée).';
      case 'NotSupportedError':
        return "Cet appareil ne propose pas d'authentificateur compatible.";
    }
  }
  if (error instanceof TypeError) {
    return 'Le daemon ne répond pas. Vérifiez que nestord est lancé, puis réessayez.';
  }
  return step === 'enroll' ? "L'enregistrement de la passkey a échoué." : 'La connexion par passkey a échoué.';
}

/** Ferme la session cote daemon : son jeton cesse d'etre accepte. Sans effet si le daemon est injoignable. */
export async function closeSession(base: string, token: string): Promise<void> {
  try {
    await fetch(`${base}/auth/logout`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ token }),
    });
  } catch {
    // daemon injoignable : la session expirera d'elle-meme
  }
}
