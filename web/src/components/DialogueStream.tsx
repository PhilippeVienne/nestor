import React, { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Copy, Check, ArrowDown } from 'lucide-react';
import type { MessageItem, DaemonStatus } from '../types';

interface DialogueStreamProps {
  messages: MessageItem[];
  status: DaemonStatus;
  onSelectSuggestion?: (text: string) => void;
}

const SUGGESTIONS = ['Où en sont mes missions ?', "Qu'ai-je à faire aujourd'hui ?", 'Retiens que je préfère le train', "Quel est l'état de la machine ?"];

export const DialogueStream: React.FC<DialogueStreamProps> = ({ messages, status, onSelectSuggestion }) => {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [userHasScrolled, setUserHasScrolled] = useState(false);

  useEffect(() => {
    if (!userHasScrolled && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [messages, userHasScrolled]);

  const handleScroll = () => {
    if (!scrollContainerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef.current;
    setUserHasScrolled(scrollHeight - scrollTop - clientHeight >= 50);
  };

  const scrollToBottom = () => {
    scrollContainerRef.current?.scrollTo({ top: scrollContainerRef.current.scrollHeight, behavior: 'smooth' });
    setUserHasScrolled(false);
  };

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  return (
    <div className="relative flex flex-col h-full overflow-hidden">
      <div ref={scrollContainerRef} onScroll={handleScroll} className="flex-1 overflow-y-auto px-4 sm:px-8 py-5 flex flex-col gap-5">
        {messages.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-center px-4 py-6">
            <p className="m-0 font-display text-[22px] text-ivory-50">À votre disposition, Monsieur.</p>
            <p className="m-0 mt-1 mb-6 text-[13px] text-ivory-500">Parlez, ou écrivez ci-dessous.</p>
            <div className="flex flex-wrap items-center justify-center gap-2 max-w-lg">
              {SUGGESTIONS.map((item) => (
                <button key={item} type="button" onClick={() => onSelectSuggestion?.(item)} className="pill h-9 px-3.5 cursor-pointer hover:border-brass-500 hover:text-ivory-100 transition-colors">
                  {item}
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((msg) => {
            if (msg.role === 'notice') {
              return (
                <div key={msg.id} className="flex justify-center">
                  <span className="text-[12px] text-ivory-700 italic">{msg.text}</span>
                </div>
              );
            }
            const isUser = msg.role === 'user';
            return (
              <div key={msg.id} className={`group flex flex-col gap-1 ${isUser ? 'items-end' : 'items-start'}`}>
                <div className="flex items-baseline gap-2 text-[12px] text-ivory-500 px-1">
                  <span className={isUser ? '' : 'font-display text-[13px] text-brass-300'}>{isUser ? 'Vous' : 'Nestor'}</span>
                  {isUser && !msg.isFinal && <span className="text-listen-300">transcription…</span>}
                  {!isUser && msg.isStreaming && <span className="text-brass-300">écrit…</span>}
                  <button
                    type="button"
                    onClick={() => copyToClipboard(msg.text, msg.id)}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100 text-ivory-700 hover:text-ivory-300 transition-opacity"
                    title="Copier le texte"
                    aria-label="Copier le texte"
                  >
                    {copiedId === msg.id ? <Check className="w-3 h-3 text-ok-400" /> : <Copy className="w-3 h-3" />}
                  </button>
                </div>
                <div
                  className={`max-w-[92%] sm:max-w-[78%] rounded-2xl px-4 py-2.5 border select-text ${
                    isUser ? 'bg-ink-800/80 border-ink-700 text-ivory-100 rounded-tr-md' : 'bg-ink-900/70 border-ink-800 text-ivory-100 rounded-tl-md'
                  }`}
                >
                  <div className={`prose prose-invert max-w-none leading-relaxed text-[15px] ${isUser ? '' : 'font-display text-[17px]'}`}>
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>{msg.text || (msg.isStreaming ? '…' : '')}</ReactMarkdown>
                  </div>
                </div>
              </div>
            );
          })
        )}

        {status === 'thinking' && (
          <div className="flex items-center gap-2 text-[13px] text-think-300 px-1">
            <span className="w-2 h-2 rounded-full bg-think-400 nestor-breathe" />
            <span>Nestor réfléchit…</span>
          </div>
        )}
      </div>

      {userHasScrolled && (
        <button type="button" onClick={scrollToBottom} className="btn btn-icon absolute bottom-4 right-6 rounded-full" title="Revenir en bas" aria-label="Revenir en bas">
          <ArrowDown className="w-4 h-4" />
        </button>
      )}
    </div>
  );
};
