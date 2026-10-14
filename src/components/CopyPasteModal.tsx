import React, { useState, useEffect } from "react";
import { Folder, Zap, RefreshCw, Settings, X, ArrowDown } from "lucide-react";
import { translations, Language } from "../i18n";

interface CopyPasteOption {
  id: string;
  title: string;
  subtitle: string;
  icon: React.ReactNode;
  action: () => void;
}

interface CopyPasteModalProps {
  isOpen: boolean;
  source: string;
  destination: string;
  onClose: () => void;
  onSelectExplorer: () => void;
  onSelectDeepNodeCopy: (policy?: string) => void;
  onSelectDeepNodeMove: () => void;
  lang?: Language;
}

export const CopyPasteModal: React.FC<CopyPasteModalProps> = ({
  isOpen,
  source,
  destination,
  onClose,
  onSelectExplorer,
  onSelectDeepNodeCopy,
  onSelectDeepNodeMove,
  lang = "tr",
}) => {
  const [selectedIndex, setSelectedIndex] = useState(1); // Default to DeepNode Copy
  const t = translations[lang] || translations.tr;

  const options: CopyPasteOption[] = [
    {
      id: "explorer",
      title: "Explorer",
      subtitle: "Windows Explorer",
      icon: <Folder className="w-6 h-6 text-amber-400 flex-shrink-0" />,
      action: onSelectExplorer,
    },
    {
      id: "deepnode-copy",
      title: `${t.appTitle} - ${t.copy}`,
      subtitle: `${t.copy} (${t.directIo} & xxHash3)`,
      icon: <Zap className="w-6 h-6 text-indigo-400 flex-shrink-0" />,
      action: () => onSelectDeepNodeCopy("auto_rename"),
    },
    {
      id: "deepnode-replace-older",
      title: `${t.appTitle} - ${t.conflictOlder}`,
      subtitle: `${t.conflictOlder} (/OverwriteOlder)`,
      icon: <RefreshCw className="w-6 h-6 text-sky-400 flex-shrink-0" />,
      action: () => onSelectDeepNodeCopy("older"),
    },
    {
      id: "deepnode-move",
      title: `${t.appTitle} - ${t.move}`,
      subtitle: `${t.move} (${t.destination})`,
      icon: <Zap className="w-6 h-6 text-emerald-400 flex-shrink-0" />,
      action: onSelectDeepNodeMove,
    },
  ];

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % options.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + options.length) % options.length);
      } else if (e.key === "Enter") {
        e.preventDefault();
        options[selectedIndex]?.action();
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, selectedIndex, options]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm z-50 flex items-center justify-center p-4 select-none">
      <div className="w-full max-w-lg bg-neutral-900 border border-neutral-700/80 rounded-xl shadow-2xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-150">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 bg-neutral-950/80 border-b border-neutral-800">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-neutral-100">Copy &amp; Paste</h3>
          </div>
          <div className="flex items-center gap-2 text-neutral-400">
            <button
              onClick={() => {}}
              title="Settings"
              className="p-1 hover:text-neutral-200 hover:bg-neutral-800 rounded transition"
            >
              <Settings className="w-4 h-4" />
            </button>
            <button
              onClick={onClose}
              title="Close (Esc)"
              className="p-1 hover:text-rose-400 hover:bg-neutral-800 rounded transition"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        {/* Source & Destination details */}
        <div className="px-4 py-2.5 bg-neutral-950/40 border-b border-neutral-800/80 text-xs font-mono text-neutral-400 flex flex-col gap-1">
          <div className="flex items-center gap-2 truncate">
            <span className="text-neutral-500 flex-shrink-0">...</span>
            <span className="truncate text-neutral-300" title={source}>
              {source || t.selectedItems}
            </span>
          </div>
          <div className="flex items-center gap-2 text-neutral-500 pl-2">
            <ArrowDown className="w-3.5 h-3.5 text-neutral-500" />
            <span className="truncate text-neutral-300" title={destination}>
              {destination || t.destination}
            </span>
          </div>
        </div>

        {/* Options list */}
        <div className="p-2 flex flex-col gap-1 bg-neutral-900 max-h-80 overflow-y-auto">
          {options.map((opt, idx) => {
            const isSelected = selectedIndex === idx;
            return (
              <div
                key={opt.id}
                onClick={opt.action}
                onMouseEnter={() => setSelectedIndex(idx)}
                className={`flex items-center gap-3.5 px-3.5 py-2.5 rounded-lg cursor-pointer transition text-left ${
                  isSelected
                    ? "bg-indigo-600/90 text-white shadow-md shadow-indigo-950"
                    : "hover:bg-neutral-800/60 text-neutral-200"
                }`}
              >
                {opt.icon}
                <div className="flex flex-col min-w-0 flex-1">
                  <div className="text-xs font-medium tracking-wide truncate">
                    {opt.title}
                  </div>
                  <div
                    className={`text-[11px] truncate font-mono ${
                      isSelected ? "text-indigo-100" : "text-neutral-400"
                    }`}
                  >
                    {opt.subtitle}
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        {/* Footer tip */}
        <div className="px-4 py-2 bg-neutral-950/70 border-t border-neutral-800/80 flex items-center justify-between text-[11px] text-neutral-500 font-mono">
          <span>Enter: {t.copy} | Esc: {t.cancel}</span>
          <span className="text-indigo-400">DeepNode v0.1.0</span>
        </div>
      </div>
    </div>
  );
};
