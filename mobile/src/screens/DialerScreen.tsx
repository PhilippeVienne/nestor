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
} from 'react-native';
import { Ionicons, MaterialCommunityIcons } from '@expo/vector-icons';
import { SoundWaveOrb } from '../components/SoundWaveOrb';
import { PermissionsAndroid, Platform } from 'react-native';
import { NestorCall } from '../native/NestorCall';
import { isStandbyEnabled, loadConnection, saveConnection, saveStandbyEnabled, splitTokenFromUrl } from '../storage/connection';
import { isSharingActive, isSharingEnabled, startSharing, stopSharing } from '../location/sharing';

interface DialerScreenProps {
  onStartCall: (serverUrl: string, token?: string) => void;
  isConnecting: boolean;
  statusMessage?: string;
}

export const DialerScreen: React.FC<DialerScreenProps> = ({
  onStartCall,
  isConnecting,
  statusMessage,
}) => {
  const [serverUrl, setServerUrl] = useState('ws://10.0.2.2:8340/ws');
  const [token, setToken] = useState('');

  // Partage de position en arriere-plan : preference memorisee et etat reel du suivi.
  const [sharing, setSharing] = useState(false);
  const [sharingBusy, setSharingBusy] = useState(false);
  const [sharingNote, setSharingNote] = useState<string | null>(null);

  // Restaure la derniere connexion memorisee (URL + jeton d'onboarding).
  useEffect(() => {
    loadConnection().then((saved) => {
      if (saved.serverUrl) setServerUrl(saved.serverUrl);
      if (saved.token) setToken(saved.token);
    });
    Promise.all([isSharingEnabled(), isSharingActive()]).then(([enabled, active]) => {
      setSharing(enabled && active);
      if (enabled && !active) setSharingNote("Le suivi s'est arrêté : réactivez-le.");
    });
  }, []);

  // Canal hors appel : service de premier plan, relance au lancement si la preference est posee.
  const [standby, setStandby] = useState(false);
  const [standbyBusy, setStandbyBusy] = useState(false);
  const [standbyNote, setStandbyNote] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([isStandbyEnabled(), NestorCall.isStandbyRunning(), loadConnection()]).then(([enabled, running, saved]) => {
      setStandby(enabled && running);
      if (enabled && !running && saved.serverUrl) {
        NestorCall.startStandby(saved.serverUrl, saved.token).then((ok) => setStandby(ok));
      }
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

  const handleStart = () => {
    const connection = splitTokenFromUrl(serverUrl, token);
    setServerUrl(connection.url);
    setToken(connection.token);
    saveConnection(connection.url, connection.token);
    onStartCall(connection.url, connection.token);
  };

  const presets = [
    { label: 'Émulateur (10.0.2.2)', url: 'ws://10.0.2.2:8340/ws' },
    { label: 'Localhost', url: 'ws://127.0.0.1:8340/ws' },
    { label: 'Wi-Fi LAN', url: 'ws://192.168.1.50:8340/ws' },
    { label: 'Tailscale', url: 'wss://kanto.felis-ionian.ts.net:8443/ws' },
  ];

  return (
    <SafeAreaView style={styles.safeArea}>
      <StatusBar barStyle="light-content" backgroundColor="#070b14" />
      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={styles.container}
        keyboardShouldPersistTaps="handled"
      >
        {/* Nestor Branding Hero */}
        <View style={styles.heroSection}>
          <SoundWaveOrb state="idle" rms={0.05} peak={0.1} size={130} />
          <Text style={styles.title}>NESTOR</Text>
          <Text style={styles.subtitle}>Assistant Vocal Local pour Claude Code</Text>
          <View style={styles.telecomPill}>
            <Ionicons name="phone-portrait-outline" size={13} color="#38bdf8" />
            <Text style={styles.telecomPillText}>API Telecom Android • Mode Appel</Text>
          </View>
        </View>

        {/* Server Address Configuration Card */}
        <View style={styles.card}>
          <Text style={styles.cardLabel}>Adresse du daemon nestord :</Text>
          <View style={styles.inputWrapper}>
            <Ionicons name="server-outline" size={18} color="#64748b" style={styles.inputIcon} />
            <TextInput
              style={styles.textInput}
              value={serverUrl}
              onChangeText={setServerUrl}
              placeholder="ws://10.0.2.2:8340/ws"
              placeholderTextColor="#475569"
              autoCapitalize="none"
              autoCorrect={false}
            />
          </View>

          <Text style={[styles.cardLabel, { marginTop: 14 }]}>
            Jeton d'onboarding (commande `nestord onboard`) :
          </Text>
          <View style={styles.inputWrapper}>
            <Ionicons name="key-outline" size={18} color="#64748b" style={styles.inputIcon} />
            <TextInput
              style={styles.textInput}
              value={token}
              onChangeText={setToken}
              placeholder="Optionnel si l'URL contient ?token=..."
              placeholderTextColor="#475569"
              autoCapitalize="none"
              autoCorrect={false}
              secureTextEntry
            />
          </View>

          {/* Quick Presets */}
          <View style={styles.presetsRow}>
            {presets.map((p) => (
              <TouchableOpacity
                key={p.label}
                style={[
                  styles.presetChip,
                  serverUrl === p.url && styles.presetChipActive,
                ]}
                onPress={() => setServerUrl(p.url)}
              >
                <Text
                  style={[
                    styles.presetChipText,
                    serverUrl === p.url && styles.presetChipTextActive,
                  ]}
                >
                  {p.label}
                </Text>
              </TouchableOpacity>
            ))}
          </View>
        </View>

        {/* Canal hors appel */}
        <View style={styles.card}>
          <View style={styles.switchRow}>
            <View style={styles.switchTextContainer}>
              <Text style={styles.cardLabel}>Rester joignable hors appel</Text>
              <Text style={styles.featureDesc}>
                Connexion légère maintenue avec le daemon : alertes en notification, et le téléphone sonne au réveil programmé.
              </Text>
            </View>
            <Switch
              value={standby}
              disabled={standbyBusy}
              onValueChange={toggleStandby}
              trackColor={{ false: '#334155', true: 'rgba(52, 211, 153, 0.5)' }}
              thumbColor={standby ? '#34d399' : '#94a3b8'}
            />
          </View>
          {standbyNote && <Text style={styles.sharingNote}>{standbyNote}</Text>}
        </View>

        {/* Partage de position en arriere-plan */}
        <View style={styles.card}>
          <View style={styles.switchRow}>
            <View style={styles.switchTextContainer}>
              <Text style={styles.cardLabel}>Partager ma position avec Nestor</Text>
              <Text style={styles.featureDesc}>
                Hors appel, tous les 200 m ou 5 minutes. Nestor n'en garde que le lieu reconnu (domicile, bureau…).
              </Text>
            </View>
            <Switch
              value={sharing}
              disabled={sharingBusy}
              onValueChange={toggleSharing}
              trackColor={{ false: '#334155', true: 'rgba(56, 189, 248, 0.5)' }}
              thumbColor={sharing ? '#38bdf8' : '#94a3b8'}
            />
          </View>
          {sharingNote && <Text style={styles.sharingNote}>{sharingNote}</Text>}
        </View>

        {/* Feature Highlights */}
        <View style={styles.featuresContainer}>
          <View style={styles.featureRow}>
            <View style={styles.featureIconContainer}>
              <MaterialCommunityIcons name="phone-in-talk" size={20} color="#38bdf8" />
            </View>
            <View style={styles.featureTextContainer}>
              <Text style={styles.featureTitle}>Intégration Téléphonique Android</Text>
              <Text style={styles.featureDesc}>
                Géré comme un vrai appel téléphonique : notification système, contrôle écran verrouillé et capteur de proximité.
              </Text>
            </View>
          </View>

          <View style={styles.featureRow}>
            <View style={styles.featureIconContainer}>
              <MaterialCommunityIcons name="microphone" size={20} color="#34d399" />
            </View>
            <View style={styles.featureTextContainer}>
              <Text style={styles.featureTitle}>Full-Duplex & Élimination d'Écho (AEC)</Text>
              <Text style={styles.featureDesc}>
                Microphone 16 kHz mono avec annulation matérielle d'écho et bascule haut-parleur / écouteur / Bluetooth.
              </Text>
            </View>
          </View>

          <View style={styles.featureRow}>
            <View style={styles.featureIconContainer}>
              <MaterialCommunityIcons name="lightning-bolt" size={20} color="#a855f7" />
            </View>
            <View style={styles.featureTextContainer}>
              <Text style={styles.featureTitle}>Barge-In Instantané</Text>
              <Text style={styles.featureDesc}>
                Interrompez Nestor d'un mot ou d'un appui : le flux audio TTS est coupé immédiatement.
              </Text>
            </View>
          </View>

          <View style={styles.featureRow}>
            <View style={[styles.featureIconContainer, { backgroundColor: 'rgba(245, 158, 11, 0.15)' }]}>
              <MaterialCommunityIcons name="flash-outline" size={20} color="#fbbf24" />
            </View>
            <View style={styles.featureTextContainer}>
              <Text style={styles.featureTitle}>⚡ Mode Réduit de Secours (AGY)</Text>
              <Text style={styles.featureDesc}>
                En cas de quota de session Claude épuisé, Nestor bascule automatiquement sur Antigravity sans rupture de dialogue.
              </Text>
            </View>
          </View>
        </View>

        {/* Status indicator if connecting */}
        {isConnecting && (
          <View style={styles.statusBox}>
            <ActivityIndicator size="small" color="#38bdf8" />
            <Text style={styles.statusText}>
              {statusMessage || 'Établissement de la communication...'}
            </Text>
          </View>
        )}

        {/* Big Action Call Button */}
        <TouchableOpacity
          style={[styles.callButton, isConnecting && styles.callButtonDisabled]}
          onPress={handleStart}
          disabled={isConnecting}
          activeOpacity={0.8}
        >
          <View style={styles.callButtonInner}>
            <MaterialCommunityIcons name="phone" size={28} color="#ffffff" />
            <Text style={styles.callButtonText}>
              {isConnecting ? 'Connexion en cours...' : 'Appeler Nestor'}
            </Text>
          </View>
        </TouchableOpacity>
      </ScrollView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: '#070b14',
  },
  scrollView: {
    flex: 1,
  },
  container: {
    paddingHorizontal: 20,
    paddingTop: 20,
    paddingBottom: 64,
    alignItems: 'center',
  },
  heroSection: {
    alignItems: 'center',
    marginBottom: 28,
  },
  title: {
    fontSize: 32,
    fontWeight: '800',
    color: '#f8fafc',
    letterSpacing: 3,
    marginTop: 12,
  },
  subtitle: {
    fontSize: 14,
    color: '#94a3b8',
    marginTop: 4,
    textAlign: 'center',
  },
  telecomPill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    backgroundColor: 'rgba(56, 189, 248, 0.12)',
    borderColor: 'rgba(56, 189, 248, 0.3)',
    borderWidth: 1,
    borderRadius: 20,
    paddingHorizontal: 12,
    paddingVertical: 5,
    marginTop: 12,
  },
  telecomPillText: {
    color: '#38bdf8',
    fontSize: 12,
    fontWeight: '600',
  },
  card: {
    width: '100%',
    backgroundColor: 'rgba(15, 23, 42, 0.75)',
    borderRadius: 16,
    padding: 16,
    borderWidth: 1,
    borderColor: '#1e293b',
    marginBottom: 24,
  },
  cardLabel: {
    fontSize: 13,
    fontWeight: '600',
    color: '#cbd5e1',
    marginBottom: 8,
  },
  inputWrapper: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#090d16',
    borderRadius: 12,
    borderWidth: 1,
    borderColor: '#334155',
    paddingHorizontal: 12,
  },
  inputIcon: {
    marginRight: 8,
  },
  textInput: {
    flex: 1,
    height: 44,
    color: '#f8fafc',
    fontSize: 14,
    fontFamily: 'monospace',
  },
  presetsRow: {
    flexDirection: 'row',
    gap: 8,
    marginTop: 10,
    flexWrap: 'wrap',
  },
  presetChip: {
    paddingHorizontal: 10,
    paddingVertical: 5,
    borderRadius: 8,
    backgroundColor: '#1e293b',
    borderWidth: 1,
    borderColor: '#334155',
  },
  presetChipActive: {
    backgroundColor: 'rgba(56, 189, 248, 0.2)',
    borderColor: '#38bdf8',
  },
  presetChipText: {
    fontSize: 11,
    color: '#94a3b8',
  },
  presetChipTextActive: {
    color: '#38bdf8',
    fontWeight: '600',
  },
  switchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
  },
  switchTextContainer: {
    flex: 1,
  },
  sharingNote: {
    marginTop: 10,
    fontSize: 12,
    color: '#fbbf24',
    lineHeight: 16,
  },
  featuresContainer: {
    width: '100%',
    gap: 16,
    marginBottom: 24,
  },
  featureRow: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 12,
    backgroundColor: 'rgba(15, 23, 42, 0.4)',
    padding: 12,
    borderRadius: 12,
    borderWidth: 1,
    borderColor: '#1e293b',
  },
  featureIconContainer: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: '#0f172a',
    justifyContent: 'center',
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#334155',
  },
  featureTextContainer: {
    flex: 1,
  },
  featureTitle: {
    fontSize: 13,
    fontWeight: '700',
    color: '#f1f5f9',
    marginBottom: 2,
  },
  featureDesc: {
    fontSize: 11,
    color: '#94a3b8',
    lineHeight: 16,
  },
  statusBox: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    backgroundColor: 'rgba(56, 189, 248, 0.1)',
    borderColor: 'rgba(56, 189, 248, 0.3)',
    borderWidth: 1,
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 10,
    marginBottom: 16,
    width: '100%',
  },
  statusText: {
    color: '#38bdf8',
    fontSize: 13,
    fontWeight: '500',
  },
  callButton: {
    width: '100%',
    height: 58,
    borderRadius: 29,
    backgroundColor: '#059669',
    justifyContent: 'center',
    alignItems: 'center',
    shadowColor: '#059669',
    shadowOffset: { width: 0, height: 6 },
    shadowOpacity: 0.4,
    shadowRadius: 14,
    elevation: 8,
    marginTop: 8,
  },
  callButtonDisabled: {
    backgroundColor: '#334155',
    shadowOpacity: 0,
  },
  callButtonInner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  callButtonText: {
    color: '#ffffff',
    fontSize: 18,
    fontWeight: '700',
    letterSpacing: 0.5,
  },
});
