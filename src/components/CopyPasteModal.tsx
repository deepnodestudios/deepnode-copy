import React, { useState, useEffect } from "react";
import { Folder, Zap, RefreshCw, X, ArrowDown } from "lucide-react";
import { translations, Language } from "../i18n";
import packageJson from "../../package.json";

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
  const [isSubmitted, setIsSubmitted] = useState(false);
  const t = translations[lang] || translations.tr;

  const handleSelectOption = (action: () => void) => {
    if (isSubmitted) return;
    setIsSubmitted(true);
    action();
  };

  const options: (CopyPasteOption & { shortcutKey: string })[] = [
    {
      id: "explorer",
      title: "Explorer",
      subtitle: "Windows Explorer",
      icon: <Folder className="w-6 h-6 text-amber-400 flex-shrink-0" />,
      action: onSelectExplorer,
      shortcutKey: "1",
    },
    {
      id: "deepnode-copy",
      title: `${t.appTitle} - ${t.copy}`,
      subtitle: `${t.copy} (${t.directIo} & xxHash3)`,
      icon: <Zap className="w-6 h-6 text-indigo-400 flex-shrink-0" />,
      action: () => onSelectDeepNodeCopy("auto_rename"),
      shortcutKey: "2",
    },
    {
      id: "deepnode-replace-older",
      title: `${t.appTitle} - ${t.conflictOlder}`,
      subtitle: `${t.conflictOlder} (/OverwriteOlder)`,
      icon: <RefreshCw className="w-6 h-6 text-sky-400 flex-shrink-0" />,
      action: () => onSelectDeepNodeCopy("older"),
      shortcutKey: "3",
    },
    {
      id: "deepnode-move",
      title: `${t.appTitle} - ${t.move}`,
      subtitle: `${t.move} (${t.destination})`,
      icon: <Zap className="w-6 h-6 text-emerald-400 flex-shrink-0" />,
      action: onSelectDeepNodeMove,
      shortcutKey: "4",
    },
  ];

  useEffect(() => {
    if (isOpen) {
      setIsSubmitted(false);
      try {
        window.focus();
      } catch (e) {}
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      const key = e.key.toLowerCase();
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % options.length);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + options.length) % options.length);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        const selected = options[selectedIndex];
        if (selected) {
          handleSelectOption(selected.action);
        }
      } else if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      } else if (key === "1" || key === "e") {
        e.preventDefault();
        handleSelectOption(options[0].action);
      } else if (key === "2" || key === "c" || key === "k") {
        e.preventDefault();
        handleSelectOption(options[1].action);
      } else if (key === "3") {
        e.preventDefault();
        handleSelectOption(options[2].action);
      } else if (key === "4" || key === "m" || key === "t") {
        e.preventDefault();
        handleSelectOption(options[3].action);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, selectedIndex, options, isSubmitted]);

  if (!isOpen) return null;

  return (
    <div
      className="w-full h-full bg-neutral-900 border border-neutral-700/90 flex flex-col select-none overflow-hidden"
      data-tauri-drag-region="false"
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-3.5 py-2 bg-neutral-950/80 border-b border-neutral-800 cursor-move"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-2" data-tauri-drag-region>
          <h3 className="text-xs font-semibold text-neutral-100" data-tauri-drag-region>
            {t.appTitle} - Copy &amp; Paste
          </h3>
        </div>
        <div className="flex items-center gap-1.5 text-neutral-400" data-tauri-drag-region="false">
          <button
            onPointerDown={(e) => {
              e.stopPropagation();
              onClose();
            }}
            onClick={(e) => {
              e.stopPropagation();
              onClose();
            }}
            title="Close (Esc)"
            className="p-1 hover:text-rose-400 hover:bg-neutral-800 rounded transition cursor-pointer"
            data-tauri-drag-region="false"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Source & Destination details */}
      <div
        className="px-3 py-1.5 bg-neutral-950/50 border-b border-neutral-800/80 text-[11px] font-mono text-neutral-400 flex flex-col gap-0.5 cursor-move"
        data-tauri-drag-region
      >
        <div className="flex items-center gap-1.5 truncate" data-tauri-drag-region>
          <span className="text-neutral-500 flex-shrink-0">...</span>
          <span className="truncate text-neutral-300" title={source}>
            {source || t.selectedItems}
          </span>
        </div>
        <div className="flex items-center gap-1.5 text-neutral-500 pl-2" data-tauri-drag-region>
          <ArrowDown className="w-3 h-3 text-neutral-500" />
          <span className="truncate text-neutral-300" title={destination}>
            {destination || t.destination}
          </span>
        </div>
      </div>

      {/* Options list */}
      <div className="p-1.5 flex flex-col gap-1 bg-neutral-900 flex-1 overflow-y-auto" data-tauri-drag-region="false">
        {options.map((opt, idx) => {
          const isSelected = selectedIndex === idx;
          return (
            <div
              key={opt.id}
              data-tauri-drag-region="false"
              onPointerDown={(e) => {
                if (e.button === 0) {
                  e.preventDefault();
                  e.stopPropagation();
                  handleSelectOption(opt.action);
                }
              }}
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                handleSelectOption(opt.action);
              }}
              onMouseEnter={() => !isSubmitted && setSelectedIndex(idx)}
              className={`flex items-center justify-between gap-2.5 px-3 py-1.5 rounded-lg cursor-pointer transition text-left select-none ${
                isSubmitted ? "opacity-60 pointer-events-none cursor-not-allowed " : ""
              }${
                isSelected
                  ? "bg-indigo-600/90 text-white shadow-md shadow-indigo-950"
                  : "hover:bg-neutral-800/60 text-neutral-200"
              }`}
            >
              <div className="flex items-center gap-2.5 min-w-0 flex-1">
                {opt.icon}
                <div className="flex flex-col min-w-0 flex-1">
                  <div className="text-xs font-medium tracking-wide truncate">
                    {opt.title}
                  </div>
                  <div
                    className={`text-[10px] truncate font-mono ${
                      isSelected ? "text-indigo-100" : "text-neutral-400"
                    }`}
                  >
                    {opt.subtitle}
                  </div>
                </div>
              </div>
              <span
                className={`text-[10px] font-mono px-1.5 py-0.5 rounded border transition flex-shrink-0 ${
                  isSelected
                    ? "bg-indigo-700/80 border-indigo-400 text-white font-bold"
                    : "bg-neutral-800/80 border-neutral-700 text-neutral-400"
                }`}
              >
                {idx === 1 ? "Enter" : opt.shortcutKey}
              </span>
            </div>
          );
        })}
      </div>

      {/* Footer tip */}
      <div
        className="px-3 py-1.5 bg-neutral-950/70 border-t border-neutral-800/80 flex items-center justify-between text-[10px] text-neutral-500 font-mono cursor-move"
        data-tauri-drag-region
      >
        <span>1-4 / Enter: {t.copy} | Esc: {t.cancel}</span>
        <span className="text-indigo-400">DeepNode v{packageJson.version}</span>
      </div>
    </div>
  );
};
