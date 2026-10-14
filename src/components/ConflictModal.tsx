import React, { useState } from "react";
import {
  AlertTriangle,
  Clock,
  HardDrive,
  Check,
  RotateCcw,
  FastForward,
  Copy,
} from "lucide-react";
import { ConflictInfo } from "../types";
import { formatBytes } from "./CompactView";
import { Language } from "../i18n";

interface ConflictModalProps {
  conflict: ConflictInfo | null;
  onResolve: (action: "overwrite" | "older" | "rename" | "skip", applyToAll: boolean) => void;
  lang: Language;
}

export const ConflictModal: React.FC<ConflictModalProps> = ({
  conflict,
  onResolve,
  lang,
}) => {
  const [applyToAll, setApplyToAll] = useState(false);

  if (!conflict) return null;

  const srcDate = conflict.src_modified > 0
    ? new Date(conflict.src_modified * 1000).toLocaleString(lang === "tr" ? "tr-TR" : "en-US")
    : "Bilinmiyor";
  const dstDate = conflict.dst_modified > 0
    ? new Date(conflict.dst_modified * 1000).toLocaleString(lang === "tr" ? "tr-TR" : "en-US")
    : "Bilinmiyor";

  const isSourceNewer = conflict.src_modified > conflict.dst_modified;
  const isTargetNewer = conflict.dst_modified > conflict.src_modified;
  const isSameSize = conflict.src_size === conflict.dst_size;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4 animate-in fade-in duration-150">
      <div className="bg-neutral-900 border border-neutral-700/80 rounded-xl max-w-lg w-full shadow-2xl overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center gap-2.5 px-4 py-3 bg-neutral-950/80 border-b border-neutral-800">
          <AlertTriangle className="w-5 h-5 text-amber-400 flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <h3 className="text-xs font-bold text-neutral-100 uppercase tracking-wide">
              {lang === "tr" ? "Dosya Çakışması Algılandı" : "File Conflict Detected"}
            </h3>
            <p className="text-[11px] text-neutral-400 font-mono truncate" title={conflict.filename}>
              {conflict.filename}
            </p>
          </div>
        </div>

        {/* Comparison Cards */}
        <div className="p-4 flex flex-col gap-3 text-xs font-sans">
          <p className="text-neutral-300 text-[11px]">
            {lang === "tr"
              ? "Hedef konumda aynı ada sahip bir dosya zaten mevcut. Ne yapmak istersiniz?"
              : "A file with the same name already exists in the destination. What would you like to do?"}
          </p>

          <div className="grid grid-cols-2 gap-2.5">
            {/* Source Card */}
            <div className={`p-3 rounded-lg border flex flex-col gap-1.5 ${
              isSourceNewer
                ? "bg-indigo-950/30 border-indigo-700/60"
                : "bg-neutral-950/40 border-neutral-800"
            }`}>
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider text-indigo-400">
                  {lang === "tr" ? "Kaynak (Yeni)" : "Source (New)"}
                </span>
                {isSourceNewer && (
                  <span className="text-[9px] px-1 py-0.2 rounded bg-emerald-950 border border-emerald-800 text-emerald-300 font-semibold">
                    {lang === "tr" ? "Daha Yeni" : "Newer"}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-1.5 text-neutral-200 font-mono font-bold text-xs">
                <HardDrive className="w-3.5 h-3.5 text-neutral-400" />
                <span>{formatBytes(conflict.src_size)}</span>
              </div>
              <div className="flex items-center gap-1.5 text-[10px] text-neutral-400 font-mono">
                <Clock className="w-3 h-3 text-neutral-500" />
                <span className="truncate">{srcDate}</span>
              </div>
            </div>

            {/* Target Card */}
            <div className={`p-3 rounded-lg border flex flex-col gap-1.5 ${
              isTargetNewer
                ? "bg-amber-950/30 border-amber-700/60"
                : "bg-neutral-950/40 border-neutral-800"
            }`}>
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-wider text-neutral-400">
                  {lang === "tr" ? "Hedef (Mevcut)" : "Target (Existing)"}
                </span>
                {isTargetNewer && (
                  <span className="text-[9px] px-1 py-0.2 rounded bg-amber-950 border border-amber-800 text-amber-300 font-semibold">
                    {lang === "tr" ? "Daha Yeni" : "Newer"}
                  </span>
                )}
                {isSameSize && (
                  <span className="text-[9px] px-1 py-0.2 rounded bg-neutral-800 text-neutral-300 font-semibold">
                    {lang === "tr" ? "Aynı Boyut" : "Same Size"}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-1.5 text-neutral-200 font-mono font-bold text-xs">
                <HardDrive className="w-3.5 h-3.5 text-neutral-400" />
                <span>{formatBytes(conflict.dst_size)}</span>
              </div>
              <div className="flex items-center gap-1.5 text-[10px] text-neutral-400 font-mono">
                <Clock className="w-3 h-3 text-neutral-500" />
                <span className="truncate">{dstDate}</span>
              </div>
            </div>
          </div>

          {/* Apply to All Checkbox */}
          <label className="flex items-center gap-2 p-2 rounded bg-neutral-950/50 border border-neutral-800 text-xs text-neutral-300 cursor-pointer hover:bg-neutral-950 transition select-none">
            <input
              type="checkbox"
              checked={applyToAll}
              onChange={(e) => setApplyToAll(e.target.checked)}
              className="rounded bg-neutral-800 border-neutral-700 text-indigo-500 focus:ring-0 cursor-pointer"
            />
            <span>
              {lang === "tr"
                ? "Tüm sonraki çakışmalar için bu eylemi uygula"
                : "Apply this action to all upcoming conflicts"}
            </span>
          </label>
        </div>

        {/* Action Buttons */}
        <div className="p-3 bg-neutral-950/80 border-t border-neutral-800 grid grid-cols-2 gap-2 text-xs">
          <button
            onClick={() => onResolve("overwrite", applyToAll)}
            className="flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg bg-rose-700 hover:bg-rose-600 text-white font-semibold transition"
          >
            <Check className="w-3.5 h-3.5" />
            <span>{lang === "tr" ? "Üzerine Yaz (Overwrite)" : "Overwrite"}</span>
          </button>

          <button
            onClick={() => onResolve("older", applyToAll)}
            className="flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg bg-indigo-700 hover:bg-indigo-600 text-white font-semibold transition"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>{lang === "tr" ? "Eskileri Değiştir (Older)" : "Replace Older"}</span>
          </button>

          <button
            onClick={() => onResolve("rename", applyToAll)}
            className="flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 font-medium transition border border-neutral-700"
          >
            <Copy className="w-3.5 h-3.5" />
            <span>{lang === "tr" ? "Yeniden Adlandır (Rename)" : "Auto Rename"}</span>
          </button>

          <button
            onClick={() => onResolve("skip", applyToAll)}
            className="flex items-center justify-center gap-1.5 py-2 px-3 rounded-lg bg-neutral-800 hover:bg-amber-900/80 text-neutral-300 hover:text-white font-medium transition border border-neutral-700"
          >
            <FastForward className="w-3.5 h-3.5" />
            <span>{lang === "tr" ? "Bu Dosyayı Atla (Skip)" : "Skip"}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
