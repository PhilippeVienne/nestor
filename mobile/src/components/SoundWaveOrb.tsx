import React, { useEffect, useRef, useState } from 'react';
import { AccessibilityInfo, View, StyleSheet, Animated, Easing } from 'react-native';
import { NestorState } from '../native/NestorCall';
import { alpha, colors } from '../theme';

interface SoundWaveOrbProps {
  state: NestorState;
  rms: number;
  peak: number;
  size?: number;
}

/** Couleurs de l'orbe par etat, les memes que l'orbe du web. */
function palette(state: NestorState) {
  switch (state) {
    case 'listening':
      return { primary: colors.listen400, glow: alpha.listen(0.4), outerGlow: alpha.listen(0.12) };
    case 'thinking':
      return { primary: colors.think400, glow: alpha.think(0.4), outerGlow: alpha.think(0.12) };
    case 'speaking':
      return { primary: colors.brass400, glow: alpha.brass(0.45), outerGlow: alpha.brass(0.14) };
    case 'idle':
    default:
      return { primary: colors.brass700, glow: alpha.brass(0.22), outerGlow: alpha.brass(0.06) };
  }
}

export const SoundWaveOrb: React.FC<SoundWaveOrbProps> = ({ state, rms, peak, size = 200 }) => {
  const pulseAnim = useRef(new Animated.Value(1)).current;
  const waveAnim1 = useRef(new Animated.Value(1)).current;
  const waveAnim2 = useRef(new Animated.Value(1)).current;

  // Mouvement reduit demande par le systeme : l'orbe reste immobile.
  const [reduceMotion, setReduceMotion] = useState(false);
  useEffect(() => {
    AccessibilityInfo.isReduceMotionEnabled().then(setReduceMotion).catch(() => {});
    const sub = AccessibilityInfo.addEventListener('reduceMotionChanged', setReduceMotion);
    return () => sub.remove();
  }, []);

  // Respiration lente, plus vive quand Nestor reflechit ; deux ondes qui s'eloignent.
  useEffect(() => {
    if (reduceMotion) {
      pulseAnim.setValue(1);
      waveAnim1.setValue(1);
      waveAnim2.setValue(1);
      return;
    }
    const breath = state === 'thinking' ? 900 : 2200;
    const pulseLoop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulseAnim, { toValue: 1.06, duration: breath, easing: Easing.inOut(Easing.ease), useNativeDriver: true }),
        Animated.timing(pulseAnim, { toValue: 0.97, duration: breath, easing: Easing.inOut(Easing.ease), useNativeDriver: true }),
      ])
    );
    const wave1Loop = Animated.loop(
      Animated.sequence([
        Animated.timing(waveAnim1, { toValue: 1.35, duration: 2400, easing: Easing.out(Easing.ease), useNativeDriver: true }),
        Animated.timing(waveAnim1, { toValue: 1, duration: 0, useNativeDriver: true }),
      ])
    );
    const wave2Loop = Animated.loop(
      Animated.sequence([
        Animated.delay(1200),
        Animated.timing(waveAnim2, { toValue: 1.45, duration: 2600, easing: Easing.out(Easing.ease), useNativeDriver: true }),
        Animated.timing(waveAnim2, { toValue: 1, duration: 0, useNativeDriver: true }),
      ])
    );
    pulseLoop.start();
    wave1Loop.start();
    wave2Loop.start();
    return () => {
      pulseLoop.stop();
      wave1Loop.stop();
      wave2Loop.stop();
    };
  }, [state, reduceMotion, pulseAnim, waveAnim1, waveAnim2]);

  const tone = palette(state);
  const audioScale = Math.min(1.35, 1 + rms * 1.4 + peak * 0.25);
  const ripples = state !== 'idle' && !reduceMotion;

  return (
    <View style={[styles.container, { width: size * 1.5, height: size * 1.5 }]}>
      {ripples && (
        <Animated.View
          style={[
            styles.wave,
            {
              width: size * 1.3,
              height: size * 1.3,
              borderRadius: (size * 1.3) / 2,
              borderColor: tone.primary,
              opacity: waveAnim2.interpolate({ inputRange: [1, 1.45], outputRange: [0.3, 0] }),
              transform: [{ scale: waveAnim2 }],
            },
          ]}
        />
      )}
      {ripples && (
        <Animated.View
          style={[
            styles.wave,
            {
              width: size * 1.15,
              height: size * 1.15,
              borderRadius: (size * 1.15) / 2,
              borderColor: tone.primary,
              opacity: waveAnim1.interpolate({ inputRange: [1, 1.35], outputRange: [0.45, 0] }),
              transform: [{ scale: waveAnim1 }],
            },
          ]}
        />
      )}

      {/* Halo */}
      <Animated.View
        style={[
          styles.halo,
          {
            width: size,
            height: size,
            borderRadius: size / 2,
            backgroundColor: tone.outerGlow,
            transform: [{ scale: pulseAnim }, { scale: audioScale }],
          },
        ]}
      />

      {/* Coeur */}
      <Animated.View
        style={[
          styles.orbCore,
          {
            width: size * 0.7,
            height: size * 0.7,
            borderRadius: (size * 0.7) / 2,
            backgroundColor: tone.primary,
            shadowColor: tone.primary,
            transform: [{ scale: pulseAnim }, { scale: audioScale }],
          },
        ]}
      >
        <View
          style={[
            styles.innerGlow,
            {
              width: size * 0.42,
              height: size * 0.42,
              borderRadius: (size * 0.42) / 2,
              transform: [{ translateX: -size * 0.06 }, { translateY: -size * 0.06 }],
            },
          ]}
        />
      </Animated.View>
    </View>
  );
};

const styles = StyleSheet.create({
  container: {
    justifyContent: 'center',
    alignItems: 'center',
    position: 'relative',
  },
  wave: {
    position: 'absolute',
    borderWidth: 1,
  },
  halo: {
    position: 'absolute',
  },
  orbCore: {
    justifyContent: 'center',
    alignItems: 'center',
    shadowOffset: { width: 0, height: 0 },
    shadowOpacity: 0.8,
    shadowRadius: 24,
    elevation: 16,
  },
  innerGlow: {
    backgroundColor: colors.ivory50,
    opacity: 0.28,
  },
});
