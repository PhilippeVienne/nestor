import { useState } from 'react';
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
import { Card, SituationPanel, SituationStrip, AlertsPanel, TasksPanel, DevicesPanel, SystemPanel, HealthPanel } from './components/DashboardPanels';

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
    health,
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
  // Reaction aux transitions pendant le rendu (et non dans un effet) : l'utilisateur
  // garde la main entre deux changements, par exemple pour rouvrir la console.
  const runningToolsCount = toolCalls.filter((t) => t.status === 'running').length;
  const [consoleTrigger, setConsoleTrigger] = useState({ runningToolsCount, needsAnswer });
  if (consoleTrigger.runningToolsCount !== runningToolsCount || consoleTrigger.needsAnswer !== needsAnswer) {
    setConsoleTrigger({ runningToolsCount, needsAnswer });
    if (needsAnswer) {
      setIsConsoleOpen(false);
    } else if (runningToolsCount > 0) {
      setIsConsoleOpen(true);
    }
  }

  const leftPanels = (
    <>
      <SituationPanel context={context} />
      <AlertsPanel activity={activity} />
      <TasksPanel todos={todos} onAdd={addTodo} onComplete={completeTodo} onDelete={deleteTodo} />
    </>
  );

  const rightPanels = (
    <>
      <Card title="Missions" aside={missions.some((m) => m.status === 'started') ? <span className="pill h-6 text-think-300 border-think-600/50">en cours</span> : undefined}>
        {missions.length === 0 ? (
          <p className="m-0 text-[13px] text-ivory-700">Aucune mission. Les tâches longues sont déléguées ici.</p>
        ) : (
          <MissionPanel missions={missions} usage={null} onStopMission={stopMission} bare />
        )}
      </Card>
      <Card title="Conscience" aside={<span className="font-mono text-[11px] text-ivory-500">{settings?.judge_model ?? '—'}</span>}>
        <div className="-m-3 max-h-80 flex flex-col">
          <ConsciencePanel judgements={judgements} onResolve={resolveJudgement} />
        </div>
      </Card>
      <HealthPanel health={health} />
      <DevicesPanel clients={clients} connected={connectionState === 'connected'} />
      <SystemPanel telemetry={telemetry} usage={usage} context={context} />
    </>
  );

  return (
    <div
      inert={hidden}
      className="flex flex-col h-screen w-screen bg-ink-950 text-ivory-100 overflow-hidden font-sans select-none"
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
        <aside className="hidden xl:flex w-[330px] shrink-0 flex-col gap-3 p-3 overflow-y-auto border-r border-ink-800 select-text">
          {leftPanels}
        </aside>

        {/* Left/Center Cockpit Area */}
        <div className="flex-1 flex flex-col h-full overflow-hidden relative">
          {/* Ambient Background Gradient based on Status */}
          <div
            className={`absolute inset-0 pointer-events-none transition-opacity duration-1000 ${
              status === 'listening'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(95,199,187,0.18),transparent_70%)]'
                : status === 'thinking'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(160,143,228,0.18),transparent_70%)]'
                : status === 'speaking'
                ? 'opacity-30 bg-[radial-gradient(ellipse_at_top,rgba(207,165,82,0.2),transparent_70%)]'
                : 'opacity-15 bg-[radial-gradient(ellipse_at_top,rgba(207,165,82,0.1),transparent_70%)]'
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
                  ? 'bg-listen-400/20'
                  : status === 'thinking'
                  ? 'bg-think-400/25'
                  : status === 'speaking'
                  ? 'bg-brass-400/25'
                  : 'bg-brass-400/8'
              }`}
            />
          </div>

          {/* Sous 1280 px, la situation du jour reste visible sans ouvrir le tiroir */}
          <SituationStrip context={context} />

          {/* Bloc Voix : ce que le daemon entend face au seuil d'interruption */}
          <VoiceHud meter={voiceMeter} settings={settings} />

          {/* Dialogue Section (STT voice transcripts + Streaming Claude Markdown) */}
          <div className="flex-1 overflow-hidden relative">
            <DialogueStream
              messages={messages}
              status={status}
              onSelectSuggestion={(txt) => sendText(txt)}
            />
          </div>

          {/* Journal d'activite sous le dialogue (grand ecran) */}
          <div className="hidden xl:flex shrink-0 h-36 flex-col border-t border-ink-800 bg-ink-900/40 select-text">
            <div className="px-3 pt-2 text-[12px] text-ivory-500">Journal</div>
            <ActivityLog activity={activity} />
          </div>

          {/* Ecriture d'un connecteur externe en attente d'accord */}
          {toolApprovals.map((approval) => (
            <div
              key={approval.id}
              className="attention shrink-0 z-10 px-4 sm:px-6 py-3 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm select-text"
            >
              <div className="flex-1 min-w-[220px]">
                <div className="font-medium text-alert-300">
                  {approval.server} veut écrire : <span className="font-mono">{approval.tool}</span>
                </div>
                <pre className="m-0 mt-1 max-h-28 overflow-auto rounded-lg bg-ink-950 p-2 text-[12px] text-ivory-300 whitespace-pre-wrap break-words">
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
            <div className="attention shrink-0 z-10 px-4 sm:px-6 py-3 flex flex-wrap items-center gap-x-6 gap-y-2 text-sm select-text">
              <div className="flex-1 min-w-[220px]">
                <div className="font-medium text-alert-300">Votre accord est demandé</div>
                <div className="text-ivory-100 break-words">{pendingJudgement.text}</div>
                {pendingJudgement.rationale && (
                  <div className="text-[13px] text-ivory-500 break-words">{pendingJudgement.rationale}</div>
                )}
              </div>
              <div className="w-full sm:w-[260px]">
                <JudgeActions id={pendingJudgement.id} onResolve={resolveJudgement} />
              </div>
            </div>
          )}

          {/* Bottom Controls Bar (Barge-in + Text Fallback + VU-meter + Remote Audio) */}
          <div className="border-t border-ink-800 bg-ink-950/95 backdrop-blur shrink-0 z-10">
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
        <aside className="hidden xl:flex w-[340px] shrink-0 flex-col gap-3 p-3 overflow-y-auto border-l border-ink-800 select-text">
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
          <div className="xl:hidden absolute inset-0 z-30 flex flex-col bg-ink-950 select-text">
            <div className="shrink-0 flex items-center justify-between gap-3 px-4 py-3 border-b border-ink-800">
              <h2 className="m-0 font-display text-[20px] text-ivory-50">Tableau de bord</h2>
              <button
                type="button"
                aria-label="Fermer le tableau de bord"
                onClick={() => setIsDashboardOpen(false)}
                className="btn btn-icon"
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
