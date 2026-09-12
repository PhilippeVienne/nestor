import React, { useEffect, useRef } from 'react';
import { View, StyleSheet, Animated, Easing } from 'react-native';
import { NestorState } from '../native/NestorCall';

interface SoundWaveOrbProps {
  state: NestorState;
  rms: number;
  peak: number;
  size?: number;
}

export const SoundWaveOrb: React.FC<SoundWaveOrbProps> = ({
  state,
  rms,
  peak,
  size = 200,
}) => {
  const pulseAnim = useRef(new Animated.Value(1)).current;
  const rotateAnim = useRef(new Animated.Value(0)).current;
  const waveAnim1 = useRef(new Animated.Value(1)).current;
  const waveAnim2 = useRef(new Animated.Value(1)).current;

  // Continuous breathing / pulse animation
  useEffect(() => {
    const pulseLoop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulseAnim, {
          toValue: 1.08,
          duration: state === 'thinking' ? 1000 : 1800,
          easing: Easing.inOut(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(pulseAnim, {
          toValue: 0.96,
          duration: state === 'thinking' ? 1000 : 1800,
          easing: Easing.inOut(Easing.ease),
          useNativeDriver: true,
        }),
      ])
    );

    const rotateLoop = Animated.loop(
      Animated.timing(rotateAnim, {
        toValue: 1,
        duration: state === 'thinking' ? 4000 : 12000,
        easing: Easing.linear,
        useNativeDriver: true,
      })
    );

    const wave1Loop = Animated.loop(
      Animated.sequence([
        Animated.timing(waveAnim1, {
          toValue: 1.35,
          duration: 2000,
          easing: Easing.out(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(waveAnim1, {
          toValue: 1,
          duration: 0,
          useNativeDriver: true,
        }),
      ])
    );

    const wave2Loop = Animated.loop(
      Animated.sequence([
        Animated.delay(1000),
        Animated.timing(waveAnim2, {
          toValue: 1.45,
          duration: 2200,
          easing: Easing.out(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(waveAnim2, {
          toValue: 1,
          duration: 0,
          useNativeDriver: true,
        }),
      ])
    );

    pulseLoop.start();
    rotateLoop.start();
    wave1Loop.start();
    wave2Loop.start();

    return () => {
      pulseLoop.stop();
      rotateLoop.stop();
      wave1Loop.stop();
      wave2Loop.stop();
    };
  }, [state]);

  // Determine state colors
  const getColor = () => {
    switch (state) {
      case 'listening':
        return {
          primary: '#38bdf8', // Cyan
          secondary: '#0284c7',
          glow: 'rgba(56, 189, 248, 0.45)',
          outerGlow: 'rgba(56, 189, 248, 0.15)',
        };
      case 'thinking':
        return {
          primary: '#c084fc', // Purple
          secondary: '#7c3aed',
          glow: 'rgba(192, 132, 252, 0.45)',
          outerGlow: 'rgba(192, 132, 252, 0.15)',
        };
      case 'speaking':
        return {
          primary: '#34d399', // Emerald
          secondary: '#059669',
          glow: 'rgba(52, 211, 153, 0.45)',
          outerGlow: 'rgba(52, 211, 153, 0.15)',
        };
      case 'idle':
      default:
        return {
          primary: '#60a5fa', // Soft Blue
          secondary: '#1e3a8a',
          glow: 'rgba(96, 165, 250, 0.25)',
          outerGlow: 'rgba(96, 165, 250, 0.08)',
        };
    }
  };

  const colors = getColor();

  // Audio reactivity scale
  const audioScale = Math.min(1.4, 1 + rms * 1.5 + peak * 0.3);

  const spin = rotateAnim.interpolate({
    inputRange: [0, 1],
    outputRange: ['0deg', '360deg'],
  });

  return (
    <View style={[styles.container, { width: size * 1.5, height: size * 1.5 }]}>
      {/* Ripple wave 2 */}
      <Animated.View
        style={[
          styles.wave,
          {
            width: size * 1.3,
            height: size * 1.3,
            borderRadius: (size * 1.3) / 2,
            borderColor: colors.primary,
            opacity: waveAnim2.interpolate({
              inputRange: [1, 1.45],
              outputRange: [0.35, 0],
            }),
            transform: [{ scale: waveAnim2 }],
          },
        ]}
      />

      {/* Ripple wave 1 */}
      <Animated.View
        style={[
          styles.wave,
          {
            width: size * 1.15,
            height: size * 1.15,
            borderRadius: (size * 1.15) / 2,
            borderColor: colors.primary,
            opacity: waveAnim1.interpolate({
              inputRange: [1, 1.35],
              outputRange: [0.5, 0],
            }),
            transform: [{ scale: waveAnim1 }],
          },
        ]}
      />

      {/* Outer Halo */}
      <Animated.View
        style={[
          styles.halo,
          {
            width: size,
            height: size,
            borderRadius: size / 2,
            backgroundColor: colors.outerGlow,
            transform: [{ scale: pulseAnim }, { scale: audioScale }],
          },
        ]}
      />

      {/* Core Glowing Orb */}
      <Animated.View
        style={[
          styles.orbCore,
          {
            width: size * 0.72,
            height: size * 0.72,
            borderRadius: (size * 0.72) / 2,
            backgroundColor: colors.primary,
            shadowColor: colors.primary,
            transform: [{ scale: pulseAnim }, { scale: audioScale }, { rotate: spin }],
          },
        ]}
      >
        {/* Inner highlights */}
        <View
          style={[
            styles.innerGlow,
            {
              width: size * 0.45,
              height: size * 0.45,
              borderRadius: (size * 0.45) / 2,
              backgroundColor: '#ffffff',
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
    borderWidth: 1.5,
  },
  halo: {
    position: 'absolute',
  },
  orbCore: {
    justifyContent: 'center',
    alignItems: 'center',
    shadowOffset: { width: 0, height: 0 },
    shadowOpacity: 0.9,
    shadowRadius: 28,
    elevation: 20,
  },
  innerGlow: {
    opacity: 0.35,
  },
});
