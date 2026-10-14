import React from "react";
import { Play, Pause, X, ChevronDown, ShieldCheck, Pin, FolderOpen, ArrowRight } from "lucide-react";
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

export const formatEta = (seconds: number, lang: "tr" | "en" = "tr"): string => {
  if (seconds <= 0 || !isFinite(seconds)) return lang === "tr" ? "0 sn" : "0s";
  if (seconds < 60) return `${Math.round(seconds)} ${lang === "tr" ? "sn" : "s"}`;
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  if (m < 60) return `${m} ${lang === "tr" ? "dk" : "m"} ${s} ${lang === "tr" ? "sn" : "s"}`;
  const h = Math.floor(m / 60);
  const remM = m % 60;
  return `${h} ${lang === "tr" ? "sa" : "h"} ${remM} ${lang === "tr" ? "dk" : "m"}`;
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
  onCloseApp,
  lang,
}) => {
  const t = translations[lang];
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
    <div className="w-full h-full flex flex-col justify-between p-3 bg-neutral-900 border border-neutral-800 rounded-lg shadow-2xl select-none font-sans text-neutral-100">
      {/* Top Header */}
      <div className="flex items-center justify-between pb-1.5 border-b border-neutral-800/80">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs font-bold text-indigo-400 tracking-wide">DeepNode Copy</span>
          {status === "transferring" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-indigo-950 text-indigo-300 border border-indigo-800/80 font-semibold flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse" />
              <span>{t.statusTransferring} ({stats.overall_percentage.toFixed(0)}%)</span>
            </span>
          )}
          {status === "paused" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-amber-950 text-amber-300 border border-amber-800/80 font-semibold flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-amber-400" />
              <span>{t.statusPaused}</span>
            </span>
          )}
          {status === "completed" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-950 text-emerald-300 border border-emerald-800/80 font-semibold flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
              <span>{t.statusFinished}</span>
            </span>
          )}
          {status === "canceled" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-rose-950 text-rose-300 border border-rose-800/80 font-semibold flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-rose-400" />
              <span>{t.statusCanceled}</span>
            </span>
          )}
          {status === "failed" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-red-950 text-red-300 border border-red-800/80 font-semibold flex items-center gap-1.5">
              <span className="w-1.5 h-1.5 rounded-full bg-red-500" />
              <span>{t.statusFailed}</span>
            </span>
          )}
          {status === "idle" && (
            <span className="text-[10px] px-2 py-0.5 rounded-full bg-neutral-800 text-neutral-300 border border-neutral-700 font-semibold">
              {t.statusReady}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1">
          <button
            onClick={onToggleAlwaysOnTop}
            title={t.alwaysOnTop}
            className={`p-1 rounded text-xs transition ${
              alwaysOnTop
                ? "bg-indigo-600 text-white"
                : "bg-neutral-800 hover:bg-neutral-700 text-neutral-400"
            }`}
          >
            <Pin className="w-3 h-3" />
          </button>
          <button
            onClick={onOpenVerify}
            title={t.clipboardTest}
            className="p-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-indigo-400 transition"
          >
            <ShieldCheck className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* From / To Path info */}
      {(source || destination) && (
        <div className="flex items-center gap-1.5 py-1 text-[11px] text-neutral-400 font-mono overflow-hidden">
          <FolderOpen className="w-3 h-3 text-neutral-500 flex-shrink-0" />
          <span className="truncate max-w-[45%]" title={source || ""}>
            {source ? source.split("\\").pop() || source : "..."}
          </span>
          <ArrowRight className="w-2.5 h-2.5 text-neutral-600 flex-shrink-0" />
          <span className="truncate max-w-[45%] text-neutral-300 font-semibold" title={destination || ""}>
            {destination ? destination.split("\\").pop() || destination : "..."}
          </span>
        </div>
      )}

      {/* Progress Bars Section */}
      <div className="flex flex-col gap-1.5 my-0.5">
        {/* 1. Current File Progress */}
        <div className="flex flex-col gap-0.5">
          <div className="flex items-center justify-between text-[11px]">
            <span className="font-mono text-neutral-300 truncate max-w-[70%]" title={stats.current_file_name}>
              {stats.current_file_name || t.statusReady}
            </span>
            <span className="text-[10px] font-mono text-neutral-400 flex-shrink-0">
              {stats.current_file_size > 0 ? `${fileTransferredStr} / ${fileSizeStr}` : ""}
            </span>
          </div>
          <div className="w-full h-2 bg-neutral-800 rounded-full overflow-hidden p-0.5 border border-neutral-700/60">
            <div
              className={`h-full rounded-full transition-all duration-100 ${
                status === "paused" ? "bg-amber-500" : "bg-sky-400"
              }`}
              style={{ width: `${Math.min(100, Math.max(0, filePercentage))}%` }}
            />
          </div>
        </div>

        {/* 2. Total Progress */}
        <div className="flex flex-col gap-0.5">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-neutral-400 font-medium text-[10px]">{t.totalProgress}</span>
            <span className="font-mono text-[11px] font-bold text-neutral-200">
              {transferredStr} / {totalStr} ({stats.overall_percentage.toFixed(1)}%)
            </span>
          </div>
          <div className="w-full h-2.5 bg-neutral-800 rounded-full overflow-hidden p-0.5 border border-neutral-700/60 shadow-inner">
            <div
              className={`h-full rounded-full transition-all duration-100 ${
                status === "paused"
                  ? "bg-amber-500"
                  : status === "canceled"
                  ? "bg-rose-500"
                  : "bg-gradient-to-r from-indigo-500 via-sky-400 to-emerald-400"
              }`}
              style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
            />
          </div>
        </div>
      </div>

      {/* 3. Real-Time Speed Chart */}
      <div className="my-1">
        <SpeedChart speedHistory={speedHistory} />
      </div>

      {/* Telemetry metrics & Control buttons */}
      <div className="flex items-center justify-between pt-1 border-t border-neutral-800/80 text-[11px] font-mono">
        <div className="flex items-center gap-3 text-neutral-400">
          <span>
            {t.speed}: <strong className="text-neutral-100">{speedMb} MB/s</strong>
            <span className="text-[9px] text-neutral-500 ml-1">({avgSpeedMb})</span>
          </span>
          <span>
            {t.eta}: <strong className="text-neutral-100">{etaStr}</strong>
          </span>
          <button
            onClick={onExpand}
            title={lang === "tr" ? "Dosya listesini açmak için tıklayın" : "Click to view file list"}
            className="hover:text-indigo-300 transition cursor-pointer flex items-center gap-1"
          >
            <span>{t.files}:</span>
            <strong className="text-neutral-100 underline decoration-indigo-500/50 underline-offset-2">
              {stats.completed_files}/{stats.total_files}
            </strong>
          </button>
        </div>

        <div className="flex items-center gap-1.5">
          {(status === "transferring" || status === "paused") && (
            <button
              onClick={onPauseToggle}
              className={`flex items-center gap-1 px-2.5 py-1 rounded text-xs font-sans font-medium transition shadow ${
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
              className="p-1 rounded bg-neutral-800 hover:bg-rose-900/80 text-neutral-300 hover:text-white transition border border-neutral-700"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}

          {(status === "completed" || status === "canceled" || status === "failed") && onCloseApp && (
            <button
              onClick={onCloseApp}
              title={lang === "tr" ? "Kapat" : "Close"}
              className="px-2.5 py-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-sans font-medium transition border border-neutral-700"
            >
              {lang === "tr" ? "Kapat" : "Close"}
            </button>
          )}

          <button
            onClick={onExpand}
            className="flex items-center gap-0.5 px-2 py-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-sans font-medium transition border border-neutral-700 ml-1"
          >
            <span>{t.more}</span>
            <ChevronDown className="w-3 h-3" />
          </button>
        </div>
      </div>
    </div>
  );
};
