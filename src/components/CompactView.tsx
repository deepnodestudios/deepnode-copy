import React from "react";
import { Play, Pause, X, ChevronDown, Pin, Zap, Minus, Maximize2, SkipForward } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TransferStats, TransferStatus } from "../types";
import { translations, Language } from "../i18n";

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
  onSkip?: () => void;
  onExpand: () => void;
  onOpenVerify: () => void;
  onOpenAbout: () => void;
  onCloseApp?: () => void;
  onMinimizeWindow?: () => void;
  lang: Language;
}

export const CompactView: React.FC<CompactViewProps> = ({
  stats,
  source,
  destination,
  status,
  alwaysOnTop,
  onToggleAlwaysOnTop,
  onPauseToggle,
  onCancel,
  onSkip,
  onExpand,
  onCloseApp,
  onMinimizeWindow,
  lang,
}) => {
  const t = translations[lang] || translations.tr;
  const speedBps = stats.current_speed_bps;
  const speedDisplay =
    speedBps >= 1_073_741_824
      ? `${(speedBps / 1_073_741_824).toFixed(2)} GB/s`
      : `${(speedBps / 1_048_576).toFixed(1)} MB/s`;

  const transferredStr = formatBytes(stats.transferred_bytes);
  const totalStr = formatBytes(stats.total_bytes);
  const etaStr = formatEta(stats.eta_seconds, lang);

  const [autoCloseRemaining, setAutoCloseRemaining] = React.useState<number | null>(null);
  const [isHovered, setIsHovered] = React.useState(false);

  React.useEffect(() => {
    // If completed without fatal errors, start 2.5s auto-close timer (matching reference copy tool UX)
    if (status === "completed") {
      setAutoCloseRemaining(2.5);
    } else {
      setAutoCloseRemaining(null);
    }
  }, [status]);

  React.useEffect(() => {
    if (autoCloseRemaining === null) return;
    if (autoCloseRemaining <= 0) {
      if (onCloseApp) {
        onCloseApp();
      }
      return;
    }

    if (isHovered) {
      // Pause countdown while user hovers over the window
      return;
    }

    const interval = setInterval(() => {
      setAutoCloseRemaining((prev) => {
        if (prev === null) return null;
        const next = Math.max(0, parseFloat((prev - 0.1).toFixed(1)));
        return next;
      });
    }, 100);

    return () => clearInterval(interval);
  }, [autoCloseRemaining, isHovered, onCloseApp]);

  return (
    <div
      data-tauri-drag-region="deep"
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      onMouseDown={(e) => {
        if (e.button === 0 && !(e.target as HTMLElement)?.closest('button, input, select, a, [role="button"], textarea')) {
          getCurrentWindow().startDragging().catch(() => {});
        }
      }}
      className="w-full h-full flex flex-col justify-between p-3.5 bg-[#080c16] border border-[#1e293b]/80 rounded-2xl shadow-2xl select-none font-sans text-slate-100 overflow-hidden cursor-grab active:cursor-grabbing"
    >
      {/* ── Top Bar ── */}
      <div
        data-tauri-drag-region="deep"
        className="flex items-center justify-between pb-2 border-b border-[#172136] cursor-grab active:cursor-grabbing"
      >
        <div data-tauri-drag-region="deep" className="flex items-center gap-2 min-w-0">
          <img data-tauri-drag-region="deep" src="/branding/app-icon.png" alt="icon" className="w-4 h-4 rounded shadow-sm" />
          <span data-tauri-drag-region="deep" className="text-xs font-bold text-slate-100 tracking-tight">DeepNode Copy</span>
          <span data-tauri-drag-region="deep" className="inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full bg-emerald-950/70 text-emerald-400 border border-emerald-500/40 font-bold font-mono">
            <Zap className="w-2.5 h-2.5 text-emerald-400" />
            <span>Direct I/O</span>
          </span>
        </div>

        {/* Window controls */}
        <div data-tauri-drag-region="false" className="flex items-center gap-1">
          <button
            data-tauri-drag-region="false"
            onClick={onToggleAlwaysOnTop}
            title={t.alwaysOnTop}
            className={`p-1 rounded-md text-xs transition border cursor-pointer ${
              alwaysOnTop
                ? "bg-blue-600 border-blue-500 text-white"
                : "bg-[#111624] hover:bg-[#1a233a] border-slate-800 text-slate-400 hover:text-slate-200"
            }`}
          >
            <Pin className="w-3 h-3" />
          </button>

          {onMinimizeWindow && (
            <button
              data-tauri-drag-region="false"
              onClick={onMinimizeWindow}
              title="Simge Durumuna Küçült"
              className="p-1 rounded-md bg-[#111624] hover:bg-[#1a233a] border border-slate-800 text-slate-400 hover:text-slate-200 transition cursor-pointer"
            >
              <Minus className="w-3 h-3" />
            </button>
          )}

          <button
            data-tauri-drag-region="false"
            onClick={onExpand}
            title={lang === "tr" ? "Pencereyi Genişlet" : "Expand Window"}
            className="p-1 rounded-md bg-[#111624] hover:bg-[#1a233a] border border-slate-800 text-slate-400 hover:text-sky-400 transition cursor-pointer"
          >
            <Maximize2 className="w-3 h-3" />
          </button>

          {onCloseApp && (
            <button
              data-tauri-drag-region="false"
              onClick={onCloseApp}
              title={t.close}
              className="p-1 rounded-md bg-[#111624] hover:bg-rose-950/80 border border-slate-800 text-slate-400 hover:text-rose-400 transition cursor-pointer"
            >
              <X className="w-3 h-3" />
            </button>
          )}
        </div>
      </div>

      {/* ── Active File & Giant Speed Display ── */}
      <div data-tauri-drag-region="deep" className="flex items-center justify-between gap-3 mt-1.5 mb-1 cursor-grab active:cursor-grabbing">
        <div data-tauri-drag-region="deep" className="flex flex-col min-w-0 flex-1">
          <span data-tauri-drag-region="deep" className="text-sm font-bold text-white truncate" title={stats.current_file_name}>
            {stats.current_file_name || (status === "idle" ? (lang === "tr" ? "Kuyruk Boş / Beklemede" : "Idle / Queue Empty") : t.statusReady)}
          </span>
          <span data-tauri-drag-region="deep" className="text-[11px] text-slate-400 font-mono truncate mt-0.5" title={source || destination ? `${source} → ${destination}` : ""}>
            {source && destination
              ? `${source} → ${destination}`
              : source
              ? `${source} → ${lang === "tr" ? "Hedef bekleniyor..." : "Target pending..."}`
              : destination
              ? `${lang === "tr" ? "Kuyruk bekleniyor" : "Queue pending"} → ${destination}`
              : lang === "tr"
              ? "Aktarım kuyruğu boş — Dosya bekleniyor"
              : "Transfer queue empty — Ready for files"}
          </span>
        </div>

        <div data-tauri-drag-region="deep" className="text-right flex-shrink-0 font-mono">
          <div data-tauri-drag-region="deep" className="text-lg font-black text-cyan-400 tracking-tight">
            {status === "canceled" ? "0.0 MB/s" : speedDisplay}
          </div>
        </div>
      </div>

      {/* ── Gradient Progress Bar ── */}
      <div data-tauri-drag-region="deep" className="flex flex-col gap-1.5 my-1 cursor-grab active:cursor-grabbing">
        <div data-tauri-drag-region="deep" className="w-full h-2 bg-[#101626] rounded-full overflow-hidden border border-slate-800/80">
          <div
            data-tauri-drag-region="deep"
            className={`h-full rounded-full transition-all duration-150 ${
              status === "paused"
                ? "bg-amber-500"
                : status === "canceled"
                ? "bg-rose-500"
                : "bg-gradient-to-r from-blue-600 via-cyan-400 to-emerald-400 shadow-sm"
            }`}
            style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
          />
        </div>

        <div data-tauri-drag-region="deep" className="flex items-center justify-between text-[11px] font-mono text-slate-300">
          <span data-tauri-drag-region="deep">
            Transferred: <strong data-tauri-drag-region="deep" className="text-white font-semibold">{transferredStr}</strong> / {totalStr} ({stats.overall_percentage.toFixed(0)}%)
          </span>
          <span data-tauri-drag-region="deep" className="text-slate-400">
            ETA: <strong data-tauri-drag-region="deep" className="text-white font-semibold">{etaStr}</strong> (File {stats.completed_files} / {stats.total_files})
          </span>
        </div>
      </div>

      {/* ── Bottom Controls Toolbar ── */}
      <div data-tauri-drag-region="deep" className="flex items-center justify-between pt-2 border-t border-[#172136] cursor-grab active:cursor-grabbing">
        <div data-tauri-drag-region="deep" className="flex items-center gap-2">
          {status === "transferring" || status === "paused" ? (
            <>
              <button
                data-tauri-drag-region="false"
                onClick={onPauseToggle}
                className={`flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl text-xs font-semibold transition cursor-pointer shadow-sm ${
                  status === "paused"
                    ? "bg-emerald-600 hover:bg-emerald-500 text-white"
                    : "bg-blue-600 hover:bg-blue-500 text-white"
                }`}
              >
                {status === "paused" ? <Play className="w-3.5 h-3.5 fill-current" /> : <Pause className="w-3.5 h-3.5 fill-current" />}
                <span>{status === "paused" ? t.resume : t.pause}</span>
              </button>

              {onSkip && (
                <button
                  data-tauri-drag-region="false"
                  onClick={onSkip}
                  title={lang === "tr" ? "Bu Dosyayı Atla ve Sonrakine Geç" : "Skip this file and proceed to next"}
                  className="flex items-center gap-1 px-3 py-1.5 rounded-xl bg-[#121827] hover:bg-amber-950/50 border border-[#1e293b] hover:border-amber-600/60 text-slate-300 hover:text-amber-300 text-xs font-medium transition cursor-pointer"
                >
                  <SkipForward className="w-3.5 h-3.5" />
                  <span>{lang === "tr" ? "Atla" : "Skip"}</span>
                </button>
              )}

              <button
                data-tauri-drag-region="false"
                onClick={onCancel}
                title={t.cancel}
                className="flex items-center gap-1 px-3 py-1.5 rounded-xl bg-[#121827] hover:bg-rose-950/60 border border-[#1e293b] hover:border-rose-900 text-slate-300 hover:text-rose-400 text-xs font-medium transition cursor-pointer"
              >
                <X className="w-3.5 h-3.5" />
                <span>{t.cancel}</span>
              </button>
            </>
          ) : status === "completed" ? (
            <div data-tauri-drag-region="deep" className="flex items-center gap-2">
              <span data-tauri-drag-region="deep" className="text-xs font-semibold text-emerald-400 flex items-center gap-1.5">
                <span>✔ {lang === "tr" ? "Aktarım Tamamlandı" : "Transfer Completed"}</span>
              </span>
              {autoCloseRemaining !== null && autoCloseRemaining > 0 && (
                <span data-tauri-drag-region="deep" className="text-[11px] text-slate-400 font-mono">
                  ({Math.ceil(autoCloseRemaining)}s {lang === "tr" ? "kapanıyor" : "closing"}...)
                </span>
              )}
              {isHovered && autoCloseRemaining !== null && (
                <span data-tauri-drag-region="deep" className="text-[10px] text-amber-400 font-mono">
                  [{lang === "tr" ? "Duraklatıldı" : "Paused"}]
                </span>
              )}
            </div>
          ) : status === "canceled" ? (
            <span data-tauri-drag-region="deep" className="text-xs font-semibold text-rose-400 flex items-center gap-1.5">
              <span>✕ {lang === "tr" ? "Aktarım İptal Edildi" : "Transfer Canceled"}</span>
            </span>
          ) : (
            <div data-tauri-drag-region="deep" className="flex items-center gap-1.5 text-xs text-slate-400 font-medium">
              <span className="w-2 h-2 rounded-full bg-slate-500" />
              <span>{lang === "tr" ? "Kuyruk Boş — Beklemede" : "Idle — Ready for transfer"}</span>
            </div>
          )}
        </div>

        <button
          data-tauri-drag-region="false"
          onClick={() => {
            setAutoCloseRemaining(null);
            onExpand();
          }}
          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-[#1e293b] text-slate-200 text-xs font-semibold transition cursor-pointer shadow-sm"
        >
          <ChevronDown className="w-3.5 h-3.5 text-slate-400" />
          <span>{lang === "tr" ? "Ayrıntılar" : "Details"}</span>
        </button>
      </div>
    </div>
  );
};
