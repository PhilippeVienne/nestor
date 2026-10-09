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

/** Journal d'activite : les evenements du daemon depuis l'ouverture de la page, du plus recent au plus ancien. */
export const ActivityLog: React.FC<{ activity: ActivityItem[] }> = ({ activity }) => (
  <div className="flex-1 overflow-y-auto p-3 text-sm">
    {activity.length === 0 ? (
      <p className="text-xs text-slate-500 py-6 text-center">Aucune activité depuis l'ouverture de la page.</p>
    ) : (
      <ol className="space-y-2">
        {activity.map((item) => (
          <li key={item.id} className="flex gap-3">
            <span className="w-[62px] shrink-0 font-mono text-[11px] text-slate-500 pt-0.5">
              {item.timestamp.toLocaleTimeString()}
            </span>
            <span className="min-w-0">
              <span className="font-mono text-[10px] uppercase tracking-wider text-slate-400">{KIND_LABEL[item.kind]}</span>
              <span className="block text-slate-200 break-words">{item.text}</span>
            </span>
          </li>
        ))}
      </ol>
    )}
  </div>
);
