import React, { useEffect, useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  TextInput,
  ScrollView,
  SafeAreaView,
  StatusBar,
  ActivityIndicator,
  Switch,
  PermissionsAndroid,
  Platform,
} from 'react-native';
import { Ionicons, MaterialCommunityIcons } from '@expo/vector-icons';
import { SoundWaveOrb } from '../components/SoundWaveOrb';
import { NestorCall } from '../native/NestorCall';
import { isStandbyEnabled, loadConnection, saveConnection, saveStandbyEnabled, splitTokenFromUrl } from '../storage/connection';
import { isSharingActive, isSharingEnabled, startSharing, stopSharing } from '../location/sharing';
import { alpha, colors, fonts, radius } from '../theme';
import { probeDaemon, type ReachResult } from '../native/reachability';

interface DialerScreenProps {
  onStartCall: (serverUrl: string, token?: string) => void;
  isConnecting: boolean;
  statusMessage?: string;
}

/** Ligne a interrupteur : titre, explication, etat. */
const SwitchRow: React.FC<{
  icon: React.ComponentProps<typeof MaterialCommunityIcons>['name'];
  title: string;
  hint: string;
  value: boolean;
  busy: boolean;
  note: string | null;
  onChange: (next: boolean) => void;
}> = ({ icon, title, hint, value, busy, note, onChange }) => (
  <View style={styles.switchBlock}>
    <View style={styles.switchRow}>
      <View style={[styles.switchIcon, value && styles.switchIconActive]}>
        <MaterialCommunityIcons name={icon} size={18} color={value ? colors.brass300 : colors.ivory500} />
      </View>
      <View style={styles.switchText}>
        <Text style={styles.switchTitle}>{title}</Text>
        <Text style={styles.switchHint}>{hint}</Text>
      </View>
      <Switch
        value={value}
        disabled={busy}
        onValueChange={onChange}
        trackColor={{ false: colors.ink700, true: alpha.brass(0.45) }}
        thumbColor={value ? colors.brass300 : colors.ivory500}
      />
    </View>
    {note && <Text style={styles.switchNote}>{note}</Text>}
  </View>
);

export const DialerScreen: React.FC<DialerScreenProps> = ({ onStartCall, isConnecting, statusMessage }) => {
  const [serverUrl, setServerUrl] = useState('ws://10.0.2.2:8340/ws');
  const [token, setToken] = useState('');
  const [showConnection, setShowConnection] = useState(false);

  // Canal hors appel : service de premier plan, relance au lancement si la preference est posee.
  const [standby, setStandby] = useState(false);
  const [standbyBusy, setStandbyBusy] = useState(false);
  const [standbyNote, setStandbyNote] = useState<string | null>(null);

  // Partage de position en arriere-plan : preference memorisee et etat reel du suivi.
  const [sharing, setSharing] = useState(false);
  const [sharingBusy, setSharingBusy] = useState(false);
  const [sharingNote, setSharingNote] = useState<string | null>(null);

  // Restaure la derniere connexion memorisee (URL + jeton d'onboarding) et les services.
  useEffect(() => {
    loadConnection().then((saved) => {
      if (saved.serverUrl) setServerUrl(saved.serverUrl);
      if (saved.token) setToken(saved.token);
      // Sans adresse memorisee, le reglage de connexion est ouvert d'emblee.
      if (!saved.serverUrl) setShowConnection(true);
    });
    Promise.all([isStandbyEnabled(), NestorCall.isStandbyRunning(), loadConnection()]).then(([enabled, running, saved]) => {
      setStandby(enabled && running);
      if (enabled && !running && saved.serverUrl) {
        NestorCall.startStandby(saved.serverUrl, saved.token).then((ok) => setStandby(ok));
      }
    });
    Promise.all([isSharingEnabled(), isSharingActive()]).then(([enabled, active]) => {
      setSharing(enabled && active);
      if (enabled && !active) setSharingNote("Le suivi s'est arrêté : réactivez-le.");
    });
  }, []);

  const toggleStandby = async (next: boolean) => {
    setStandbyBusy(true);
    setStandbyNote(null);
    try {
      if (next) {
        if (Platform.OS === 'android' && Platform.Version >= 33) {
          const granted = await PermissionsAndroid.request(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS);
          if (granted !== PermissionsAndroid.RESULTS.GRANTED) {
            setStandbyNote('Sans notifications, les alertes de Nestor ne seront pas visibles.');
          }
        }
        const connection = splitTokenFromUrl(serverUrl, token);
        await saveConnection(connection.url, connection.token);
        const ok = await NestorCall.startStandby(connection.url, connection.token);
        await saveStandbyEnabled(ok);
        setStandby(ok);
        if (!ok) setStandbyNote('Démarrage impossible.');
      } else {
        await NestorCall.stopStandby();
        await saveStandbyEnabled(false);
        setStandby(false);
      }
    } catch (err) {
      setStandby(false);
      setStandbyNote(err instanceof Error ? err.message : 'Activation impossible.');
    } finally {
      setStandbyBusy(false);
    }
  };

  const toggleSharing = async (next: boolean) => {
    setSharingBusy(true);
    setSharingNote(null);
    try {
      if (next) {
        // Le jeton doit etre memorise avant : la tache de fond le relit depuis le stockage.
        const connection = splitTokenFromUrl(serverUrl, token);
        await saveConnection(connection.url, connection.token);
        const result = await startSharing();
        setSharing(result.ok);
        if (!result.ok) setSharingNote(result.reason);
      } else {
        await stopSharing();
        setSharing(false);
      }
    } catch (err) {
      setSharing(false);
      setSharingNote(err instanceof Error ? err.message : 'Activation impossible.');
    } finally {
      setSharingBusy(false);
    }
  };

  // Par ou passe la connexion : sonde au chargement, a chaque changement d'adresse, et
  // avant un appel, pour dire « activez Tailscale » plutot que d'echouer en silence.
  const [reach, setReach] = useState<ReachResult | null>(null);
  useEffect(() => {
    let cancelled = false;
    const url = splitTokenFromUrl(serverUrl, token).url;
    const timer = setTimeout(() => {
      probeDaemon(url).then((result) => !cancelled && setReach(result));
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [serverUrl, token]);

  const handleStart = async () => {
    const connection = splitTokenFromUrl(serverUrl, token);
    setServerUrl(connection.url);
    setToken(connection.token);
    saveConnection(connection.url, connection.token);
    const result = await probeDaemon(connection.url);
    setReach(result);
    if (result.reach !== 'tailnet') return;
    onStartCall(connection.url, connection.token);
  };

  const presets = [
    { label: 'Émulateur', url: 'ws://10.0.2.2:8340/ws' },
    { label: 'Localhost', url: 'ws://127.0.0.1:8340/ws' },
    { label: 'Wi-Fi', url: 'ws://192.168.1.50:8340/ws' },
    { label: 'Tailscale', url: 'wss://nestor.vienne.me/ws' },
  ];

  let host = serverUrl;
  try {
    host = new URL(serverUrl).host;
  } catch {
    // adresse en cours de saisie
  }

  return (
    <SafeAreaView style={styles.safeArea}>
      <StatusBar barStyle="light-content" backgroundColor={colors.ink950} />
      <ScrollView style={styles.scrollView} contentContainerStyle={styles.container} keyboardShouldPersistTaps="handled">
        {/* En-tete : l'orbe, le nom, la devise */}
        <View style={styles.hero}>
          <SoundWaveOrb state="idle" rms={0.03} peak={0.05} size={120} />
          <Text style={styles.title}>Nestor</Text>
          <Text style={styles.subtitle}>À votre service, discrètement.</Text>
        </View>

        {/* Appel */}
        <TouchableOpacity
          style={[styles.callButton, isConnecting && styles.callButtonDisabled]}
          onPress={handleStart}
          disabled={isConnecting}
          activeOpacity={0.85}
          accessibilityRole="button"
          accessibilityLabel="Appeler Nestor"
        >
          {isConnecting ? (
            <ActivityIndicator size="small" color={colors.ink950} />
          ) : (
            <MaterialCommunityIcons name="phone" size={24} color={colors.ink950} />
          )}
          <Text style={styles.callButtonText}>{isConnecting ? statusMessage || 'Connexion…' : 'Appeler Nestor'}</Text>
        </TouchableOpacity>
        {reach && (
          <View style={styles.reachRow}>
            <View style={[styles.reachDot, reach.reach === 'tailnet' ? styles.reachDotOk : reach.reach === 'public' ? styles.reachDotWarn : styles.reachDotBad]} />
            <Text style={[styles.reachText, reach.reach === 'tailnet' ? styles.reachTextOk : styles.reachTextWarn]}>{reach.note}</Text>
          </View>
        )}

        {/* Services hors appel */}
        <View style={styles.card}>
          <Text style={styles.cardTitle}>Hors appel</Text>
          <SwitchRow
            icon="bell-ring-outline"
            title="Rester joignable"
            hint="Alertes en notification ; le téléphone sonne au réveil programmé."
            value={standby}
            busy={standbyBusy}
            note={standbyNote}
            onChange={toggleStandby}
          />
          <View style={styles.separator} />
          <SwitchRow
            icon="map-marker-outline"
            title="Partager ma position"
            hint="Tous les 200 m ou 5 minutes. Nestor n'en garde que le lieu reconnu."
            value={sharing}
            busy={sharingBusy}
            note={sharingNote}
            onChange={toggleSharing}
          />
        </View>

        {/* Connexion au daemon : repliee une fois reglee */}
        <View style={styles.card}>
          <TouchableOpacity style={styles.cardHeader} onPress={() => setShowConnection(!showConnection)} activeOpacity={0.7}>
            <View style={styles.cardHeaderText}>
              <Text style={styles.cardTitle}>Daemon</Text>
              {!showConnection && <Text style={styles.cardSubtitle}>{host}</Text>}
            </View>
            <Ionicons name={showConnection ? 'chevron-up' : 'chevron-down'} size={18} color={colors.ivory500} />
          </TouchableOpacity>

          {showConnection && (
            <>
              <Text style={styles.fieldLabel}>Adresse de nestord</Text>
              <View style={styles.inputWrapper}>
                <Ionicons name="server-outline" size={16} color={colors.ivory500} />
                <TextInput
                  style={styles.textInput}
                  value={serverUrl}
                  onChangeText={setServerUrl}
                  placeholder="ws://10.0.2.2:8340/ws"
                  placeholderTextColor={colors.ivory700}
                  autoCapitalize="none"
                  autoCorrect={false}
                />
              </View>
              <View style={styles.presetsRow}>
                {presets.map((p) => (
                  <TouchableOpacity
                    key={p.label}
                    style={[styles.presetChip, serverUrl === p.url && styles.presetChipActive]}
                    onPress={() => setServerUrl(p.url)}
                  >
                    <Text style={[styles.presetChipText, serverUrl === p.url && styles.presetChipTextActive]}>{p.label}</Text>
                  </TouchableOpacity>
                ))}
              </View>

              <Text style={[styles.fieldLabel, { marginTop: 12 }]}>Jeton d'accès</Text>
              <View style={styles.inputWrapper}>
                <Ionicons name="key-outline" size={16} color={colors.ivory500} />
                <TextInput
                  style={styles.textInput}
                  value={token}
                  onChangeText={setToken}
                  placeholder="Fourni par nestord onboard"
                  placeholderTextColor={colors.ivory700}
                  autoCapitalize="none"
                  autoCorrect={false}
                  secureTextEntry
                />
              </View>
              <Text style={styles.fieldHint}>Un lien complet collé dans l'adresse, jeton compris, est accepté.</Text>
            </>
          )}
        </View>
      </ScrollView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: colors.ink950,
  },
  scrollView: {
    flex: 1,
  },
  container: {
    paddingHorizontal: 20,
    paddingTop: 12,
    paddingBottom: 48,
    gap: 14,
  },
  hero: {
    alignItems: 'center',
    marginBottom: 4,
  },
  title: {
    fontFamily: fonts.display,
    fontSize: 36,
    color: colors.ivory50,
    marginTop: -8,
  },
  subtitle: {
    fontSize: 14,
    color: colors.ivory500,
    marginTop: 2,
  },
  callButton: {
    height: 58,
    borderRadius: 29,
    backgroundColor: colors.brass400,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
  },
  callButtonDisabled: {
    backgroundColor: colors.brass700,
  },
  callButtonText: {
    color: colors.ink950,
    fontSize: 18,
    fontWeight: '600',
  },
  reachRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingHorizontal: 6,
    marginTop: -4,
  },
  reachDot: {
    width: 8,
    height: 8,
    borderRadius: 4,
  },
  reachDotOk: { backgroundColor: colors.ok400 },
  reachDotWarn: { backgroundColor: colors.alert400 },
  reachDotBad: { backgroundColor: colors.danger400 },
  reachText: {
    flex: 1,
    fontSize: 12,
    lineHeight: 16,
  },
  reachTextOk: { color: colors.ivory500 },
  reachTextWarn: { color: colors.alert300 },
  card: {
    backgroundColor: alpha.ink(0.7),
    borderRadius: radius.card,
    borderWidth: 1,
    borderColor: colors.ink800,
    padding: 16,
    gap: 10,
  },
  cardHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    minHeight: 24,
  },
  cardHeaderText: {
    flex: 1,
    gap: 2,
  },
  cardTitle: {
    fontFamily: fonts.display,
    fontSize: 18,
    color: colors.ivory100,
  },
  cardSubtitle: {
    fontFamily: fonts.mono,
    fontSize: 12,
    color: colors.ivory500,
  },
  separator: {
    height: 1,
    backgroundColor: colors.ink800,
  },
  switchBlock: {
    gap: 6,
  },
  switchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    minHeight: 44,
  },
  switchIcon: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: colors.ink850,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.ink500,
  },
  switchIconActive: {
    borderColor: colors.brass500,
    backgroundColor: alpha.brass(0.15),
  },
  switchText: {
    flex: 1,
    gap: 2,
  },
  switchTitle: {
    fontSize: 15,
    fontWeight: '500',
    color: colors.ivory100,
  },
  switchHint: {
    fontSize: 12,
    color: colors.ivory500,
    lineHeight: 16,
  },
  switchNote: {
    fontSize: 12,
    color: colors.alert300,
    lineHeight: 16,
    marginLeft: 48,
  },
  fieldLabel: {
    fontSize: 13,
    color: colors.ivory300,
  },
  fieldHint: {
    fontSize: 12,
    color: colors.ivory700,
    lineHeight: 16,
  },
  inputWrapper: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    backgroundColor: colors.ink950,
    borderRadius: radius.field,
    borderWidth: 1,
    borderColor: colors.ink500,
    paddingHorizontal: 12,
  },
  textInput: {
    flex: 1,
    height: 44,
    color: colors.ivory100,
    fontSize: 14,
    fontFamily: fonts.mono,
  },
  presetsRow: {
    flexDirection: 'row',
    gap: 8,
    flexWrap: 'wrap',
  },
  presetChip: {
    height: 32,
    paddingHorizontal: 12,
    borderRadius: radius.pill,
    backgroundColor: colors.ink850,
    borderWidth: 1,
    borderColor: colors.ink500,
    justifyContent: 'center',
  },
  presetChipActive: {
    borderColor: colors.brass500,
    backgroundColor: alpha.brass(0.15),
  },
  presetChipText: {
    fontSize: 12,
    color: colors.ivory500,
  },
  presetChipTextActive: {
    color: colors.brass300,
    fontWeight: '600',
  },
});
