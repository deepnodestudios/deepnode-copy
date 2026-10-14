import React, { useState } from "react";
import {
  AlertTriangle,
  Clock,
  HardDrive,
  RotateCw,
  ArrowRightLeft,
  FastForward,
  Split,
  X,
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

  const locale = lang === "tr" ? "tr-TR" : lang === "de" ? "de-DE" : lang === "fr" ? "fr-FR" : lang === "ru" ? "ru-RU" : "en-US";
  const srcDate = conflict.src_modified > 0
    ? new Date(conflict.src_modified * 1000).toLocaleString(locale)
    : "-";
  const dstDate = conflict.dst_modified > 0
    ? new Date(conflict.dst_modified * 1000).toLocaleString(locale)
    : "-";

  const isSourceNewer = conflict.src_modified >= conflict.dst_modified;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 animate-in fade-in duration-150">
      <div className="bg-[#090d16] border border-slate-700/80 rounded-2xl max-w-xl w-full shadow-2xl p-6 text-slate-100 flex flex-col gap-4 font-sans select-none">
        {/* Header */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <AlertTriangle className="w-5 h-5 text-amber-400 flex-shrink-0" />
            <h3 className="text-sm font-bold text-white tracking-wide">
              {lang === "tr" ? "Çakışma Tespit Edildi — Hedef Dosya Zaten Mevcut" : "Conflict Detected — Target File Already Exists"}
            </h3>
          </div>
          <button
            onClick={() => onResolve("skip", applyToAll)}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/60 transition cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Subtitle */}
        <p className="text-xs text-slate-400 -mt-1 leading-relaxed">
          {lang === "tr"
            ? "Hedef konumda aynı ada sahip bir dosya zaten bulunuyor. DeepNode Copy bu dosyayı nasıl işlesin:"
            : "A file with the same name already exists at the destination. Select how DeepNode Copy should handle this file:"}
        </p>

        {/* Comparison Cards */}
        <div className="grid grid-cols-2 gap-3.5 my-1">
          {/* Source Card */}
          <div className="p-4 rounded-xl border border-emerald-500/50 bg-emerald-950/20 flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-bold uppercase tracking-wider text-emerald-400">
                {lang === "tr" ? "KAYNAK DOSYA" : "SOURCE FILE"}
              </span>
              <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-emerald-900/60 border border-emerald-500/50 text-emerald-300">
                {isSourceNewer ? (lang === "tr" ? "DAHA YENİ" : "NEWER") : (lang === "tr" ? "DAHA ESKİ" : "OLDER")}
              </span>
            </div>
            <div className="text-sm font-bold text-white truncate" title={conflict.filename}>
              {conflict.filename}
            </div>
            <div className="flex items-center gap-2 text-xs text-slate-300 font-mono">
              <HardDrive className="w-3.5 h-3.5 text-slate-400" />
              <span>{formatBytes(conflict.src_size)}</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
              <Clock className="w-3.5 h-3.5 text-slate-500" />
              <span className="truncate">{srcDate}</span>
            </div>
          </div>

          {/* Target Card */}
          <div className="p-4 rounded-xl border border-slate-700 bg-slate-900/40 flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-bold uppercase tracking-wider text-slate-400">
                {lang === "tr" ? "HEDEF DOSYA (MEVCUT)" : "TARGET FILE (EXISTING)"}
              </span>
              <span className="text-[10px] font-bold px-2 py-0.5 rounded-full bg-slate-800 border border-slate-600 text-slate-300">
                {!isSourceNewer ? (lang === "tr" ? "DAHA YENİ" : "NEWER") : (lang === "tr" ? "DAHA ESKİ" : "OLDER")}
              </span>
            </div>
            <div className="text-sm font-bold text-white truncate" title={conflict.filename}>
              {conflict.filename}
            </div>
            <div className="flex items-center gap-2 text-xs text-slate-300 font-mono">
              <HardDrive className="w-3.5 h-3.5 text-slate-400" />
              <span>{formatBytes(conflict.dst_size)}</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
              <Clock className="w-3.5 h-3.5 text-slate-500" />
              <span className="truncate">{dstDate}</span>
            </div>
          </div>
        </div>

        {/* 2x2 Action Buttons */}
        <div className="grid grid-cols-2 gap-3 text-xs font-semibold">
          <button
            onClick={() => onResolve("older", applyToAll)}
            className="flex items-center justify-center gap-2.5 py-3 px-4 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold transition shadow-lg shadow-blue-600/25 cursor-pointer"
          >
            <RotateCw className="w-4 h-4" />
            <span>{lang === "tr" ? "Yalnızca Eskileri Değiştir (Önerilen)" : "Overwrite Older (Recommended)"}</span>
          </button>

          <button
            onClick={() => onResolve("overwrite", applyToAll)}
            className="flex items-center justify-center gap-2.5 py-3 px-4 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-slate-700 text-slate-200 transition cursor-pointer"
          >
            <ArrowRightLeft className="w-4 h-4" />
            <span>{lang === "tr" ? "Tümünün Üzerine Yaz" : "Overwrite All"}</span>
          </button>

          <button
            onClick={() => onResolve("rename", applyToAll)}
            className="flex items-center justify-center gap-2.5 py-3 px-4 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-slate-700 text-slate-200 transition cursor-pointer"
          >
            <Split className="w-4 h-4" />
            <span>{lang === "tr" ? "Yeniden Adlandır (İkisini de Tut)" : "Auto-Rename (Keep Both)"}</span>
          </button>

          <button
            onClick={() => onResolve("skip", applyToAll)}
            className="flex items-center justify-center gap-2.5 py-3 px-4 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-slate-700 text-slate-200 transition cursor-pointer"
          >
            <FastForward className="w-4 h-4" />
            <span>{lang === "tr" ? "Bu Dosyayı Atla" : "Skip This File"}</span>
          </button>
        </div>

        {/* Apply to all checkbox */}
        <label className="flex items-center gap-2.5 pt-1 text-xs text-slate-300 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={applyToAll}
            onChange={(e) => setApplyToAll(e.target.checked)}
            className="w-4 h-4 rounded bg-[#121827] border-slate-700 text-blue-600 focus:ring-0 cursor-pointer"
          />
          <span>{lang === "tr" ? "Bu oturumdaki kalan tüm çakışmalara uygula" : "Apply to all remaining conflicts in this session"}</span>
        </label>
      </div>
    </div>
  );
};
