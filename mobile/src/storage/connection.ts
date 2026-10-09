import * as SecureStore from 'expo-secure-store';

const URL_KEY = 'nestor_server_url';
const TOKEN_KEY = 'nestor_auth_token';

export interface SavedConnection {
  serverUrl?: string;
  token?: string;
}

/** Relit l'URL et le jeton memorises (Keystore Android). Echec silencieux. */
export async function loadConnection(): Promise<SavedConnection> {
  try {
    const [serverUrl, token] = await Promise.all([
      SecureStore.getItemAsync(URL_KEY),
      SecureStore.getItemAsync(TOKEN_KEY),
    ]);
    return { serverUrl: serverUrl ?? undefined, token: token ?? undefined };
  } catch (err) {
    console.warn('[connection] lecture du stockage securise impossible', err);
    return {};
  }
}

/** Memorise l'URL et le jeton ; un jeton vide efface l'ancien. */
export async function saveConnection(serverUrl: string, token: string): Promise<void> {
  try {
    await SecureStore.setItemAsync(URL_KEY, serverUrl);
    if (token) {
      await SecureStore.setItemAsync(TOKEN_KEY, token);
    } else {
      await SecureStore.deleteItemAsync(TOKEN_KEY);
    }
  } catch (err) {
    console.warn('[connection] ecriture du stockage securise impossible', err);
  }
}

/**
 * Separe un `?token=` colle dans l'URL (sortie de `nestord onboard`) : le jeton
 * va dans le stockage securise et l'URL memorisee reste sans secret.
 */
export function splitTokenFromUrl(rawUrl: string, token: string): { url: string; token: string } {
  const url = rawUrl.trim();
  const match = url.match(/[?&]token=([^&]*)/);
  if (!match) return { url, token: token.trim() };
  let found = match[1];
  try {
    found = decodeURIComponent(found);
  } catch {
    // jeton laisse tel quel s'il n'est pas encode
  }
  const cleaned = url.replace(/([?&])token=[^&]*&?/, '$1').replace(/[?&]$/, '');
  return { url: cleaned, token: found || token.trim() };
}

const STANDBY_KEY = 'nestor_standby';

/** Preference « rester joignable hors appel ». */
export async function isStandbyEnabled(): Promise<boolean> {
  try {
    return (await SecureStore.getItemAsync(STANDBY_KEY)) === '1';
  } catch {
    return false;
  }
}

export async function saveStandbyEnabled(enabled: boolean): Promise<void> {
  try {
    if (enabled) await SecureStore.setItemAsync(STANDBY_KEY, '1');
    else await SecureStore.deleteItemAsync(STANDBY_KEY);
  } catch (err) {
    console.warn('[connection] preference de veille non enregistree', err);
  }
}
