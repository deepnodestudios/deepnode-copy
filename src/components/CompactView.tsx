import React from "react";
import { Play, Pause, X, ChevronDown, ShieldCheck, Pin, Info, Zap } from "lucide-react";
import { TransferStats, TransferStatus } from "../types";
import { translations, Language } from "../i18n";
import { SpeedChart } from "./SpeedChart";

export const formatBytes = (bytes: number): string => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
};

export const formatEta = (seconds: number, lang: Language = "tr"): string => {
  const dict = translations[lang] || translations.en;
  if (seconds <= 0 || !isFinite(seconds)) return `0 ${dict.secShort}`;
  if (seconds < 60) return `${Math.round(seconds)} ${dict.secShort}`;
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  if (m < 60) return `${m} ${dict.minShort} ${s} ${dict.secShort}`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return `${h} ${dict.hourShort} ${remM} ${dict.minShort}`;
};

interface CompactViewProps {
  stats: TransferStats;
  source?: string;
  destination?: string;
  status: TransferStatus;
  speedHistory: number[];
  alwaysOnTop: boolean;
  onToggleAlwaysOnTop: () => void;
  onPauseToggle: () => void;
  onCancel: () => void;
  onExpand: () => void;
  onOpenVerify: () => void;
  onOpenAbout: () => void;
  onCloseApp?: () => void;
  lang: Language;
}

export const CompactView: React.FC<CompactViewProps> = ({
  stats,
  source,
  destination,
  status,
  speedHistory,
  alwaysOnTop,
  onToggleAlwaysOnTop,
  onPauseToggle,
  onCancel,
  onExpand,
  onOpenVerify,
  onOpenAbout,
  onCloseApp,
  lang,
}) => {
  const t = translations[lang] || translations.tr;
  const speedMb = (stats.current_speed_bps / 1_048_576).toFixed(1);
  const avgSpeedMb = (stats.average_speed_bps / 1_048_576).toFixed(1);
  const transferredStr = formatBytes(stats.transferred_bytes);
  const totalStr = formatBytes(stats.total_bytes);

  const fileTransferredStr = formatBytes(stats.current_file_transferred);
  const fileSizeStr = formatBytes(stats.current_file_size);
  const filePercentage =
    stats.current_file_size > 0
      ? Math.min(100, Math.max(0, (stats.current_file_transferred / stats.current_file_size) * 100))
      : stats.overall_percentage;

  const etaStr = formatEta(stats.eta_seconds, lang);

  return (
    <div className="w-full h-full flex flex-col justify-between p-3.5 bg-[#0b0f19] border border-dark-border rounded-xl shadow-2xl select-none font-sans text-neutral-100 overflow-hidden">
      {/* ── Title Bar ── */}
      <div className="flex items-center justify-between pb-2 border-b border-dark-border">
        <div className="flex items-center gap-2 min-w-0">
          <img src="/branding/app-icon.png" alt="icon" className="w-4 h-4 rounded" />
          <span className="text-xs font-extrabold text-neutral-100 tracking-tight">DeepNode Copy</span>
          <span className="inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full bg-emerald-950/70 text-emerald-300 border border-emerald-800/80 font-bold font-mono">
            <Zap className="w-2.5 h-2.5 text-emerald-400" />
            <span>Direct I/O</span>
          </span>

          {status === "transferring" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-blue-950 text-sky-300 border border-blue-800 font-semibold font-mono">
              {stats.overall_percentage.toFixed(0)}%
            </span>
          )}
          {status === "paused" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-amber-950 text-amber-300 border border-amber-800 font-semibold">
              {t.statusPaused}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1.5">
          <button
            onClick={onToggleAlwaysOnTop}
            title={t.alwaysOnTop}
            className={`p-1 rounded-md text-xs transition border ${
              alwaysOnTop
                ? "bg-blue-600 border-blue-500 text-white"
                : "bg-[#111624] hover:bg-[#161e31] border-dark-border text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Pin className="w-3 h-3" />
          </button>
          <button
            onClick={onOpenVerify}
            title={t.clipboardTest}
            className="p-1 rounded-md bg-[#111624] hover:bg-[#161e31] border border-dark-border text-neutral-400 hover:text-sky-400 transition cursor-pointer"
          >
            <ShieldCheck className="w-3 h-3" />
          </button>
          <button
            onClick={onOpenAbout}
            title={t.aboutTitle}
            className="p-1 rounded-md bg-[#111624] hover:bg-[#161e31] border border-dark-border text-neutral-400 hover:text-sky-400 transition cursor-pointer"
          >
            <Info className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* ── File Row with Giant Speed Tag ── */}
      <div className="flex items-center justify-between gap-3 my-1">
        <div className="flex flex-col min-w-0 flex-1">
          <span className="text-xs font-bold text-neutral-100 truncate" title={stats.current_file_name}>
            {stats.current_file_name || t.statusReady}
          </span>
          {(source || destination) && (
            <span className="text-[10px] text-neutral-500 font-mono truncate" title={`${source} → ${destination}`}>
              {source ? source.split("\\").pop() : ""} → {destination ? destination.split("\\").pop() : ""}
              {stats.current_file_size > 0 && ` (${fileTransferredStr}/${fileSizeStr} • ${filePercentage.toFixed(0)}%)`}
            </span>
          )}
        </div>

        <div className="text-right flex-shrink-0 font-mono">
          <div className="text-base font-black text-sky-400 flex items-baseline justify-end gap-1">
            <span>{status === "canceled" ? "0.0" : speedMb}</span>
            <span className="text-[10px] text-neutral-400">MB/s</span>
          </div>
          <div className="text-[9px] text-neutral-500 font-mono">Avg: {avgSpeedMb} MB/s</div>
        </div>
      </div>

      {/* ── Progress Bar with Gradient ── */}
      <div className="flex flex-col gap-1 my-1">
        <div className="w-full h-2.5 bg-[#111624] rounded-full overflow-hidden p-0.5 border border-dark-border">
          <div
            className={`h-full rounded-full transition-all duration-150 ${
              status === "paused"
                ? "bg-amber-500"
                : status === "canceled"
                ? "bg-rose-500"
                : "bg-gradient-to-r from-blue-600 via-sky-400 to-emerald-400 shadow-sm"
            }`}
            style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
          />
        </div>

        <div className="flex items-center justify-between text-[10px] font-mono text-neutral-400">
          <span>
            {t.transferred}: <strong className="text-neutral-200">{transferredStr}</strong> / {totalStr} ({stats.overall_percentage.toFixed(0)}%)
          </span>
          <span>
            {t.eta}: <strong className="text-neutral-200">{etaStr}</strong> ({stats.completed_files}/{stats.total_files})
          </span>
        </div>
      </div>

      {/* ── Mini SpeedChart ── */}
      <div className="my-0.5">
        <SpeedChart speedHistory={speedHistory} />
      </div>

      {/* ── Bottom Controls ── */}
      <div className="flex items-center justify-between pt-2 border-t border-dark-border">
        <div className="flex items-center gap-1.5">
          {(status === "transferring" || status === "paused") && (
            <button
              onClick={onPauseToggle}
              className={`flex items-center gap-1 px-3 py-1 rounded-lg text-xs font-bold transition shadow-sm ${
                status === "paused"
                  ? "bg-emerald-600 hover:bg-emerald-500 text-white"
                  : "bg-amber-600 hover:bg-amber-500 text-white"
              }`}
            >
              {status === "paused" ? <Play className="w-3 h-3" /> : <Pause className="w-3 h-3" />}
              <span>{status === "paused" ? t.resume : t.pause}</span>
            </button>
          )}

          {(status === "transferring" || status === "paused") && (
            <button
              onClick={onCancel}
              title={t.cancel}
              className="p-1 rounded-lg bg-[#161e31] hover:bg-rose-950/80 text-neutral-300 hover:text-rose-300 border border-dark-border transition"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}

          {(status === "completed" || status === "canceled" || status === "failed") && onCloseApp && (
            <button
              onClick={onCloseApp}
              className="px-3 py-1 rounded-lg bg-[#161e31] hover:bg-[#1d2740] text-neutral-200 text-xs font-bold transition border border-dark-border"
            >
              {t.close}
            </button>
          )}
        </div>

        <button
          onClick={onExpand}
          className="flex items-center gap-1 px-3 py-1 rounded-lg bg-[#161e31] hover:bg-[#1d2740] text-sky-400 text-xs font-bold transition border border-dark-border cursor-pointer shadow-sm"
        >
          <span>{lang === "tr" ? "Genişlet" : "Details"}</span>
          <ChevronDown className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
};
