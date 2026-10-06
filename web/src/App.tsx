import { useState, useEffect } from 'react';
import { X } from 'lucide-react';
import { useNestorWebSocket } from './hooks/useNestorWebSocket';
import { useAuth, type Auth } from './auth/useAuth';
import { LoginScreen } from './components/LoginScreen';
import { OrbCanvas } from './components/OrbCanvas';
import { DialogueStream } from './components/DialogueStream';
import { ToolConsole } from './components/ToolConsole';
import { ControlBar } from './components/ControlBar';
import { Header } from './components/Header';
import { VoiceHud } from './components/VoiceHud';
import { SettingsPanel } from './components/SettingsPanel';
import { JudgeActions, ConsciencePanel } from './components/ConsciencePanel';
import { ActivityLog } from './components/ActivityLog';
import { MissionPanel } from './components/MissionPanel';
import { Card, SituationPanel, TasksPanel, DevicesPanel, SystemPanel } from './components/DashboardPanels';

/**
 * L'ecran de connexion passe d'abord : l'application, et avec elle le WebSocket, n'est
 * montee qu'une fois une preuve d'acces obtenue ou si le daemon n'en exige aucune.
 */
export function App() {
  const auth = useAuth();
  const entered = auth.phase === 'ready';
  return (
    <>
      {(entered || auth.phase === 'opening') && <Dashboard key={auth.epoch} auth={auth} hidden={!entered} />}
      {!entered && <LoginScreen auth={auth} />}
    </>
  );
}

/** L'application elle-meme. `hidden` : encore masquee par l'ecran de connexion. */
function Dashboard({ auth, hidden }: { auth: Auth; hidden: boolean }) {
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
    context,
    todos,
    addTodo,
    completeTodo,
    deleteTodo,
    clients,
    telemetry,
    connectors,
    setToolMode,
    toolApprovals,
    resolveToolApproval,
  } = useNestorWebSocket({ onOpen: auth.onSocketOpen, onRefused: auth.onSocketRefused });

  // Default closed to keep UI clean and spacious; opens smoothly when user wants or tools run
  const [isConsoleOpen, setIsConsoleOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  // Petit ecran : les panneaux des colonnes s'ouvrent dans un tiroir plein ecran.
  const [isDashboardOpen, setIsDashboardOpen] = useState(false);

  const pendingJudgement = judgements.find((j) => j.pending);
  // Une decision attend l'utilisateur (accord d'ecriture, confirmation du juge).
  const needsAnswer = toolApprovals.length > 0 || !!pendingJudgement;

  // La console s'ouvre quand un outil demarre, sauf si une decision attend : son voile
  // recouvrirait le bandeau de reponse. Dans ce cas elle se referme.
  const runningToolsCount = toolCalls.filter((t) => t.status === 'running').length;
  useEffect(() => {
    if (needsAnswer) {
      setIsConsoleOpen(false);
    } else if (runningToolsCount > 0) {
      setIsConsoleOpen(true);
    }
  }, [runningToolsCount, needsAnswer]);

  const leftPanels = (
    <>
      <SituationPanel context={context} />
      <TasksPanel todos={todos} onAdd={addTodo} onComplete={completeTodo} onDelete={deleteTodo} />
      <DevicesPanel clients={clients} connected={connectionState === 'connected'} />
    </>
  );

  const rightPanels = (
    <>
      <Card title="Conscience" aside={<span className="font-mono text-[11px] text-slate-400">{settings?.judge_model ?? '—'}</span>}>
        <div className="-m-3 max-h-80 flex flex-col">
          <ConsciencePanel judgements={judgements} onResolve={resolveJudgement} />
        </div>
      </Card>
      <Card title="Missions">
        {missions.length === 0 ? (
          <p className="text-slate-500 text-[13px]">Aucune mission en cours.</p>
        ) : (
          <div className="-m-3">
            <MissionPanel missions={missions} usage={null} onStopMission={stopMission} />
          </div>
        )}
      </Card>
      <SystemPanel telemetry={telemetry} usage={usage} />
    </>
  );

  return (
    <div
      inert={hidden}
      className="flex flex-col h-screen w-screen bg-[#080a0f] text-slate-100 overflow-hidden font-sans select-none"
    >
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
        onOpenDashboard={() => setIsDashboardOpen(true)}
      />

      {/* Main Workspace Body */}
      <div className="flex-1 flex overflow-hidden relative">
        {/* Colonne gauche du tableau de bord (grand ecran) */}
        <aside className="hidden xl:flex w-[320px] shrink-0 flex-col gap-3 p-3 overflow-y-auto border-r border-slate-800/80 select-text">
          {leftPanels}
        </aside>

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

          {/* Journal d'activite sous le dialogue (grand ecran) */}
          <div className="hidden xl:flex shrink-0 h-36 flex-col border-t border-slate-800/80 bg-slate-950/60 select-text">
            <div className="px-3 pt-2 text-[11px] font-mono font-semibold tracking-[0.14em] text-slate-400 uppercase">Journal d'activité</div>
            <ActivityLog activity={activity} />
          </div>

          {/* Ecriture d'un connecteur externe en attente d'accord */}
          {toolApprovals.map((approval) => (
            <div
              key={approval.id}
              className="shrink-0 z-10 border-t border-amber-500/40 bg-amber-950/40 px-3 sm:px-6 py-3 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm select-text"
            >
              <div className="flex-1 min-w-[220px]">
                <div className="font-semibold text-amber-200">
                  {approval.server} veut écrire : <span className="font-mono">{approval.tool}</span>
                </div>
                <pre className="mt-1 max-h-28 overflow-auto rounded-lg bg-black/40 p-2 text-[12px] text-slate-200 whitespace-pre-wrap break-words">
                  {approval.arguments}
                </pre>
              </div>
              <div className="w-full sm:w-[260px]">
                <JudgeActions id={approval.id} onResolve={resolveToolApproval} />
              </div>
            </div>
          ))}

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

        {/* Colonne droite du tableau de bord (grand ecran) */}
        <aside className="hidden xl:flex w-[340px] shrink-0 flex-col gap-3 p-3 overflow-y-auto border-l border-slate-800/80 select-text">
          {rightPanels}
        </aside>

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

        {/* Tableau de bord en tiroir (petit ecran : les colonnes laterales sont masquees) */}
        {isDashboardOpen && (
          <div className="xl:hidden absolute inset-0 z-30 flex flex-col bg-[#080a0f] select-text">
            <div className="shrink-0 flex items-center justify-between gap-3 px-4 py-3 border-b border-slate-800">
              <h2 className="text-lg font-semibold text-slate-100">Tableau de bord</h2>
              <button
                type="button"
                aria-label="Fermer le tableau de bord"
                onClick={() => setIsDashboardOpen(false)}
                className="w-11 h-11 inline-flex items-center justify-center rounded-lg border border-slate-700 text-slate-300 hover:text-white hover:border-slate-500"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="flex-1 overflow-y-auto p-3 grid grid-cols-1 md:grid-cols-2 gap-3 content-start">
              <div className="flex flex-col gap-3 min-w-0">{leftPanels}</div>
              <div className="flex flex-col gap-3 min-w-0">{rightPanels}</div>
            </div>
          </div>
        )}

        <SettingsPanel
          isOpen={isSettingsOpen}
          onClose={() => setIsSettingsOpen(false)}
          settings={settings}
          onChange={updateSettings}
          lastInterruptRms={voiceMeter.lastInterruptRms}
          connectors={connectors}
          onSetToolMode={setToolMode}
          context={context}
          auth={auth}
          connected={connectionState === 'connected'}
        />
      </div>
    </div>
  );
}

export default App;
