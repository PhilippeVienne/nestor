import React from 'react';
import type { ActivityItem } from '../types';

const KIND_LABEL: Record<ActivityItem['kind'], string> = {
  wake: 'Veille',
  tool: 'Outil',
  mission: 'Mission',
  voice: 'Voix',
  judge: 'Juge',
  system: 'Système',
  alert: 'Alerte',
};

const KIND_TONE: Record<ActivityItem['kind'], string> = {
  wake: 'text-ivory-500',
  tool: 'text-listen-300',
  mission: 'text-think-300',
  voice: 'text-ivory-500',
  judge: 'text-alert-300',
  system: 'text-ivory-500',
  alert: 'text-brass-300',
};

/** Journal d'activite : les evenements du daemon depuis l'ouverture de la page, du plus recent au plus ancien. */
export const ActivityLog: React.FC<{ activity: ActivityItem[] }> = ({ activity }) => (
  <div className="flex-1 overflow-y-auto px-3 py-2 text-[13px]">
    {activity.length === 0 ? (
      <p className="text-ivory-700 py-6 text-center">Rien depuis l'ouverture de la page.</p>
    ) : (
      <ol className="flex flex-col gap-1.5">
        {activity.map((item) => (
          <li key={item.id} className="flex gap-3 items-baseline">
            <span className="w-[52px] shrink-0 font-mono text-[11px] text-ivory-700 tabular-nums">
              {item.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
            </span>
            <span className={`w-[58px] shrink-0 text-[11px] font-medium ${KIND_TONE[item.kind]}`}>{KIND_LABEL[item.kind]}</span>
            <span className="min-w-0 text-ivory-300 break-words">{item.text}</span>
          </li>
        ))}
      </ol>
    )}
  </div>
);
