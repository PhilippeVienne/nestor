import React, { useEffect, useState } from 'react';
import { Plus, Trash2, BellRing, CalendarClock, MapPin, Moon, MonitorDot } from 'lucide-react';
import type { ActivityItem, ClientInfo, ConnectorInfo, ContextInfo, HealthInfo, TelemetryInfo, TodoItem, ToolMode, UsageInfo } from '../types';

/** Heure courante, rafraichie a intervalle fixe : une echeance passe en retard sans interaction. */
function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

/** Carte d'un panneau du tableau de bord. */
export const Card: React.FC<{ title: string; aside?: React.ReactNode; children: React.ReactNode }> = ({ title, aside, children }) => (
  <section className="card text-sm">
    <div className="flex items-center justify-between gap-3">
      <h2 className="card-title m-0">{title}</h2>
      {aside}
    </div>
    {children}
  </section>
);

const Row: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className="flex justify-between gap-3 min-h-6">
    <span className="row-label shrink-0 pt-px">{label}</span>
    <span className="text-ivory-100 text-right break-words min-w-0">{children}</span>
  </div>
);

/** Valeur que nestord ne connait pas encore : affichee comme telle, jamais inventee. */
const Unknown: React.FC<{ children: React.ReactNode }> = ({ children }) => <span className="text-ivory-700">{children}</span>;

// ------------------------------------------------------------------ Situation

/** Duree d'inactivite lisible : « 3 min », « 2 h 05 ». */
function formatIdle(secs: number): string {
  const minutes = Math.floor(secs / 60);
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')}`;
}

/** Debut d'un rendez-vous : « 15:00 » aujourd'hui, sinon « jeu. 15:00 ». */
function formatEventStart(startMs: number, now: Date): string {
  const start = new Date(startMs);
  const time = start.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return start.toDateString() === now.toDateString() ? time : `${start.toLocaleDateString([], { weekday: 'short' })} ${time}`;
}

/** Temps restant avant un instant, en mots courts. */
function formatUntil(ms: number, now: number): string {
  const minutes = Math.round((ms - now) / 60_000);
  if (minutes <= 0) return 'maintenant';
  if (minutes < 60) return `dans ${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `dans ${hours} h ${String(minutes % 60).padStart(2, '0')}` : `dans ${Math.round(hours / 24)} j`;
}

/** Ligne d'etat avec son icone : presence, lieu, veille. */
const StateLine: React.FC<{ icon: React.ReactNode; label: string; children: React.ReactNode }> = ({ icon, label, children }) => (
  <div className="flex items-start gap-2.5">
    <span className="mt-0.5 text-ivory-500 shrink-0">{icon}</span>
    <span className="min-w-0 flex-1">
      <span className="row-label block">{label}</span>
      <span className="block text-ivory-100 break-words">{children}</span>
    </span>
  </div>
);

/**
 * La journee de Monsieur : l'heure et le prochain rendez-vous en tete, puis ce que
 * nestord sait de la situation (lieu, presence, veille).
 */
export const SituationPanel: React.FC<{ context: ContextInfo | null }> = ({ context }) => {
  const nowMs = useNow(30_000);
  const now = new Date(nowMs);
  const event = context?.next_event;

  return (
    <section className="card text-sm">
      <div className="flex items-baseline justify-between gap-3">
        <h2 className="card-title m-0">Situation</h2>
        <span className="font-display text-[22px] leading-none text-ivory-50 tabular-nums">
          {now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        </span>
      </div>
      <p className="m-0 -mt-1 text-[13px] text-ivory-500 capitalize">
        {now.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long' })}
      </p>

      {/* Prochain rendez-vous : le bloc le plus utile de la journee */}
      <div className="rounded-xl border border-ink-700 bg-ink-950/60 px-3.5 py-3 flex items-start gap-3">
        <CalendarClock className="w-4 h-4 mt-0.5 text-brass-400 shrink-0" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          {!context?.calendar_connected ? (
            <>
              <span className="block text-ivory-300">Agenda non branché</span>
              <span className="block text-[12px] text-ivory-700">nestord onboard --google</span>
            </>
          ) : context.calendar_error ? (
            <>
              <span className="block text-danger-300">Agenda illisible</span>
              <span className="block text-[12px] text-ivory-500 break-words">{context.calendar_error}</span>
            </>
          ) : event ? (
            <>
              <span className="block font-display text-[17px] leading-tight text-ivory-50 break-words">{event.title}</span>
              <span className="block text-[13px] text-ivory-300">
                {formatEventStart(event.start_ms, now)} · {formatUntil(event.start_ms, nowMs)}
              </span>
              {event.location && !event.online && (
                <span className="block text-[12px] text-ivory-500 truncate">{event.location}</span>
              )}
              {event.online && <span className="block text-[12px] text-ivory-500">en ligne</span>}
            </>
          ) : (
            <span className="block text-ivory-300">Rien à l'agenda dans les prochaines heures</span>
          )}
        </div>
      </div>

      <div className="flex flex-col gap-2.5 pt-1">
        <StateLine icon={<MapPin className="w-4 h-4" />} label="Lieu">
          {context?.place ?? <Unknown>inconnu</Unknown>}
        </StateLine>
        <StateLine icon={<MonitorDot className="w-4 h-4" />} label="Présence">
          {context?.present === undefined ? (
            <Unknown>non mesurée</Unknown>
          ) : context.present ? (
            <span className="text-ok-300">devant l'ordinateur</span>
          ) : (
            <span className="text-alert-300">absent depuis {formatIdle(context.idle_secs ?? 0)}</span>
          )}
        </StateLine>
        <StateLine icon={<Moon className="w-4 h-4" />} label="Veille de la machine">
          {context?.inhibit ? (
            <>
              <span className="text-listen-300">empêchée</span>
              <span className="block text-[12px] text-ivory-500 break-words">{context.inhibit}</span>
            </>
          ) : !context?.sleep_managed ? (
            <span className="text-ivory-300">laissée à GNOME</span>
          ) : context.sleep_allowed ? (
            <>
              <span className="text-ok-300">permise</span>
              {context.wake_at_ms && <span className="block text-[12px] text-ivory-500">réveil {formatEventStart(context.wake_at_ms, now)}</span>}
            </>
          ) : (
            <>
              <span className="text-alert-300">différée</span>
              <span className="block text-[12px] text-ivory-500 break-words">{context.sleep_blockers.join(', ')}</span>
              {context.wake_armed && context.wake_at_ms && (
                <span className="block text-[12px] text-ivory-500">réveil armé {formatEventStart(context.wake_at_ms, now)}</span>
              )}
            </>
          )}
        </StateLine>
        {context && (
          <p className="m-0 text-[12px] text-ivory-700">
            Heures calmes {context.quiet_start}–{context.quiet_end}
            {context.quiet_active ? ' · en cours' : ''}
          </p>
        )}
      </div>
    </section>
  );
};

/** Sous 1280 px, l'essentiel de la situation en une ligne au-dessus du dialogue. */
export const SituationStrip: React.FC<{ context: ContextInfo | null }> = ({ context }) => {
  const nowMs = useNow(30_000);
  const now = new Date(nowMs);
  const event = context?.next_event;
  return (
    <div className="xl:hidden shrink-0 px-4 py-2 border-b border-ink-800 bg-ink-900/40 flex flex-wrap items-baseline gap-x-4 gap-y-1 text-[13px]">
      <span className="font-display text-[17px] text-ivory-50 tabular-nums">{now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
      <span className="text-ivory-100 min-w-0 truncate">
        {event ? (
          <>
            <CalendarClock className="inline w-3.5 h-3.5 mr-1 -mt-0.5 text-brass-400" aria-hidden="true" />
            {event.title} · {formatEventStart(event.start_ms, now)}
          </>
        ) : context?.calendar_connected ? (
          <span className="text-ivory-500">rien à l'agenda</span>
        ) : (
          <span className="text-ivory-700">agenda non branché</span>
        )}
      </span>
      <span className="text-ivory-500">
        {context?.place ?? 'lieu inconnu'}
        {context?.present === undefined ? '' : context.present ? ' · présent' : ` · absent ${formatIdle(context.idle_secs ?? 0)}`}
      </span>
    </div>
  );
};

// ------------------------------------------------------------------ Alertes

const ALERT_KIND_LABEL: Record<string, string> = {
  departure: 'Départ',
  event_imminent: 'Rendez-vous',
  mission_stalled: 'Mission',
  session_fallback: 'Mode réduit',
  quota: 'Quota',
  todo_due: 'Tâches',
  wake: 'Réveil',
  health: 'Santé',
};

/** Ce que Nestor a signale de lui-meme : les alertes de la boucle proactive. */
export const AlertsPanel: React.FC<{ activity: ActivityItem[] }> = ({ activity }) => {
  const alerts = activity.filter((a) => a.kind === 'alert').slice(0, 6);
  return (
    <Card
      title="Alertes"
      aside={<BellRing className={`w-4 h-4 ${alerts.length > 0 ? 'text-brass-400' : 'text-ivory-700'}`} aria-hidden="true" />}
    >
      {alerts.length === 0 ? (
        <p className="m-0 text-[13px] text-ivory-700">Rien à signaler. Nestor préviendra de lui-même : départ, rendez-vous, mission, quota.</p>
      ) : (
        <ol className="flex flex-col gap-2">
          {alerts.map((alert) => {
            const kind = alert.text.match(/^\[(\w+)\]/)?.[1];
            return (
              <li key={alert.id} className="flex gap-3 items-baseline">
                <span className="font-mono text-[11px] text-ivory-700 tabular-nums shrink-0">
                  {alert.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                </span>
                <span className="min-w-0 text-ivory-100 break-words">
                  {kind && ALERT_KIND_LABEL[kind] ? <span className="text-brass-300">{ALERT_KIND_LABEL[kind]} · </span> : null}
                  {alert.text}
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </Card>
  );
};

// ------------------------------------------------------------------ Taches

const RECURRENCE_LABEL: Record<string, string> = { daily: 'chaque jour' };

function todoDetail(todo: TodoItem): string | null {
  if (todo.recurrence) {
    if (RECURRENCE_LABEL[todo.recurrence]) return RECURRENCE_LABEL[todo.recurrence];
    const [kind, value] = todo.recurrence.split(':');
    if (kind === 'weekly') return `chaque ${value}`;
    if (kind === 'monthly') return `le ${value} du mois`;
    return todo.recurrence;
  }
  if (todo.due_at) {
    const due = new Date(todo.due_at * 1000);
    const late = due.getTime() < Date.now();
    return `${late ? 'en retard · ' : ''}${due.toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}`;
  }
  return null;
}

export const TasksPanel: React.FC<{
  todos: TodoItem[];
  onAdd: (title: string, options?: { dueAt?: string; recurrence?: string }) => void;
  onComplete: (id: number) => void;
  onDelete: (id: number) => void;
}> = ({ todos, onAdd, onComplete, onDelete }) => {
  const [title, setTitle] = useState('');
  const [dueAt, setDueAt] = useState('');
  const [daily, setDaily] = useState(false);
  const now = useNow(60_000);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    onAdd(title, daily ? { recurrence: 'daily' } : dueAt ? { dueAt } : undefined);
    setTitle('');
    setDueAt('');
    setDaily(false);
  };

  return (
    <Card title="Tâches" aside={<span className="font-mono text-[12px] text-ivory-500 tabular-nums">{todos.length}</span>}>
      {todos.length === 0 ? (
        <p className="m-0 text-[13px] text-ivory-700">Rien en attente.</p>
      ) : (
        <ul className="flex flex-col m-0 p-0 list-none">
          {todos.map((todo) => {
            const detail = todoDetail(todo);
            const late = !!todo.due_at && !todo.recurrence && todo.due_at * 1000 < now;
            return (
              <li key={todo.id} className="flex items-center gap-2.5 min-h-11">
                <input
                  type="checkbox"
                  className="w-[18px] h-[18px] accent-brass-400 shrink-0"
                  aria-label={`Marquer « ${todo.title} » comme faite`}
                  onChange={() => onComplete(todo.id)}
                />
                <span className="flex-1 min-w-0">
                  <span className="block text-ivory-100 break-words">{todo.title}</span>
                  {detail && <span className={`block text-[12px] ${late ? 'text-alert-300' : 'text-ivory-500'}`}>{detail}</span>}
                </span>
                <button
                  type="button"
                  aria-label={`Supprimer « ${todo.title} »`}
                  onClick={() => onDelete(todo.id)}
                  className="btn btn-quiet btn-icon min-h-9 w-9 hover:text-danger-300 shrink-0"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <form onSubmit={submit} className="border-t border-ink-800 pt-3 flex flex-col gap-2 select-text">
        <label htmlFor="todo-title" className="sr-only">
          Nouvelle tâche
        </label>
        <div className="flex gap-2">
          <input
            id="todo-title"
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Nouvelle tâche…"
            className="field flex-1 min-w-0"
          />
          <button type="submit" aria-label="Ajouter la tâche" className="btn btn-primary btn-icon shrink-0">
            <Plus className="w-5 h-5" />
          </button>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-[13px] text-ivory-300">
          <label className="flex items-center gap-2 min-h-9">
            <input type="checkbox" className="w-4 h-4 accent-brass-400" checked={daily} onChange={(e) => setDaily(e.target.checked)} />
            Chaque jour
          </label>
          {!daily && (
            <label className="flex items-center gap-2 min-h-9">
              Échéance
              <input type="datetime-local" value={dueAt} onChange={(e) => setDueAt(e.target.value)} className="field min-h-9 px-2 text-[13px]" />
            </label>
          )}
        </div>
      </form>
    </Card>
  );
};

// ------------------------------------------------------------------ Appareils

const CLIENT_LABEL: Record<string, string> = { web: 'Navigateur', mobile: 'Téléphone en appel', standby: 'Téléphone en veille', autre: 'Autre client' };

export const DevicesPanel: React.FC<{ clients: ClientInfo[]; connected: boolean }> = ({ clients, connected }) => (
  <Card title="Appareils">
    <Row label="Daemon nestord">
      <span className={connected ? 'text-ok-300' : 'text-ivory-700'}>{connected ? 'en ligne' : 'hors ligne'}</span>
    </Row>
    {clients.length === 0 ? (
      <p className="m-0 text-[13px] text-ivory-700">Aucun client connecté.</p>
    ) : (
      clients.map((client) => (
        <Row key={client.id} label={CLIENT_LABEL[client.kind] ?? client.kind}>
          <span className="text-ivory-100">depuis {new Date(client.connected_at_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
        </Row>
      ))
    )}
  </Card>
);

// ------------------------------------------------------------------ Sante

const HEALTH_TONE: Record<string, string> = { ok: 'bg-ok-400', warn: 'bg-alert-400', down: 'bg-danger-400' };

/** Ce qui doit tourner autour de nestord : juge, bord Tailscale, certificats, sauvegarde. */
export const HealthPanel: React.FC<{ health: HealthInfo | null }> = ({ health }) => {
  const down = health?.items.filter((i) => i.status === 'down').length ?? 0;
  return (
    <Card
      title="Santé"
      aside={
        health ? (
          <span className={`text-[12px] ${down > 0 ? 'text-danger-300' : 'text-ivory-500'}`}>
            {down > 0 ? `${down} hors service` : `vérifiée ${new Date(health.checked_at_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`}
          </span>
        ) : undefined
      }
    >
      {!health ? (
        <p className="m-0 text-[13px] text-ivory-700">Première vérification dans quelques secondes.</p>
      ) : (
        <ul className="m-0 p-0 list-none flex flex-col gap-1.5">
          {health.items.map((i) => (
            <li key={i.name} className="flex items-baseline gap-2.5">
              <span className={`w-2 h-2 rounded-full shrink-0 ${HEALTH_TONE[i.status] ?? 'bg-ivory-700'}`} aria-hidden="true" />
              <span className="row-label shrink-0">{i.name}</span>
              <span className={`min-w-0 flex-1 text-right break-words ${i.status === 'down' ? 'text-danger-300' : i.status === 'warn' ? 'text-alert-300' : 'text-ivory-300'}`}>
                {i.detail}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
};

// ------------------------------------------------------------------ Systeme

const Ms: React.FC<{ value?: number }> = ({ value }) =>
  value === undefined ? <Unknown>pas encore mesuré</Unknown> : <span className="font-mono text-[13px] tabular-nums">{value} ms</span>;

/** Jauge fine : quota, memoire de la carte. */
const Gauge: React.FC<{ value: number; tone?: 'brass' | 'alert' | 'danger' }> = ({ value, tone = 'brass' }) => (
  <span className="inline-block w-16 h-1.5 rounded-full bg-ink-700 overflow-hidden align-middle mr-2">
    <span
      className={`block h-full rounded-full ${tone === 'danger' ? 'bg-danger-400' : tone === 'alert' ? 'bg-alert-400' : 'bg-brass-400'}`}
      style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
    />
  </span>
);

const Quota: React.FC<{ value: number }> = ({ value }) => {
  const pct = Math.round(value * 100);
  return (
    <>
      <Gauge value={pct} tone={pct >= 85 ? 'danger' : pct >= 60 ? 'alert' : 'brass'} />
      <span className="font-mono text-[13px] tabular-nums">{pct} %</span>
    </>
  );
};

export const SystemPanel: React.FC<{ telemetry: TelemetryInfo | null; usage: UsageInfo | null; context: ContextInfo | null }> = ({
  telemetry,
  usage,
  context,
}) => (
  <Card title="Système">
    <Row label="Quota Claude, 5 h">{usage ? <Quota value={usage.fiveHour} /> : <Unknown>inconnu</Unknown>}</Row>
    <Row label="Quota Claude, 7 j">{usage ? <Quota value={usage.sevenDay} /> : <Unknown>inconnu</Unknown>}</Row>
    <Row label="Mémoire longue">
      {context?.memory_facts === undefined ? <Unknown>—</Unknown> : <span className="tabular-nums">{context.memory_facts} faits</span>}
    </Row>
    <div className="border-t border-ink-800 pt-3 flex flex-col gap-2">
      <Row label="Transcription">
        <Ms value={telemetry?.stt_ms} />
      </Row>
      <Row label="Premier mot">
        <Ms value={telemetry?.first_word_ms} />
      </Row>
      <Row label="Synthèse">
        <Ms value={telemetry?.tts_ms} />
      </Row>
    </div>
    <div className="border-t border-ink-800 pt-3 flex flex-col gap-2 text-[13px]">
      <Row label="Modèles">
        <span className="font-mono text-[12px] text-ivory-300 break-all">
          {[telemetry?.stt_model, telemetry?.tts_voice, telemetry?.judge_model].filter(Boolean).join(' · ') || <Unknown>—</Unknown>}
        </span>
      </Row>
      <Row label="Carte graphique">
        {telemetry?.gpu ? (
          <span className="font-mono text-[12px] text-ivory-300">
            <Gauge value={(telemetry.gpu.memory_used_mb / telemetry.gpu.memory_total_mb) * 100} />
            {(telemetry.gpu.memory_used_mb / 1024).toFixed(1)} / {(telemetry.gpu.memory_total_mb / 1024).toFixed(0)} Go
          </span>
        ) : (
          <Unknown>non détectée</Unknown>
        )}
      </Row>
    </div>
  </Card>
);

// ------------------------------------------------------------------ Connecteurs

const STATUS_LABEL: Record<string, string> = {
  connected: 'connecté',
  connecting: 'connexion…',
  error: 'en erreur',
  disabled: 'non démarré',
};

const MODE_LABEL: Record<ToolMode, string> = {
  read: 'Lecture libre',
  confirm: 'Avec mon accord',
  off: 'Non exposé',
};

export const ConnectorsList: React.FC<{
  connectors: ConnectorInfo[];
  onSetToolMode: (server: string, tool: string, mode: ToolMode) => void;
}> = ({ connectors, onSetToolMode }) => {
  const externals = connectors.filter((c) => c.kind !== 'interne');
  return (
    <div className="flex flex-col gap-2">
      <div className="rounded-xl border border-alert-600/50 bg-alert-600/10 p-3 text-[13px] text-alert-300">
        <span className="font-medium text-ivory-100">Règle par défaut : lecture seule, votre accord pour toute écriture.</span> Nestor
        consulte librement ; envoyer, modifier ou supprimer attend votre « oui », par bouton ou à la voix.
      </div>

      {connectors.map((connector) => (
        <div key={connector.name} className="rounded-xl border border-ink-800 p-3 flex flex-col gap-2">
          <div className="flex items-baseline justify-between gap-3">
            <span className="font-medium text-ivory-100">
              {connector.name} <span className="font-normal text-ivory-500">· {connector.kind}</span>
            </span>
            <span
              className={`font-mono text-[11px] shrink-0 ${
                connector.status === 'connected' ? 'text-ok-300' : connector.status === 'connecting' ? 'text-ivory-300' : 'text-alert-300'
              }`}
            >
              {STATUS_LABEL[connector.status] ?? connector.status}
              {connector.status === 'connected' ? ` · ${connector.tools.length} outils` : ''}
            </span>
          </div>
          {connector.detail && <div className="text-[13px] text-alert-300 break-words">{connector.detail}</div>}

          {connector.kind === 'interne' ? (
            <div className="font-mono text-[11px] text-ivory-500 break-words">{connector.tools.map((t) => t.name).join(' · ')}</div>
          ) : (
            connector.tools.map((tool) => (
              <div key={tool.name} className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 min-h-11">
                <label htmlFor={`mode-${connector.name}-${tool.name}`} className="min-w-0 flex-1">
                  <span className="block font-mono text-[13px] text-ivory-100 break-words">{tool.name}</span>
                  {tool.description && <span className="block text-[12px] text-ivory-500 break-words">{tool.description}</span>}
                </label>
                <select
                  id={`mode-${connector.name}-${tool.name}`}
                  value={tool.mode ?? 'confirm'}
                  onChange={(e) => onSetToolMode(connector.name, tool.name, e.target.value as ToolMode)}
                  className={`field text-[13px] shrink-0 ${
                    tool.mode === 'read' ? 'border-listen-600/60 text-listen-300' : tool.mode === 'off' ? 'text-ivory-500' : 'border-alert-600/60 text-alert-300'
                  }`}
                >
                  {(['read', 'confirm', 'off'] as const).map((mode) => (
                    <option key={mode} value={mode}>
                      {MODE_LABEL[mode]}
                      {tool.default_mode === mode ? ' (défaut)' : ''}
                    </option>
                  ))}
                </select>
              </div>
            ))
          )}
        </div>
      ))}

      {externals.length === 0 && (
        <div className="rounded-xl border border-dashed border-ink-700 p-3 text-[13px] text-ivory-500">
          Aucun connecteur externe. Déclarez un serveur dans <span className="font-mono text-ivory-100">config.toml</span> (section{' '}
          <span className="font-mono text-ivory-100">[[mcp_servers]]</span>), puis redémarrez nestord.
        </div>
      )}
      {externals.length > 0 && (
        <p className="m-0 text-[12px] text-ivory-500">
          Un changement de mode s'applique tout de suite aux appels. Exposer ou masquer un outil ne modifie la liste vue par l'assistant
          qu'à sa prochaine session.
        </p>
      )}
    </div>
  );
};
