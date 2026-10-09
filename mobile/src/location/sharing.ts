/**
 * Partage de position en arriere-plan avec nestord (etape D de .agent/VISION.md).
 *
 * expo-location livre les positions a une tache expo-task-manager, definie
 * ici dans la portee globale du bundle (exigence de TaskManager.defineTask :
 * l'application peut etre lancee en arriere-plan sans aucune vue montee).
 * La tache envoie la derniere position en `POST /location` au daemon, avec le
 * jeton memorise. Sobriete : precision « Balanced » (~100 m), mise a jour tous
 * les 200 m ou toutes les 5 minutes, pas de GPS continu.
 */
import * as Location from 'expo-location';
import * as SecureStore from 'expo-secure-store';
import * as TaskManager from 'expo-task-manager';
import { loadConnection } from '../storage/connection';

export const LOCATION_TASK = 'nestor-location-sharing';
const SHARING_KEY = 'nestor_location_sharing';

/** Adresse HTTP du daemon, deduite de celle du WebSocket (`ws://hote:port/ws`). */
export function daemonHttpBase(wsUrl: string): string {
  const url = new URL(wsUrl.trim());
  url.protocol = url.protocol === 'wss:' ? 'https:' : 'http:';
  url.pathname = '';
  url.search = '';
  return url.toString().replace(/\/$/, '');
}

/** Envoie une position au daemon. Silencieux en cas d'echec : la suivante viendra. */
export async function postLocation(location: Location.LocationObject): Promise<void> {
  const { serverUrl, token } = await loadConnection();
  if (!serverUrl) return;
  try {
    await fetch(`${daemonHttpBase(serverUrl)}/location`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
      body: JSON.stringify({
        lat: location.coords.latitude,
        lon: location.coords.longitude,
        accuracy: location.coords.accuracy ?? undefined,
        timestamp: location.timestamp,
      }),
    });
  } catch (err) {
    console.debug('[location] envoi impossible', err);
  }
}

TaskManager.defineTask(LOCATION_TASK, async ({ data, error }) => {
  if (error) {
    console.warn('[location] tache en erreur', error.message);
    return;
  }
  const locations = (data as { locations?: Location.LocationObject[] } | undefined)?.locations ?? [];
  const last = locations[locations.length - 1];
  if (last) await postLocation(last);
});

/** Preference memorisee : l'utilisateur a active le partage. */
export async function isSharingEnabled(): Promise<boolean> {
  try {
    return (await SecureStore.getItemAsync(SHARING_KEY)) === '1';
  } catch {
    return false;
  }
}

/** Le suivi est-il effectivement en cours ? */
export async function isSharingActive(): Promise<boolean> {
  try {
    return await Location.hasStartedLocationUpdatesAsync(LOCATION_TASK);
  } catch {
    return false;
  }
}

export type SharingResult = { ok: true } | { ok: false; reason: string };

/**
 * Demande les permissions (premier plan puis arriere-plan, « Toujours autoriser »
 * sur Android) et lance le suivi. Echec lisible si une permission est refusee.
 */
export async function startSharing(): Promise<SharingResult> {
  const foreground = await Location.requestForegroundPermissionsAsync();
  if (foreground.status !== 'granted') {
    return { ok: false, reason: 'Permission de localisation refusée.' };
  }
  const background = await Location.requestBackgroundPermissionsAsync();
  if (background.status !== 'granted') {
    return {
      ok: false,
      reason: "Choisissez « Toujours autoriser » dans les réglages Android pour que Nestor suive votre position hors de l'application.",
    };
  }
  await Location.startLocationUpdatesAsync(LOCATION_TASK, {
    accuracy: Location.Accuracy.Balanced,
    distanceInterval: 200,
    timeInterval: 5 * 60 * 1000,
    deferredUpdatesInterval: 5 * 60 * 1000,
    deferredUpdatesDistance: 200,
    foregroundService: {
      notificationTitle: 'Nestor suit votre position',
      notificationBody: 'Pour savoir si vous êtes chez vous ou en route.',
      notificationColor: '#38bdf8',
      killServiceOnDestroy: false,
    },
  });
  await SecureStore.setItemAsync(SHARING_KEY, '1');
  // Une premiere position tout de suite, sans attendre le premier deplacement.
  const last = await Location.getLastKnownPositionAsync({});
  if (last) await postLocation(last);
  return { ok: true };
}

export async function stopSharing(): Promise<void> {
  try {
    if (await Location.hasStartedLocationUpdatesAsync(LOCATION_TASK)) {
      await Location.stopLocationUpdatesAsync(LOCATION_TASK);
    }
  } catch (err) {
    console.warn('[location] arret impossible', err);
  }
  try {
    await SecureStore.deleteItemAsync(SHARING_KEY);
  } catch {
    // preference deja absente
  }
}
