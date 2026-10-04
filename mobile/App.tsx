import React from 'react';
import { StyleSheet, PermissionsAndroid, Platform, Alert } from 'react-native';
import { StatusBar } from 'expo-status-bar';
import { useNestorCall } from './src/hooks/useNestorCall';
import { CallScreen } from './src/screens/CallScreen';
import { DialerScreen } from './src/screens/DialerScreen';

export default function App() {
  const call = useNestorCall();

  const handleStartCall = async (serverUrl: string, token?: string) => {
    if (Platform.OS === 'android') {
      try {
        const permissionsToRequest = [
          PermissionsAndroid.PERMISSIONS.RECORD_AUDIO,
        ];
        if (Platform.Version >= 33) {
          permissionsToRequest.push(PermissionsAndroid.PERMISSIONS.POST_NOTIFICATIONS);
        }

        const granted = await PermissionsAndroid.requestMultiple(permissionsToRequest);
        const recordGranted =
          granted[PermissionsAndroid.PERMISSIONS.RECORD_AUDIO] ===
          PermissionsAndroid.RESULTS.GRANTED;

        if (!recordGranted) {
          Alert.alert(
            'Permission requise',
            "Nestor a besoin de l'accès au microphone pour entendre vos instructions vocales."
          );
          return;
        }
      } catch (err) {
        console.warn('Error requesting permissions', err);
      }
    }

    call.startCall(serverUrl, token);
  };

  const isCallActive = call.callState === 'ACTIVE' || call.callState === 'CONNECTING';

  return (
    <>
      <StatusBar style="light" />
      {isCallActive ? (
        <CallScreen call={call} onEndCall={call.endCall} />
      ) : (
        <DialerScreen
          onStartCall={handleStartCall}
          isConnecting={call.callState === 'CONNECTING'}
          statusMessage={call.callStatusDetails}
        />
      )}
    </>
  );
}

const styles = StyleSheet.create({
  root: {
    flex: 1,
    backgroundColor: '#070b14',
  },
});
