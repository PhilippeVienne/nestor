import React, { useState, useRef, useEffect } from 'react';
import {
  Animated,
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  TextInput,
  KeyboardAvoidingView,
  Platform,
  SafeAreaView,
  StatusBar,
} from 'react-native';
import { Ionicons, MaterialCommunityIcons } from '@expo/vector-icons';
import { SoundWaveOrb } from '../components/SoundWaveOrb';
import { useNestorCall, PendingRequest } from '../hooks/useNestorCall';
import { alpha, colors, fonts, radius } from '../theme';

interface CallScreenProps {
  call: ReturnType<typeof useNestorCall>;
  onEndCall: () => void;
}

/** Ce qui est demande, en une phrase. */
function requestTitle(request: PendingRequest): string {
  if (request.kind === 'tool') return `${request.server} veut écrire`;
  return request.source === 'mission' ? 'Mission à confirmer' : 'Demande à confirmer';
}

/** Une demande en attente : quoi, pourquoi, puis « Approuver » / « Refuser ». */
const PendingRequestCard: React.FC<{
  request: PendingRequest;
  onResolve: (request: PendingRequest, approve: boolean) => Promise<boolean>;
}> = ({ request, onResolve }) => {
  // Reponse envoyee : la carte ne disparait qu'a la confirmation du daemon.
  const [sent, setSent] = useState<'approve' | 'refuse' | null>(null);
  const [failed, setFailed] = useState(false);

  // Sans confirmation du daemon au bout de quelques secondes, les boutons
  // redeviennent actifs plutot que de rester bloques.
  useEffect(() => {
    if (sent === null) return;
    const timer = setTimeout(() => setSent(null), 5000);
    return () => clearTimeout(timer);
  }, [sent]);

  const answer = async (approve: boolean) => {
    setFailed(false);
    setSent(approve ? 'approve' : 'refuse');
    let ok = false;
    try {
      ok = await onResolve(request, approve);
    } catch {
      ok = false;
    }
    if (!ok) {
      setSent(null);
      setFailed(true);
    }
  };

  return (
    <View style={styles.pendingCard} accessibilityRole="alert">
      <View style={styles.pendingHeader}>
        <Ionicons name="alert-circle" size={16} color={colors.alert300} />
        <Text style={styles.pendingTitle}>{requestTitle(request)}</Text>
      </View>

      {request.kind === 'tool' ? (
        <>
          <Text style={styles.pendingLabel}>
            Serveur <Text style={styles.pendingMono}>{request.server}</Text> · outil{' '}
            <Text style={styles.pendingMono}>{request.tool}</Text>
          </Text>
          {request.arguments.trim().length > 0 && (
            <View style={styles.pendingArgsBox}>
              <Text style={styles.pendingArgs}>{request.arguments}</Text>
            </View>
          )}
          <Text style={styles.pendingReason}>
            Écriture par un connecteur externe : elle n'est exécutée qu'avec votre accord.
          </Text>
        </>
      ) : (
        <>
          <Text style={styles.pendingText}>{request.text}</Text>
          {!!request.rationale && <Text style={styles.pendingReason}>{request.rationale}</Text>}
          {(request.category || request.score !== undefined) && (
            <Text style={styles.pendingMeta}>
              {[request.category, request.score !== undefined ? `score ${request.score}` : null]
                .filter(Boolean)
                .join(' · ')}
            </Text>
          )}
        </>
      )}

      {failed && (
        <Text style={styles.pendingError}>Réponse non envoyée (connexion indisponible). Réessayez.</Text>
      )}

      <View style={styles.pendingActions}>
        <TouchableOpacity
          style={[styles.pendingButton, styles.pendingApprove, sent !== null && styles.pendingButtonDisabled]}
          onPress={() => answer(true)}
          disabled={sent !== null}
          activeOpacity={0.8}
          accessibilityRole="button"
          accessibilityLabel={`Approuver : ${requestTitle(request)}`}
        >
          <Text style={styles.pendingApproveText}>{sent === 'approve' ? 'Envoyé…' : 'Approuver'}</Text>
        </TouchableOpacity>
        <TouchableOpacity
          style={[styles.pendingButton, styles.pendingRefuse, sent !== null && styles.pendingButtonDisabled]}
          onPress={() => answer(false)}
          disabled={sent !== null}
          activeOpacity={0.8}
          accessibilityRole="button"
          accessibilityLabel={`Refuser : ${requestTitle(request)}`}
        >
          <Text style={styles.pendingRefuseText}>{sent === 'refuse' ? 'Envoyé…' : 'Refuser'}</Text>
        </TouchableOpacity>
      </View>
    </View>
  );
};

export const CallScreen: React.FC<CallScreenProps> = ({ call, onEndCall }) => {
  const [isKeyboardOpen, setIsKeyboardOpen] = useState(false);
  const [inputText, setInputText] = useState('');
  const scrollViewRef = useRef<ScrollView>(null);

  // Auto-scroll transcript to bottom
  useEffect(() => {
    scrollViewRef.current?.scrollToEnd({ animated: true });
  }, [call.messages]);

  // Signal bref a chaque interruption vocale detectee par le daemon : une pastille
  // qui apparait puis s'efface, sans deplacer le reste de l'ecran.
  const interruptOpacity = useRef(new Animated.Value(0)).current;
  const seenInterruptRef = useRef(call.interruptCount);
  useEffect(() => {
    if (call.interruptCount === seenInterruptRef.current) return;
    seenInterruptRef.current = call.interruptCount;
    interruptOpacity.stopAnimation();
    const pulse = Animated.sequence([
      Animated.timing(interruptOpacity, { toValue: 1, duration: 120, useNativeDriver: true }),
      Animated.delay(900),
      Animated.timing(interruptOpacity, { toValue: 0, duration: 400, useNativeDriver: true }),
    ]);
    pulse.start();
    return () => pulse.stop();
  }, [call.interruptCount, interruptOpacity]);

  const handleSendText = () => {
    if (!inputText.trim()) return;
    call.sendTextMessage(inputText.trim());
    setInputText('');
  };

  const getStatusText = () => {
    switch (call.nestorState) {
      case 'listening':
        return 'Nestor vous écoute...';
      case 'thinking':
        return 'Nestor réfléchit...';
      case 'speaking':
        return 'Nestor vous répond...';
      case 'idle':
      default:
        return 'En attente de votre voix...';
    }
  };

  const getStatusBadgeColor = () => {
    switch (call.nestorState) {
      case 'listening':
        return colors.listen600;
      case 'thinking':
        return colors.think600;
      case 'speaking':
        return colors.brass500;
      default:
        return colors.ink700;
    }
  };

  const getAudioRouteIcon = () => {
    switch (call.audioRoute) {
      case 'SPEAKER':
        return 'volume-high';
      case 'BLUETOOTH':
        return 'bluetooth';
      case 'WIRED_HEADSET':
        return 'headset';
      case 'EARPIECE':
      default:
        return 'cellphone-wireless';
    }
  };

  const getAudioRouteLabel = () => {
    switch (call.audioRoute) {
      case 'SPEAKER':
        return 'Haut-parleur';
      case 'BLUETOOTH':
        return 'Casque Bluetooth';
      case 'WIRED_HEADSET':
        return 'Casque filaire';
      case 'EARPIECE':
      default:
        return 'Écouteur interne';
    }
  };

  return (
    <SafeAreaView style={styles.safeArea}>
      <StatusBar barStyle="light-content" backgroundColor={colors.ink950} />
      <KeyboardAvoidingView
        style={styles.container}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        {/* Top In-Call Header */}
        <View style={styles.header}>
          <Text style={styles.callerName}>Nestor</Text>
          <Text style={styles.callTimer}>{call.callDuration}</Text>

          {/* Active Backend Indicator (Claude vs Mode Réduit AGY) */}
          <TouchableOpacity
            style={[
              styles.backendBadge,
              call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy'
                ? styles.backendBadgeAgy
                : styles.backendBadgeClaude,
            ]}
            onPress={() => {
              const isAgy =
                call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy';
              call.setBackend(isAgy ? 'claude' : 'agy');
            }}
            activeOpacity={0.7}
          >
            <Ionicons
              name={
                call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy'
                  ? 'flash'
                  : 'sparkles'
              }
              size={11}
              color={
                call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy'
                  ? colors.alert300
                  : colors.ivory300
              }
            />
            <Text
              style={[
                styles.backendBadgeText,
                call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy'
                  ? styles.backendTextAgy
                  : styles.backendTextClaude,
              ]}
            >
              {call.backendStatus.is_fallback || call.backendStatus.active_backend === 'agy'
                ? 'Mode réduit'
                : 'Claude'}
            </Text>
          </TouchableOpacity>

          {/* Audio Route Indicator */}
          <TouchableOpacity
            style={styles.audioRoutePill}
            onPress={call.toggleSpeaker}
            activeOpacity={0.7}
          >
            <MaterialCommunityIcons
              name={getAudioRouteIcon() as any}
              size={14}
              color={colors.ivory500}
            />
            <Text style={styles.audioRouteText}>{getAudioRouteLabel()}</Text>
          </TouchableOpacity>
        </View>

        {/* Central Visualizer (Orb) */}
        <View style={styles.orbSection}>
          <SoundWaveOrb
            state={call.nestorState}
            rms={call.audioLevels.rms}
            peak={call.audioLevels.peak}
            size={160}
          />
          <View
            style={[
              styles.statusPill,
              { backgroundColor: getStatusBadgeColor() },
            ]}
          >
            <Text style={styles.statusPillText}>{getStatusText()}</Text>
          </View>
          <Animated.View
            style={[styles.interruptPill, { opacity: interruptOpacity }]}
            pointerEvents="none"
            accessibilityLiveRegion="polite"
          >
            <MaterialCommunityIcons name="hand-back-right" size={12} color={colors.alert400} />
            <Text style={styles.interruptPillText}>Interruption détectée</Text>
          </Animated.View>
        </View>

        {/* Demandes en attente d'une reponse (juge, ecritures des connecteurs externes) */}
        {call.pendingRequests.length > 0 && (
          <View style={styles.pendingSection}>
            {call.pendingRequests.length > 1 && (
              <Text style={styles.pendingCount}>
                {call.pendingRequests.length} demandes attendent votre réponse
              </Text>
            )}
            <ScrollView contentContainerStyle={styles.pendingList} keyboardShouldPersistTaps="handled">
              {call.pendingRequests.map((request) => (
                <PendingRequestCard
                  key={`${request.kind}-${request.id}`}
                  request={request}
                  onResolve={call.resolveRequest}
                />
              ))}
            </ScrollView>
          </View>
        )}

        {/* Live Conversation Transcript */}
        <View style={styles.transcriptSection}>
          {call.activeTools.length > 0 && (
            <View style={styles.toolsBar}>
              {call.activeTools.map((t) => (
                <View key={t.id} style={styles.toolTag}>
                  <Ionicons name="terminal" size={11} color={colors.listen300} />
                  <Text style={styles.toolTagText}>{t.name}</Text>
                </View>
              ))}
            </View>
          )}

          <ScrollView
            ref={scrollViewRef}
            style={styles.messagesList}
            contentContainerStyle={styles.messagesContent}
          >
            {call.messages.length === 0 ? (
              <View style={styles.emptyMessages}>
                <Text style={styles.emptyMessagesText}>
                  Parlez naturellement à Nestor ou utilisez le clavier ci-dessous.
                </Text>
              </View>
            ) : (
              call.messages.map((msg) => (
                <View
                  key={msg.id}
                  style={[
                    styles.messageBubble,
                    msg.role === 'user' ? styles.userBubble : styles.assistantBubble,
                  ]}
                >
                  <Text style={styles.messageRole}>
                    {msg.role === 'user' ? 'Vous' : 'Nestor'} · {msg.time}
                  </Text>
                  <Text style={styles.messageText}>{msg.text}</Text>
                </View>
              ))
            )}
          </ScrollView>
        </View>

        {/* Slide-up Text Input when Keyboard is toggled */}
        {isKeyboardOpen && (
          <View style={styles.textInputBar}>
            <TextInput
              style={styles.textInput}
              placeholder="Écrire à Nestor..."
              placeholderTextColor={colors.ivory700}
              value={inputText}
              onChangeText={setInputText}
              onSubmitEditing={handleSendText}
              returnKeyType="send"
              autoFocus
            />
            <TouchableOpacity style={styles.sendButton} onPress={handleSendText}>
              <Ionicons name="arrow-up-circle" size={34} color={colors.brass400} />
            </TouchableOpacity>
          </View>
        )}

        {/* Bottom In-Call Controls */}
        <View style={styles.controlsBar}>
          {/* Mute Button */}
          <TouchableOpacity
            style={[styles.controlButton, call.isMuted && styles.controlButtonActive]}
            onPress={call.toggleMute}
            activeOpacity={0.8}
          >
            <Ionicons
              name={call.isMuted ? 'mic-off' : 'mic'}
              size={24}
              color={call.isMuted ? colors.danger300 : colors.ivory100}
            />
            <Text style={styles.controlLabel}>{call.isMuted ? 'Micro coupé' : 'Micro'}</Text>
          </TouchableOpacity>

          {/* Speaker Button */}
          <TouchableOpacity
            style={[
              styles.controlButton,
              call.isSpeakerOn && styles.controlButtonActiveCyan,
            ]}
            onPress={call.toggleSpeaker}
            activeOpacity={0.8}
          >
            <Ionicons
              name={call.isSpeakerOn ? 'volume-high' : 'volume-mute'}
              size={24}
              color={call.isSpeakerOn ? colors.brass300 : colors.ivory100}
            />
            <Text style={styles.controlLabel}>
              {call.isSpeakerOn ? 'HP externe' : 'Écouteur'}
            </Text>
          </TouchableOpacity>

          {/* Barge-In / Interrupt Button */}
          <TouchableOpacity
            style={styles.controlButton}
            onPress={call.bargeIn}
            activeOpacity={0.8}
          >
            <MaterialCommunityIcons name="hand-back-right" size={24} color={colors.alert300} />
            <Text style={styles.controlLabel}>Interrompre</Text>
          </TouchableOpacity>

          {/* Keyboard Toggle Button */}
          <TouchableOpacity
            style={[styles.controlButton, isKeyboardOpen && styles.controlButtonActive]}
            onPress={() => setIsKeyboardOpen(!isKeyboardOpen)}
            activeOpacity={0.8}
          >
            <Ionicons name="keypad" size={24} color={colors.ivory100} />
            <Text style={styles.controlLabel}>Clavier</Text>
          </TouchableOpacity>

          {/* End Call Button */}
          <TouchableOpacity
            style={styles.hangUpButton}
            onPress={() => {
              call.endCall();
              onEndCall();
            }}
            activeOpacity={0.8}
          >
            <MaterialCommunityIcons name="phone-hangup" size={30} color={colors.ivory50} />
          </TouchableOpacity>
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: colors.ink950,
  },
  container: {
    flex: 1,
    justifyContent: 'space-between',
  },
  header: {
    alignItems: 'center',
    paddingTop: 14,
    paddingBottom: 6,
    gap: 4,
  },
  callerName: {
    fontFamily: fonts.display,
    fontSize: 30,
    color: colors.ivory50,
  },
  callTimer: {
    fontSize: 15,
    color: colors.ivory500,
    fontVariant: ['tabular-nums'],
  },
  backendBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 5,
    marginTop: 4,
    height: 28,
    paddingHorizontal: 10,
    borderRadius: radius.pill,
    borderWidth: 1,
    backgroundColor: alpha.ink(0.6),
  },
  backendBadgeAgy: {
    borderColor: alpha.alert(0.6),
  },
  backendBadgeClaude: {
    borderColor: colors.ink700,
  },
  backendBadgeText: {
    fontSize: 12,
    fontWeight: '500',
  },
  backendTextAgy: {
    color: colors.alert300,
  },
  backendTextClaude: {
    color: colors.ivory300,
  },
  audioRoutePill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 28,
    paddingHorizontal: 10,
    borderRadius: radius.pill,
    borderWidth: 1,
    borderColor: colors.ink700,
    backgroundColor: alpha.ink(0.6),
  },
  audioRouteText: {
    fontSize: 12,
    color: colors.ivory300,
  },
  orbSection: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 8,
  },
  statusPill: {
    marginTop: 4,
    height: 30,
    paddingHorizontal: 14,
    borderRadius: radius.pill,
    justifyContent: 'center',
  },
  statusPillText: {
    color: colors.ivory50,
    fontSize: 13,
    fontWeight: '500',
  },
  interruptPill: {
    position: 'absolute',
    top: 4,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 5,
    backgroundColor: alpha.alert(0.15),
    borderColor: alpha.alert(0.5),
    borderWidth: 1,
    borderRadius: radius.pill,
    paddingHorizontal: 10,
    paddingVertical: 3,
  },
  interruptPillText: {
    color: colors.alert300,
    fontSize: 12,
    fontWeight: '500',
  },
  pendingSection: {
    marginHorizontal: 16,
    marginBottom: 10,
    maxHeight: '45%',
  },
  pendingCount: {
    color: colors.alert300,
    fontSize: 12,
    fontWeight: '600',
    marginBottom: 6,
  },
  pendingList: {
    gap: 8,
  },
  pendingCard: {
    backgroundColor: alpha.alert(0.1),
    borderColor: alpha.alert(0.5),
    borderWidth: 1,
    borderRadius: radius.card,
    padding: 12,
    gap: 6,
  },
  pendingHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
  },
  pendingTitle: {
    color: colors.alert300,
    fontSize: 15,
    fontWeight: '600',
    flexShrink: 1,
  },
  pendingText: {
    color: colors.ivory100,
    fontSize: 15,
    lineHeight: 20,
  },
  pendingLabel: {
    color: colors.ivory300,
    fontSize: 13,
    lineHeight: 18,
  },
  pendingMono: {
    color: colors.ivory100,
    fontWeight: '600',
    fontFamily: fonts.mono,
  },
  pendingArgsBox: {
    backgroundColor: colors.ink950,
    borderRadius: 8,
    padding: 8,
  },
  pendingArgs: {
    color: colors.ivory300,
    fontSize: 12,
    lineHeight: 16,
    fontFamily: fonts.mono,
  },
  pendingReason: {
    color: colors.ivory300,
    fontSize: 13,
    lineHeight: 18,
  },
  pendingMeta: {
    color: colors.ivory500,
    fontSize: 11,
  },
  pendingError: {
    color: colors.danger300,
    fontSize: 12,
  },
  pendingActions: {
    flexDirection: 'row',
    gap: 8,
    marginTop: 4,
  },
  pendingButton: {
    flex: 1,
    minHeight: 46,
    borderRadius: radius.field,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pendingButtonDisabled: {
    opacity: 0.5,
  },
  pendingApprove: {
    backgroundColor: alpha.brass(0.25),
    borderColor: colors.brass500,
  },
  pendingApproveText: {
    color: colors.brass200,
    fontSize: 15,
    fontWeight: '600',
  },
  pendingRefuse: {
    borderColor: colors.ink600,
  },
  pendingRefuseText: {
    color: colors.ivory100,
    fontSize: 15,
    fontWeight: '600',
  },
  transcriptSection: {
    flex: 1,
    marginHorizontal: 16,
    marginBottom: 12,
    backgroundColor: alpha.ink(0.6),
    borderRadius: radius.card,
    borderWidth: 1,
    borderColor: colors.ink800,
    overflow: 'hidden',
  },
  toolsBar: {
    flexDirection: 'row',
    gap: 6,
    padding: 8,
    borderBottomWidth: 1,
    borderBottomColor: colors.ink800,
  },
  toolTag: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    backgroundColor: alpha.listen(0.12),
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 8,
  },
  toolTagText: {
    color: colors.listen300,
    fontSize: 11,
    fontFamily: fonts.mono,
  },
  messagesList: {
    flex: 1,
  },
  messagesContent: {
    padding: 12,
    gap: 10,
  },
  emptyMessages: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 36,
  },
  emptyMessagesText: {
    color: colors.ivory500,
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
  },
  messageBubble: {
    borderRadius: 14,
    paddingHorizontal: 12,
    paddingVertical: 8,
    maxWidth: '86%',
    borderWidth: 1,
  },
  userBubble: {
    alignSelf: 'flex-end',
    backgroundColor: colors.ink800,
    borderColor: colors.ink700,
    borderTopRightRadius: 4,
  },
  assistantBubble: {
    alignSelf: 'flex-start',
    backgroundColor: colors.ink900,
    borderColor: colors.ink800,
    borderTopLeftRadius: 4,
  },
  messageRole: {
    fontSize: 11,
    color: colors.ivory500,
    marginBottom: 2,
  },
  messageText: {
    color: colors.ivory100,
    fontSize: 15,
    lineHeight: 21,
  },
  textInputBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingBottom: 8,
    gap: 8,
  },
  textInput: {
    flex: 1,
    height: 44,
    backgroundColor: colors.ink950,
    borderRadius: 22,
    paddingHorizontal: 16,
    color: colors.ivory100,
    fontSize: 15,
    borderWidth: 1,
    borderColor: colors.ink500,
  },
  sendButton: {
    padding: 4,
  },
  controlsBar: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingBottom: Platform.OS === 'android' ? 24 : 12,
    paddingTop: 8,
  },
  controlButton: {
    width: 62,
    height: 62,
    borderRadius: 31,
    backgroundColor: colors.ink850,
    borderWidth: 1,
    borderColor: colors.ink500,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
  },
  controlButtonActive: {
    backgroundColor: alpha.danger(0.18),
    borderColor: colors.danger400,
  },
  controlButtonActiveCyan: {
    backgroundColor: alpha.brass(0.18),
    borderColor: colors.brass500,
  },
  controlLabel: {
    color: colors.ivory500,
    fontSize: 9,
    fontWeight: '500',
  },
  hangUpButton: {
    width: 66,
    height: 66,
    borderRadius: 33,
    backgroundColor: colors.danger600,
    alignItems: 'center',
    justifyContent: 'center',
    elevation: 6,
  },
});
