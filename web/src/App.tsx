import { useState, useEffect } from 'react';
import { useNestorWebSocket } from './hooks/useNestorWebSocket';
import { OrbCanvas } from './components/OrbCanvas';
import { DialogueStream } from './components/DialogueStream';
import { ToolConsole } from './components/ToolConsole';
import { ControlBar } from './components/ControlBar';
import { Header } from './components/Header';
import { VoiceHud } from './components/VoiceHud';
import { SettingsPanel } from './components/SettingsPanel';
import { JudgeActions } from './components/ConsciencePanel';

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
    backendStatus,
    setBackend,
    isMicActive,
    toggleMic,
    isSpeakerActive,
    toggleSpeaker,
    settings,
    updateSettings,
    voiceMeter,
    judgements,
    resolveJudgement,
    activity,
  } = useNestorWebSocket();

  const pendingJudgement = judgements.find((j) => j.pending);

  // Default closed to keep UI clean and spacious; opens smoothly when user wants or tools run
  const [isConsoleOpen, setIsConsoleOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  // Automatically open console when tools start running if not already open
  const runningToolsCount = toolCalls.filter((t) => t.status === 'running').length;
  useEffect(() => {
    if (runningToolsCount > 0) {
      setIsConsoleOpen(true);
    }
  }, [runningToolsCount]);

  return (
    <div className="flex flex-col h-screen w-screen bg-[#080a0f] text-slate-100 overflow-hidden font-sans select-none">
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
        backendStatus={backendStatus}
        onSetBackend={setBackend}
        onOpenSettings={() => setIsSettingsOpen(true)}
      />

      {/* Main Workspace Body */}
      <div className="flex-1 flex overflow-hidden relative">
        {/* Left/Center Cockpit Area */}
        <div className="flex-1 flex flex-col h-full overflow-hidden relative">
          {/* Ambient Background Gradient based on Status */}
          <div
            className={`absolute inset-0 pointer-events-none transition-opacity duration-1000 ${
              status === 'listening'
                ? 'opacity-25 bg-[radial-gradient(ellipse_at_top,rgba(6,182,212,0.2),transparent_70%)]'
                : status === 'thinking'
                ? 'opacity-25 bg-[radial-gradient(ellipse_at_top,rgba(168,85,247,0.2),transparent_70%)]'
                : status === 'speaking'
                ? 'opacity-25 bg-[radial-gradient(ellipse_at_top,rgba(245,158,11,0.2),transparent_70%)]'
                : 'opacity-10 bg-[radial-gradient(ellipse_at_top,rgba(56,189,248,0.12),transparent_70%)]'
            }`}
          />

          {/* Hero Section: 3D Three.js Particle Orb (Minimalist, Floating) */}
          <div className="relative w-full h-40 sm:h-48 md:h-56 shrink-0 flex items-center justify-center overflow-hidden">
            {/* The 3D Orb Canvas */}
            <OrbCanvas status={status} audioLevels={audioLevels} />

            {/* Subtle glow circle under the orb */}
            <div
              className={`absolute bottom-2 sm:bottom-4 w-44 sm:w-52 h-8 sm:h-10 rounded-full blur-2xl pointer-events-none transition-all duration-500 ${
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

          {/* Bloc Voix : ce que le daemon entend face au seuil d'interruption */}
          <VoiceHud meter={voiceMeter} settings={settings} />

          {/* Dialogue Section (STT voice transcripts + Streaming Claude Markdown) */}
          <div className="flex-1 overflow-hidden relative bg-slate-950/40">
            <DialogueStream
              messages={messages}
              status={status}
              onSelectSuggestion={(txt) => sendText(txt)}
            />
          </div>

          {/* Confirmation demandee par le juge : visible meme console fermee */}
          {pendingJudgement && (
            <div className="shrink-0 z-10 border-t border-amber-500/40 bg-amber-950/40 px-3 sm:px-6 py-3 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm select-text">
              <div className="flex-1 min-w-[220px]">
                <div className="font-semibold text-amber-200">Confirmation requise par le juge</div>
                <div className="text-slate-100 break-words">{pendingJudgement.text}</div>
                {pendingJudgement.rationale && (
                  <div className="text-[13px] text-slate-300 break-words">{pendingJudgement.rationale}</div>
                )}
              </div>
              <div className="w-full sm:w-[260px]">
                <JudgeActions id={pendingJudgement.id} onResolve={resolveJudgement} />
              </div>
            </div>
          )}

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
          judgements={judgements}
          judgeModel={settings?.judge_model}
          onResolveJudgement={resolveJudgement}
          activity={activity}
        />

        <SettingsPanel
          isOpen={isSettingsOpen}
          onClose={() => setIsSettingsOpen(false)}
          settings={settings}
          onChange={updateSettings}
          lastInterruptRms={voiceMeter.lastInterruptRms}
        />
      </div>
    </div>
  );
}

export default App;
