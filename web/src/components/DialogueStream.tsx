import React, { useEffect, useRef, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { User, Sparkles, Copy, Check, Terminal, ArrowDown } from 'lucide-react';
import type { MessageItem, DaemonStatus } from '../types';

interface DialogueStreamProps {
  messages: MessageItem[];
  status: DaemonStatus;
  onSelectSuggestion?: (text: string) => void;
}

export const DialogueStream: React.FC<DialogueStreamProps> = ({
  messages,
  status,
  onSelectSuggestion,
}) => {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [userHasScrolled, setUserHasScrolled] = useState(false);

  // Auto-scroll to bottom on new message unless user scrolled up
  useEffect(() => {
    if (!userHasScrolled && scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [messages, userHasScrolled]);

  const handleScroll = () => {
    if (!scrollContainerRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollContainerRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setUserHasScrolled(!isAtBottom);
  };

  const scrollToBottom = () => {
    if (scrollContainerRef.current) {
      scrollContainerRef.current.scrollTo({
        top: scrollContainerRef.current.scrollHeight,
        behavior: 'smooth',
      });
      setUserHasScrolled(false);
    }
  };

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const suggestions = [
    "Explique-moi l'architecture de Nestor",
    "Lance une analyse du code avec `claude`",
    "Crée un test unitaire pour le protocole WebSocket",
    "Quel est l'état du daemon local ?",
  ];

  return (
    <div className="relative flex flex-col h-full overflow-hidden">
      {/* Messages Scroll Area */}
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-3 sm:px-4 py-4 sm:py-6 space-y-4 sm:space-y-5"
      >
        {messages.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-center px-4 py-8">
            <div className="w-14 h-14 rounded-2xl bg-cyan-500/10 border border-cyan-500/30 flex items-center justify-center text-cyan-400 mb-4 shadow-[0_0_20px_rgba(6,182,212,0.15)]">
              <Sparkles className="w-7 h-7" />
            </div>
            <h3 className="text-lg font-medium text-slate-100 mb-1 tracking-wide">
              Nestor à votre service
            </h3>
            <p className="text-xs text-slate-400 max-w-sm mb-6 leading-relaxed">
              Parlez dans votre micro pour interagir vocalement ou tapez votre commande au clavier.
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 w-full max-w-md">
              {suggestions.map((item, idx) => (
                <button
                  key={idx}
                  onClick={() => onSelectSuggestion && onSelectSuggestion(item)}
                  className="text-left text-xs bg-slate-900/60 hover:bg-slate-800/80 border border-slate-800 hover:border-cyan-500/40 text-slate-300 hover:text-cyan-300 p-2.5 rounded-lg transition-all duration-150 backdrop-blur-sm group flex items-start space-x-2"
                >
                  <Terminal className="w-3.5 h-3.5 text-cyan-400/60 group-hover:text-cyan-400 mt-0.5 shrink-0" />
                  <span className="line-clamp-2">{item}</span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          messages.map((msg) => {
            const isUser = msg.role === 'user';
            return (
              <div
                key={msg.id}
                className={`flex gap-2 sm:gap-3 text-sm ${
                  isUser ? 'justify-end' : 'justify-start'
                }`}
              >
                {!isUser && (
                  <div className="w-7 h-7 sm:w-8 sm:h-8 rounded-full bg-cyan-950/80 border border-cyan-500/40 flex items-center justify-center text-cyan-400 shrink-0 mt-0.5 shadow-[0_0_10px_rgba(6,182,212,0.2)]">
                    <Sparkles className="w-3.5 h-3.5 sm:w-4 sm:h-4" />
                  </div>
                )}

                <div
                  className={`relative max-w-[92%] sm:max-w-[82%] rounded-2xl px-3.5 sm:px-4 py-2.5 sm:py-3 border transition-all ${
                    isUser
                      ? 'bg-gradient-to-r from-cyan-900/40 to-blue-900/40 border-cyan-500/30 text-slate-100 rounded-tr-none'
                      : 'bg-slate-900/80 border-slate-800 text-slate-200 rounded-tl-none shadow-lg'
                  }`}
                >
                  {/* Top Bar for message */}
                  <div className="flex items-center justify-between gap-4 mb-1.5 pb-1 border-b border-white/5">
                    <span className="text-[10px] font-mono tracking-wider uppercase text-slate-400">
                      {isUser ? 'Vous (Vocal / Clavier)' : 'Nestor'}
                    </span>
                    <div className="flex items-center gap-2">
                      {isUser && !msg.isFinal && (
                        <span className="inline-flex items-center px-1.5 py-0.2 rounded text-[9px] bg-cyan-500/20 text-cyan-300 font-mono animate-pulse">
                          Transcription en direct...
                        </span>
                      )}
                      {!isUser && msg.isStreaming && (
                        <span className="inline-flex items-center px-1.5 py-0.2 rounded text-[9px] bg-amber-500/20 text-amber-300 font-mono animate-pulse">
                          En cours de frappe...
                        </span>
                      )}
                      <button
                        onClick={() => copyToClipboard(msg.text, msg.id)}
                        className="text-slate-500 hover:text-slate-300 transition-colors p-0.5"
                        title="Copier le texte"
                      >
                        {copiedId === msg.id ? (
                          <Check className="w-3 h-3 text-emerald-400" />
                        ) : (
                          <Copy className="w-3 h-3" />
                        )}
                      </button>
                    </div>
                  </div>

                  {/* Markdown or plain text content */}
                  <div className="prose prose-invert prose-xs max-w-none text-slate-200 leading-relaxed font-sans select-text">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {msg.text || (msg.isStreaming ? '...' : '')}
                    </ReactMarkdown>
                  </div>
                </div>

                {isUser && (
                  <div className="w-8 h-8 rounded-full bg-slate-800 border border-slate-700 flex items-center justify-center text-slate-300 shrink-0 mt-0.5">
                    <User className="w-4 h-4" />
                  </div>
                )}
              </div>
            );
          })
        )}

        {/* Status indicator when thinking */}
        {status === 'thinking' && (
          <div className="flex items-center gap-2 text-xs text-purple-300 bg-purple-950/40 border border-purple-500/30 rounded-xl px-4 py-2 w-fit animate-pulse">
            <span className="w-2 h-2 rounded-full bg-purple-400 animate-ping" />
            <span className="font-mono">Nestor réfléchit (inférence Claude Code)...</span>
          </div>
        )}
      </div>

      {/* Floating scroll down button */}
      {userHasScrolled && (
        <button
          onClick={scrollToBottom}
          className="absolute bottom-4 right-6 bg-slate-800/90 hover:bg-slate-700 border border-cyan-500/30 text-cyan-400 p-2 rounded-full shadow-lg transition-all"
          title="Défiler vers le bas"
        >
          <ArrowDown className="w-4 h-4" />
        </button>
      )}
    </div>
  );
};
