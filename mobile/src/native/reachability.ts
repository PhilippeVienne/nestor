/**
 * Par ou passe la connexion au daemon ? nestor.vienne.me repond partout, mais pas la
 * meme chose : sur le tailnet, l'API de nestord ; depuis Internet, la page publique du
 * Worker. Sonder /auth/status permet de le dire a l'utilisateur avant qu'un appel echoue.
 */
import { daemonHttpBase } from '../location/sharing';

export type Reach = 'tailnet' | 'public' | 'unreachable';

export interface ReachResult {
  reach: Reach;
  /** Phrase a afficher sous le bouton d'appel. */
  note: string;
}

export async function probeDaemon(wsUrl: string, timeoutMs = 4000): Promise<ReachResult> {
  let base: string;
  try {
    base = daemonHttpBase(wsUrl);
  } catch {
    return { reach: 'unreachable', note: 'Adresse du daemon invalide.' };
  }
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(`${base}/auth/status`, { signal: controller.signal, headers: { Accept: 'application/json' } });
    const text = await response.text();
    if (response.ok && text.trim().startsWith('{')) {
      return { reach: 'tailnet', note: 'Nestor est joignable.' };
    }
    if (response.ok && /<html/i.test(text)) {
      return { reach: 'public', note: "Vous êtes hors du tailnet : seule la page publique répond. Activez Tailscale." };
    }
    return { reach: 'unreachable', note: `Le daemon répond ${response.status} : vérifiez nestord et l'adresse.` };
  } catch (err) {
    const aborted = err instanceof Error && err.name === 'AbortError';
    return { reach: 'unreachable', note: aborted ? 'Pas de réponse du daemon. Tailscale est-il connecté ?' : 'Daemon injoignable. Tailscale est-il connecté ?' };
  } finally {
    clearTimeout(timer);
  }
}
