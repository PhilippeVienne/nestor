import { useState, useEffect } from 'react';
import { useNestorWebSocket } from './hooks/useNestorWebSocket';
import { OrbCanvas } from './components/OrbCanvas';
import { DialogueStream } from './components/DialogueStream';
import { ToolConsole } from './components/ToolConsole';
import { ControlBar } from './components/ControlBar';
import { Header } from './components/Header';

export function App() {
  const {
    connectionState,
    status,
    setStatus,
    audioLevels,
    messages,
    toolCalls,
    missions,
    usage,
    stopMission,
    sendBargeIn,
    sendText,
    isSimulated,
    setIsSimulated,
    isMicActive,
    toggleMic,
    isSpeakerActive,
    toggleSpeaker,
  } = useNestorWebSocket();

  // On desktop default open, on mobile default closed
  const [isConsoleOpen, setIsConsoleOpen] = useState(() => {
    return typeof window !== 'undefined' ? window.innerWidth >= 768 : true;
  });

  // Automatically adapt on resize
  useEffect(() => {
    const handleResize = () => {
      if (window.innerWidth < 768 && isConsoleOpen) {
        // Keep user preference or allow drawer
      }
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [isConsoleOpen]);

  const runningToolsCount = toolCalls.filter((t) => t.status === 'running').length;

  return (
    <div className="flex flex-col h-screen w-screen bg-[#07090e] text-slate-100 overflow-hidden font-sans select-none">
      {/* Top Header */}
      <Header
        status={status}
        connectionState={connectionState}
        onSetStatus={setStatus}
        isSimulated={isSimulated}
        onToggleSimulated={() => setIsSimulated(!isSimulated)}
        isConsoleOpen={isConsoleOpen}
        onToggleConsole={() => setIsConsoleOpen(!isConsoleOpen)}
        runningToolsCount={runningToolsCount}
      />

      {/* Main Workspace Body */}
      <div className="flex-1 flex overflow-hidden relative">
        {/* Left/Center Cockpit Area */}
        <div className="flex-1 flex flex-col h-full overflow-hidden relative">
          {/* Ambient Background Gradient based on Status */}
          <div
            className={`absolute inset-0 pointer-events-none transition-opacity duration-1000 ${
              status === 'listening'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(6,182,212,0.25),transparent_60%)]'
                : status === 'thinking'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(168,85,247,0.25),transparent_60%)]'
                : status === 'speaking'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(245,158,11,0.25),transparent_60%)]'
                : 'opacity-15 bg-[radial-gradient(ellipse_at_top,rgba(56,189,248,0.15),transparent_60%)]'
            }`}
          />

          {/* Hero Section: 3D Three.js Particle Orb */}
          <div className="relative w-full h-48 sm:h-64 md:h-80 shrink-0 flex items-center justify-center border-b border-slate-800/60 bg-gradient-to-b from-slate-950/40 to-slate-900/10">
            {/* The 3D Orb Canvas */}
            <OrbCanvas status={status} audioLevels={audioLevels} />

            {/* Futuristic HUD overlay badges (Cleaned for mobile) */}
            <div className="absolute top-3 left-3 sm:top-4 sm:left-5 flex flex-col gap-0.5 text-[9px] sm:text-[10px] font-mono text-slate-500 pointer-events-none">
              <span className="flex items-center gap-1 sm:gap-1.5">
                <span className={`w-1.5 h-1.5 rounded-full ${isMicActive ? 'bg-emerald-400 animate-ping' : 'bg-cyan-400'}`} />
                <span className="font-medium text-slate-300">
                  {isMicActive ? 'MICRO DISTANT ACTIF' : 'ORBE QUANTIQUE'}
                </span>
              </span>
              <span className="text-slate-600 hidden sm:block">
                17 000 POINTS D'ÉNERGIE • SYNC FFT
              </span>
            </div>

            <div className="absolute top-3 right-3 sm:top-4 sm:right-5 flex flex-col items-end gap-0.5 text-[9px] sm:text-[10px] font-mono text-slate-500 pointer-events-none">
              <span className="text-slate-400">
                MODULATION : <span className="text-cyan-400 uppercase font-semibold">{status}</span>
              </span>
              <span className="text-slate-600 hidden sm:block">
                RMS: {(audioLevels.rms).toFixed(2)} | PK: {(audioLevels.peak).toFixed(2)}
              </span>
            </div>

            {/* Subtle glow circle under the orb */}
            <div
              className={`absolute bottom-4 sm:bottom-6 w-48 sm:w-56 h-10 sm:h-12 rounded-full blur-2xl pointer-events-none transition-all duration-500 ${
                status === 'listening'
                  ? 'bg-cyan-500/20'
                  : status === 'thinking'
                  ? 'bg-purple-500/25'
                  : status === 'speaking'
                  ? 'bg-amber-500/25'
                  : 'bg-cyan-500/10'
              }`}
            />
          </div>

          {/* Dialogue Section (STT voice transcripts + Streaming Claude Markdown) */}
          <div className="flex-1 overflow-hidden relative bg-slate-950/40">
            <DialogueStream
              messages={messages}
              status={status}
              onSelectSuggestion={(txt) => sendText(txt)}
            />
          </div>

          {/* Bottom Controls Bar (Barge-in + Text Fallback + VU-meter + Remote Audio) */}
          <div className="border-t border-slate-800/80 bg-slate-950/95 backdrop-blur-xl shrink-0 z-10">
            <ControlBar
              status={status}
              audioLevels={audioLevels}
              isMicActive={isMicActive}
              onToggleMic={toggleMic}
              isSpeakerActive={isSpeakerActive}
              onToggleSpeaker={toggleSpeaker}
              onBargeIn={sendBargeIn}
              onSendText={sendText}
            />
          </div>
        </div>

        {/* Right Lateral Observability Console for Tool Calls (Sidebar on desktop, slide-over drawer on mobile) */}
        <ToolConsole
          toolCalls={toolCalls}
          missions={missions}
          usage={usage}
          onStopMission={stopMission}
          isOpen={isConsoleOpen}
          onToggle={() => setIsConsoleOpen(!isConsoleOpen)}
        />
      </div>
    </div>
  );
}

export default App;
