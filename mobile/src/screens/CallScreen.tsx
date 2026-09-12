import React, { useState, useRef, useEffect } from 'react';
import {
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
import { useNestorCall } from '../hooks/useNestorCall';

interface CallScreenProps {
  call: ReturnType<typeof useNestorCall>;
  onEndCall: () => void;
}

export const CallScreen: React.FC<CallScreenProps> = ({ call, onEndCall }) => {
  const [isKeyboardOpen, setIsKeyboardOpen] = useState(false);
  const [inputText, setInputText] = useState('');
  const scrollViewRef = useRef<ScrollView>(null);

  // Auto-scroll transcript to bottom
  useEffect(() => {
    scrollViewRef.current?.scrollToEnd({ animated: true });
  }, [call.messages]);

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
        return '#0284c7';
      case 'thinking':
        return '#7c3aed';
      case 'speaking':
        return '#059669';
      default:
        return '#334155';
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
      <StatusBar barStyle="light-content" backgroundColor="#070b14" />
      <KeyboardAvoidingView
        style={styles.container}
        behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      >
        {/* Top In-Call Header */}
        <View style={styles.header}>
          <View style={styles.telecomBadge}>
            <Ionicons name="shield-checkmark" size={13} color="#38bdf8" />
            <Text style={styles.telecomBadgeText}>Appel Système Android • Telecom</Text>
          </View>
          <Text style={styles.callerName}>Nestor</Text>
          <Text style={styles.callTimer}>{call.callDuration}</Text>

          {/* Audio Route Indicator */}
          <TouchableOpacity
            style={styles.audioRoutePill}
            onPress={call.toggleSpeaker}
            activeOpacity={0.7}
          >
            <MaterialCommunityIcons
              name={getAudioRouteIcon() as any}
              size={14}
              color="#94a3b8"
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
        </View>

        {/* Live Conversation Transcript */}
        <View style={styles.transcriptSection}>
          {call.activeTools.length > 0 && (
            <View style={styles.toolsBar}>
              {call.activeTools.map((t) => (
                <View key={t.id} style={styles.toolTag}>
                  <Ionicons name="terminal" size={11} color="#38bdf8" />
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
                    {msg.role === 'user' ? 'Vous' : 'Nestor'} • {msg.time}
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
              placeholderTextColor="#64748b"
              value={inputText}
              onChangeText={setInputText}
              onSubmitEditing={handleSendText}
              returnKeyType="send"
              autoFocus
            />
            <TouchableOpacity style={styles.sendButton} onPress={handleSendText}>
              <Ionicons name="arrow-up-circle" size={32} color="#38bdf8" />
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
              color={call.isMuted ? '#f43f5e' : '#f8fafc'}
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
              color={call.isSpeakerOn ? '#38bdf8' : '#f8fafc'}
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
            <MaterialCommunityIcons name="hand-back-right" size={24} color="#f59e0b" />
            <Text style={styles.controlLabel}>Interrompre</Text>
          </TouchableOpacity>

          {/* Keyboard Toggle Button */}
          <TouchableOpacity
            style={[styles.controlButton, isKeyboardOpen && styles.controlButtonActive]}
            onPress={() => setIsKeyboardOpen(!isKeyboardOpen)}
            activeOpacity={0.8}
          >
            <Ionicons name="keypad" size={24} color="#f8fafc" />
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
            <MaterialCommunityIcons name="phone-hangup" size={30} color="#ffffff" />
          </TouchableOpacity>
        </View>
      </KeyboardAvoidingView>
    </SafeAreaView>
  );
};

const styles = StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: '#070b14',
  },
  container: {
    flex: 1,
    justifyContent: 'space-between',
  },
  header: {
    alignItems: 'center',
    paddingTop: 16,
    paddingBottom: 8,
  },
  telecomBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: 'rgba(56, 189, 248, 0.12)',
    borderColor: 'rgba(56, 189, 248, 0.3)',
    borderWidth: 1,
    borderRadius: 20,
    paddingHorizontal: 10,
    paddingVertical: 4,
    gap: 6,
    marginBottom: 8,
  },
  telecomBadgeText: {
    color: '#38bdf8',
    fontSize: 11,
    fontWeight: '600',
    letterSpacing: 0.3,
  },
  callerName: {
    fontSize: 26,
    fontWeight: '700',
    color: '#f8fafc',
    letterSpacing: 0.5,
  },
  callTimer: {
    fontSize: 15,
    color: '#94a3b8',
    marginTop: 2,
    fontVariant: ['tabular-nums'],
  },
  audioRoutePill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    marginTop: 6,
    backgroundColor: '#111827',
    paddingHorizontal: 10,
    paddingVertical: 3,
    borderRadius: 12,
  },
  audioRouteText: {
    fontSize: 11,
    color: '#94a3b8',
  },
  orbSection: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 12,
  },
  statusPill: {
    marginTop: 8,
    paddingHorizontal: 14,
    paddingVertical: 5,
    borderRadius: 16,
  },
  statusPillText: {
    color: '#ffffff',
    fontSize: 12,
    fontWeight: '600',
  },
  transcriptSection: {
    flex: 1,
    marginHorizontal: 16,
    marginBottom: 12,
    backgroundColor: 'rgba(15, 23, 42, 0.65)',
    borderRadius: 16,
    borderWidth: 1,
    borderColor: '#1e293b',
    overflow: 'hidden',
  },
  toolsBar: {
    flexDirection: 'row',
    gap: 6,
    padding: 8,
    borderBottomWidth: 1,
    borderBottomColor: '#1e293b',
  },
  toolTag: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
    backgroundColor: 'rgba(56, 189, 248, 0.15)',
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 8,
  },
  toolTagText: {
    color: '#38bdf8',
    fontSize: 11,
    fontFamily: Platform.OS === 'android' ? 'monospace' : 'Courier',
  },
  messagesList: {
    flex: 1,
  },
  messagesContent: {
    padding: 12,
    gap: 8,
  },
  emptyMessages: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 36,
  },
  emptyMessagesText: {
    color: '#64748b',
    fontSize: 13,
    textAlign: 'center',
    lineHeight: 18,
  },
  messageBubble: {
    borderRadius: 12,
    padding: 10,
    maxWidth: '85%',
  },
  userBubble: {
    alignSelf: 'flex-end',
    backgroundColor: '#0369a1',
  },
  assistantBubble: {
    alignSelf: 'flex-start',
    backgroundColor: '#1e293b',
    borderWidth: 1,
    borderColor: '#334155',
  },
  messageRole: {
    fontSize: 10,
    color: '#94a3b8',
    marginBottom: 2,
    textTransform: 'uppercase',
  },
  messageText: {
    color: '#f8fafc',
    fontSize: 14,
    lineHeight: 20,
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
    backgroundColor: '#1e293b',
    borderRadius: 22,
    paddingHorizontal: 16,
    color: '#f8fafc',
    fontSize: 14,
    borderWidth: 1,
    borderColor: '#334155',
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
    backgroundColor: '#1e293b',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 2,
  },
  controlButtonActive: {
    backgroundColor: 'rgba(244, 63, 94, 0.25)',
    borderWidth: 1,
    borderColor: '#f43f5e',
  },
  controlButtonActiveCyan: {
    backgroundColor: 'rgba(56, 189, 248, 0.25)',
    borderWidth: 1,
    borderColor: '#38bdf8',
  },
  controlLabel: {
    color: '#94a3b8',
    fontSize: 9,
    fontWeight: '500',
  },
  hangUpButton: {
    width: 66,
    height: 66,
    borderRadius: 33,
    backgroundColor: '#e11d48',
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#e11d48',
    shadowOffset: { width: 0, height: 4 },
    shadowOpacity: 0.5,
    shadowRadius: 10,
    elevation: 8,
  },
});
