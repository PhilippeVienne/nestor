import React, { useEffect, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import type { ClientInfo, ConnectorInfo, ContextInfo, TelemetryInfo, TodoItem, UsageInfo } from '../types';

/** Carte d'un panneau du tableau de bord. */
export const Card: React.FC<{ title: string; aside?: React.ReactNode; children: React.ReactNode }> = ({
  title,
  aside,
  children,
}) => (
  <section className="rounded-xl border border-slate-800 bg-slate-900/40 p-4 flex flex-col gap-3 text-sm">
    <div className="flex items-center justify-between gap-3">
      <h2 className="text-[11px] font-mono font-semibold tracking-[0.14em] text-slate-400 uppercase">{title}</h2>
      {aside}
    </div>
    {children}
  </section>
);

const Row: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className="flex justify-between gap-3">
    <span className="text-slate-400 shrink-0">{label}</span>
    <span className="text-slate-100 text-right break-words min-w-0">{children}</span>
  </div>
);

/** Valeur que nestord ne connait pas encore : affichee comme telle, jamais inventee. */
const Unknown: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <span className="text-slate-500">{children}</span>
);

// ------------------------------------------------------------------ Situation

export const SituationPanel: React.FC<{ context: ContextInfo | null }> = ({ context }) => {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <Card title="Situation">
      <Row label="Heure">
        {now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        <span className="text-slate-400"> · {now.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long' })}</span>
      </Row>
      <Row label="Lieu">{context?.place ?? <Unknown>inconnu</Unknown>}</Row>
      <Row label="Heures calmes">
        {context ? (
          <>
            {context.quiet_start}–{context.quiet_end}
            <span className={context.quiet_active ? 'text-amber-300' : 'text-slate-400'}>
              {context.quiet_active ? ' · en cours' : ' · hors plage'}
            </span>
          </>
        ) : (
          <Unknown>—</Unknown>
        )}
      </Row>
      <div className="border-t border-slate-800 pt-3 flex flex-col gap-2">
        <Row label="Présence">
          <Unknown>non mesurée</Unknown>
        </Row>
        <Row label="Prochain rendez-vous">
          <Unknown>agenda non branché</Unknown>
        </Row>
        <Row label="Mails">
          <Unknown>non branchés</Unknown>
        </Row>
      </div>
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

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    onAdd(title, daily ? { recurrence: 'daily' } : dueAt ? { dueAt } : undefined);
    setTitle('');
    setDueAt('');
    setDaily(false);
  };

  return (
    <Card title="Tâches et rappels" aside={<span className="font-mono text-[11px] text-slate-400">{todos.length}</span>}>
      {todos.length === 0 ? (
        <p className="text-slate-500 text-[13px]">Aucune tâche en attente.</p>
      ) : (
        <ul className="flex flex-col">
          {todos.map((todo) => {
            const detail = todoDetail(todo);
            const late = !!todo.due_at && !todo.recurrence && todo.due_at * 1000 < Date.now();
            return (
              <li key={todo.id} className="flex items-center gap-2 min-h-11">
                <input
                  type="checkbox"
                  className="w-[18px] h-[18px] accent-cyan-400 shrink-0"
                  aria-label={`Marquer « ${todo.title} » comme faite`}
                  onChange={() => onComplete(todo.id)}
                />
                <span className="flex-1 min-w-0">
                  <span className="block text-slate-100 break-words">{todo.title}</span>
                  {detail && <span className={`block text-[12px] ${late ? 'text-amber-300' : 'text-slate-400'}`}>{detail}</span>}
                </span>
                <button
                  type="button"
                  aria-label={`Supprimer « ${todo.title} »`}
                  onClick={() => onDelete(todo.id)}
                  className="w-9 h-9 inline-flex items-center justify-center rounded-lg text-slate-500 hover:text-rose-300 hover:bg-slate-800/60 shrink-0"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <form onSubmit={submit} className="border-t border-slate-800 pt-3 flex flex-col gap-2 select-text">
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
            className="flex-1 min-w-0 h-11 px-3 rounded-lg border border-slate-700 bg-slate-950 text-slate-100"
          />
          <button
            type="submit"
            aria-label="Ajouter la tâche"
            className="w-11 h-11 inline-flex items-center justify-center rounded-lg border border-cyan-500/60 bg-cyan-900/50 text-cyan-50 hover:bg-cyan-800/60 shrink-0"
          >
            <Plus className="w-5 h-5" />
          </button>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-[13px] text-slate-300">
          <label className="flex items-center gap-2 min-h-9">
            <input type="checkbox" className="w-4 h-4 accent-cyan-400" checked={daily} onChange={(e) => setDaily(e.target.checked)} />
            Chaque jour
          </label>
          {!daily && (
            <label className="flex items-center gap-2 min-h-9">
              Échéance
              <input
                type="datetime-local"
                value={dueAt}
                onChange={(e) => setDueAt(e.target.value)}
                className="h-9 px-2 rounded-lg border border-slate-700 bg-slate-950 text-slate-100"
              />
            </label>
          )}
        </div>
      </form>
    </Card>
  );
};

// ------------------------------------------------------------------ Appareils

const CLIENT_LABEL: Record<string, string> = { web: 'Navigateur', mobile: 'Mobile', autre: 'Autre client' };

export const DevicesPanel: React.FC<{ clients: ClientInfo[]; connected: boolean }> = ({ clients, connected }) => (
  <Card title="Appareils">
    <Row label="Daemon nestord">
      <span className={connected ? 'text-cyan-300' : 'text-slate-500'}>{connected ? 'en ligne' : 'hors ligne'}</span>
    </Row>
    {clients.length === 0 ? (
      <p className="text-slate-500 text-[13px]">Aucun client connecté.</p>
    ) : (
      clients.map((client) => (
        <Row key={client.id} label={CLIENT_LABEL[client.kind] ?? client.kind}>
          <span className="text-cyan-300">connecté</span>
          <span className="text-slate-400">
            {' '}
            · {new Date(client.connected_at_ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </span>
        </Row>
      ))
    )}
  </Card>
);

// ------------------------------------------------------------------ Systeme

const Ms: React.FC<{ value?: number }> = ({ value }) =>
  value === undefined ? <Unknown>pas encore mesuré</Unknown> : <span className="font-mono">{value} ms</span>;

export const SystemPanel: React.FC<{ telemetry: TelemetryInfo | null; usage: UsageInfo | null }> = ({ telemetry, usage }) => (
  <Card title="Système">
    <Row label="Transcription">
      <span className="font-mono text-[13px]">{telemetry?.stt_model ?? <Unknown>—</Unknown>}</span>
    </Row>
    <Row label="Voix">
      <span className="font-mono text-[13px]">{telemetry?.tts_voice ?? <Unknown>—</Unknown>}</span>
    </Row>
    <Row label="Juge">
      <span className="font-mono text-[13px]">{telemetry?.judge_model ?? <Unknown>—</Unknown>}</span>
    </Row>
    <Row label="Carte graphique">
      {telemetry?.gpu ? (
        <span className="font-mono text-[13px]">
          {telemetry.gpu.name} · {(telemetry.gpu.memory_used_mb / 1024).toFixed(1)} / {(telemetry.gpu.memory_total_mb / 1024).toFixed(0)} Go
        </span>
      ) : (
        <Unknown>non détectée</Unknown>
      )}
    </Row>
    <div className="border-t border-slate-800 pt-3 flex flex-col gap-2">
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
    <div className="border-t border-slate-800 pt-3 flex flex-col gap-2">
      <Row label="Quota Claude, 5 heures">
        {usage ? <span className="font-mono">{Math.round(usage.fiveHour * 100)} %</span> : <Unknown>inconnu</Unknown>}
      </Row>
      <Row label="Quota Claude, 7 jours">
        {usage ? <span className="font-mono">{Math.round(usage.sevenDay * 100)} %</span> : <Unknown>inconnu</Unknown>}
      </Row>
    </div>
  </Card>
);

// ------------------------------------------------------------------ Connecteurs

export const ConnectorsList: React.FC<{ connectors: ConnectorInfo[] }> = ({ connectors }) => (
  <div className="flex flex-col gap-2">
    {connectors.map((connector) => (
      <div key={connector.name} className="rounded-lg border border-slate-800 p-3 flex flex-col gap-1.5">
        <div className="flex items-baseline justify-between gap-3">
          <span className="font-semibold text-slate-100">
            {connector.name} <span className="font-normal text-slate-400">· {connector.kind}</span>
          </span>
          <span className="font-mono text-[11px] text-cyan-300 shrink-0">
            connecté · {connector.tools.length} outils
          </span>
        </div>
        <div className="font-mono text-[11px] text-slate-400 break-words">{connector.tools.join(' · ')}</div>
      </div>
    ))}
    <div className="rounded-lg border border-dashed border-slate-700 p-3 text-[13px] text-slate-400">
      Aucun connecteur externe. Les accès personnels (messagerie, agenda…) arriveront en lecture seule, avec
      confirmation pour toute écriture.
    </div>
  </div>
);
