import React, { useState } from "react";
import {
  Play,
  Pause,
  X,
  Minimize2,
  RefreshCw,
  CheckCircle2,
  XCircle,
  ShieldCheck,
  Pin,
  Settings,
  FolderPlus,
  FilePlus,
  FolderOpen,
  Trash2,
  Zap,
  Truck,
  ExternalLink,
  RotateCcw,
  Globe,
  Info,
  ListOrdered,
  LineChart,
  History,
  FileText,
  FileArchive,
  FileVideo,
  FileCode,
  FileSpreadsheet,
} from "lucide-react";
import { FileItem, TransferStats, ScanPreviewResult, TransferStatus } from "../types";
import { SpeedChart } from "./SpeedChart";
import { formatBytes, formatEta } from "./CompactView";
import { translations, Language, supportedLanguages } from "../i18n";

interface ExpandedViewProps {
  stats: TransferStats;
  files: FileItem[];
  speedHistory: number[];
  isPaused: boolean;
  isTransferring: boolean;
  transferStatus: TransferStatus;
  alwaysOnTop: boolean;
  onPauseToggle: () => void;
  onCancel: () => void;
  onRetryFailed: () => void;
  onMinimize: () => void;
  onToggleAlwaysOnTop: () => void;
  onOpenVerify: () => void;
  onOpenAbout: () => void;
  onRegisterContextMenu: () => void;
  onCloseApp?: () => void;
  onResetToIdle?: () => void;
  onFinishAction: string;
  onChangeOnFinish: (val: string) => void;
  conflictPolicy: string;
  onChangeConflictPolicy: (val: string) => void;
  verifyMode: string;
  onChangeVerifyMode: (val: string) => void;
  hashAlgorithm: string;
  onChangeHashAlgorithm: (val: string) => void;
  directIo: boolean;
  onChangeDirectIo: (val: boolean) => void;
  stagedSources: string[];
  destination: string;
  onChangeDestination: (val: string) => void;
  stagedPreview: ScanPreviewResult | null;
  onAddFiles: () => void;
  onAddFolder: () => void;
  onBrowseDestination: () => void;
  onRemoveStaged: (index: number) => void;
  onClearStaged: () => void;
  onStartTransfer: (mode: "copy" | "move") => void;
  lang: Language;
  onLangChange: (lang: Language) => void;
}

export const ExpandedView: React.FC<ExpandedViewProps> = ({
  stats,
  files,
  speedHistory,
  isPaused,
  isTransferring,
  transferStatus,
  alwaysOnTop,
  onPauseToggle,
  onCancel,
  onRetryFailed,
  onMinimize,
  onToggleAlwaysOnTop,
  onOpenVerify,
  onOpenAbout,
  onRegisterContextMenu,
  onCloseApp,
  onResetToIdle,
  onFinishAction,
  onChangeOnFinish,
  conflictPolicy,
  onChangeConflictPolicy,
  verifyMode,
  onChangeVerifyMode,
  hashAlgorithm,
  onChangeHashAlgorithm,
  directIo,
  onChangeDirectIo,
  stagedSources,
  destination,
  onChangeDestination,
  stagedPreview,
  onAddFiles,
  onAddFolder,
  onBrowseDestination,
  onRemoveStaged,
  onClearStaged,
  onStartTransfer,
  lang,
  onLangChange,
}) => {
  const t = translations[lang] || translations.tr;
  const [filter, setFilter] = useState<"all" | "completed" | "remaining" | "skipped" | "failed">("all");
  const [sidebarTab, setSidebarTab] = useState<"queue" | "analytics" | "verify" | "settings" | "history">("queue");

  const speedMb = (stats.current_speed_bps / 1_048_576).toFixed(1);
  const avgSpeedMb = (stats.average_speed_bps / 1_048_576).toFixed(1);

  const remainingFilesCount = files.filter(
    (f) => f.status === "pending" || f.status === "copying" || f.status === "verifying"
  ).length;

  const filteredFiles = files.filter((f) => {
    if (filter === "completed") return f.status === "completed";
    if (filter === "remaining")
      return f.status === "pending" || f.status === "copying" || f.status === "verifying";
    if (filter === "skipped") return f.status === "skipped";
    if (filter === "failed") return f.status === "failed";
    return true;
  });

  const stagedTotalMb = stagedPreview ? (stagedPreview.total_bytes / 1_048_576).toFixed(1) : "0.0";

  // Tab labels helper with fallback to domain terminology
  const getTabLabel = (id: string) => {
    if (id === "queue") {
      const count = isTransferring || files.length > 0 ? files.length : stagedSources.length;
      return lang === "tr" ? `Transfer Kuyruğu (${count})` : `Transfer Queue (${count})`;
    }
    if (id === "analytics") return lang === "tr" ? "Hız Analitiği" : "Speed Analytics";
    if (id === "verify") return lang === "tr" ? "Bütünlük Doğrulama" : "Hash Verification";
    if (id === "settings") return lang === "tr" ? "İstasyon Ayarları" : "Station Settings";
    if (id === "history") return lang === "tr" ? "Geçmiş & Günlükler" : "History & Logs";
    return id;
  };

  const getFileIcon = (filename: string) => {
    const ext = filename.split(".").pop()?.toLowerCase() || "";
    if (["zip", "rar", "7z", "tar", "gz", "bin", "iso"].includes(ext)) {
      return <FileArchive className="w-4 h-4 text-sky-400 flex-shrink-0" />;
    }
    if (["mp4", "mkv", "avi", "mov", "webm"].includes(ext)) {
      return <FileVideo className="w-4 h-4 text-amber-400 flex-shrink-0" />;
    }
    if (["js", "ts", "rs", "cpp", "py", "html", "css", "json"].includes(ext)) {
      return <FileCode className="w-4 h-4 text-emerald-400 flex-shrink-0" />;
    }
    if (["csv", "xlsx", "xls"].includes(ext)) {
      return <FileSpreadsheet className="w-4 h-4 text-emerald-500 flex-shrink-0" />;
    }
    return <FileText className="w-4 h-4 text-neutral-400 flex-shrink-0" />;
  };

  return (
    <div className="w-full h-full flex flex-col bg-[#0b0f19] border border-dark-border text-neutral-100 select-none overflow-hidden font-sans">
      {/* ── Title Bar ── */}
      <div className="flex items-center justify-between px-3.5 py-2 bg-[#080c14] border-b border-dark-border flex-shrink-0">
        <div className="flex items-center gap-2.5">
          <img src="/branding/app-icon.png" alt="DeepNode Copy" className="w-5 h-5 rounded" />
          <span className="text-xs font-extrabold text-neutral-100 tracking-tight">
            DeepNode Copy
          </span>
          <span className="text-[10px] text-neutral-400 font-mono hidden sm:inline">
            — Multi-Station Engine
          </span>
          <span className="inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full bg-emerald-950/70 text-emerald-300 border border-emerald-800/80 font-bold font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <span>SIMD xxHash3</span>
          </span>
          {directIo && (
            <span className="inline-flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full bg-blue-950/70 text-blue-300 border border-blue-800/80 font-bold font-mono">
              <Zap className="w-2.5 h-2.5 text-blue-400" />
              <span>Direct I/O</span>
            </span>
          )}
        </div>

        <div className="flex items-center gap-2">
          {/* Language selector */}
          <div className="flex items-center bg-[#111624] hover:bg-[#161e31] rounded-lg px-2 py-1 border border-dark-border transition">
            <Globe className="w-3 h-3 text-neutral-400 mr-1.5 flex-shrink-0" />
            <select
              value={lang}
              onChange={(e) => onLangChange(e.target.value as Language)}
              className="bg-transparent text-[11px] font-mono text-neutral-200 focus:outline-none cursor-pointer"
              title={t.languageLabel}
            >
              {supportedLanguages.map((l) => (
                <option key={l.code} value={l.code} className="bg-[#0b0f19] text-neutral-200">
                  {l.flag} {l.nativeName} ({l.code})
                </option>
              ))}
            </select>
          </div>

          {/* Always on top toggle */}
          <button
            onClick={onToggleAlwaysOnTop}
            title={t.alwaysOnTop}
            className={`p-1.5 rounded-lg text-xs transition border ${
              alwaysOnTop
                ? "bg-blue-600 border-blue-500 text-white"
                : "bg-[#111624] hover:bg-[#161e31] border-dark-border text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Pin className="w-3.5 h-3.5" />
          </button>

          {/* About Dialog */}
          <button
            onClick={onOpenAbout}
            title={t.aboutTitle}
            className="p-1.5 rounded-lg bg-[#111624] hover:bg-[#161e31] border border-dark-border text-neutral-400 hover:text-sky-400 transition"
          >
            <Info className="w-3.5 h-3.5" />
          </button>

          {/* Compact mode toggle */}
          <button
            onClick={onMinimize}
            title={t.compactMode}
            className="p-1.5 rounded-lg bg-[#111624] hover:bg-[#161e31] border border-dark-border text-neutral-400 hover:text-sky-400 transition"
          >
            <Minimize2 className="w-3.5 h-3.5" />
          </button>

          {onCloseApp && (
            <button
              onClick={onCloseApp}
              title={t.close}
              className="p-1.5 rounded-lg bg-[#111624] hover:bg-rose-950/80 border border-dark-border text-neutral-400 hover:text-rose-300 transition"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* ── Main Layout (Sidebar + Content Area) ── */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left Sidebar */}
        <aside className="w-52 bg-[#080c14] border-r border-dark-border flex flex-col justify-between p-2.5 flex-shrink-0">
          <div className="flex flex-col gap-1">
            <button
              onClick={() => setSidebarTab("queue")}
              className={`flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-semibold transition text-left ${
                sidebarTab === "queue"
                  ? "bg-blue-600/15 border border-blue-500/40 text-sky-400 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-[#111624] border border-transparent"
              }`}
            >
              <ListOrdered className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{getTabLabel("queue")}</span>
            </button>

            <button
              onClick={() => setSidebarTab("analytics")}
              className={`flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-semibold transition text-left ${
                sidebarTab === "analytics"
                  ? "bg-blue-600/15 border border-blue-500/40 text-sky-400 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-[#111624] border border-transparent"
              }`}
            >
              <LineChart className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{getTabLabel("analytics")}</span>
            </button>

            <button
              onClick={() => setSidebarTab("verify")}
              className={`flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-semibold transition text-left ${
                sidebarTab === "verify"
                  ? "bg-blue-600/15 border border-blue-500/40 text-sky-400 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-[#111624] border border-transparent"
              }`}
            >
              <ShieldCheck className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{getTabLabel("verify")}</span>
            </button>

            <button
              onClick={() => setSidebarTab("settings")}
              className={`flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-semibold transition text-left ${
                sidebarTab === "settings"
                  ? "bg-blue-600/15 border border-blue-500/40 text-sky-400 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-[#111624] border border-transparent"
              }`}
            >
              <Settings className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{getTabLabel("settings")}</span>
            </button>

            <button
              onClick={() => setSidebarTab("history")}
              className={`flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-semibold transition text-left ${
                sidebarTab === "history"
                  ? "bg-blue-600/15 border border-blue-500/40 text-sky-400 shadow-sm"
                  : "text-neutral-400 hover:text-neutral-200 hover:bg-[#111624] border border-transparent"
              }`}
            >
              <History className="w-4 h-4 flex-shrink-0" />
              <span className="truncate">{getTabLabel("history")}</span>
            </button>
          </div>

          {/* Sidebar Footer info */}
          <div className="pt-2 border-t border-dark-border flex flex-col gap-2">
            <div className="bg-[#111624] p-2 rounded-lg border border-dark-border text-[10px] font-mono text-neutral-400">
              <div className="flex justify-between items-center mb-1">
                <span>Engine:</span>
                <span className="text-emerald-400 font-bold">Rust Win32</span>
              </div>
              <div className="flex justify-between items-center">
                <span>I/O Mode:</span>
                <span className="text-sky-400 font-bold">{directIo ? "Direct I/O" : "Overlapped"}</span>
              </div>
            </div>

            <button
              onClick={onMinimize}
              className="w-full flex items-center justify-center gap-1.5 py-1.5 rounded-lg bg-[#111624] hover:bg-[#161e31] border border-dark-border text-[11px] font-semibold text-neutral-300 transition cursor-pointer"
            >
              <Minimize2 className="w-3 h-3 text-sky-400" />
              <span>{t.compactMode}</span>
            </button>
          </div>
        </aside>

        {/* Right Main Content Area */}
        <section className="flex-1 flex flex-col min-w-0 bg-[#0d111a] overflow-hidden">
          {/* Top Active Batch / Station Summary Bar */}
          <div className="p-3 bg-[#111624] border-b border-dark-border flex flex-col gap-2.5 flex-shrink-0">
            {isTransferring || transferStatus === "completed" || transferStatus === "canceled" || transferStatus === "failed" ? (
              /* Active Transfer Header */
              <div className="flex items-center justify-between">
                <div className="flex flex-col gap-0.5 min-w-0 flex-1">
                  <div className="flex items-center gap-2 text-sm font-extrabold text-neutral-100 tracking-tight">
                    <span>
                      {isTransferring ? (lang === "tr" ? "Aktif İşlem: Dosya Aktarımı" : "Active Batch: File Transfer") :
                       transferStatus === "completed" ? (lang === "tr" ? "Tamamlandı: Aktarım Başarılı" : "Completed: Transfer Successful") :
                       (lang === "tr" ? "İşlem Durduruldu" : "Transfer Stopped")}
                    </span>
                    {transferStatus === "transferring" && (
                      <span className="text-[10px] px-2 py-0.5 rounded-full bg-blue-950 text-sky-300 border border-blue-800 font-mono font-bold">
                        {stats.overall_percentage.toFixed(0)}%
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-1.5 text-xs text-neutral-400 font-mono truncate">
                    <span className="text-neutral-500 font-sans text-[11px]">{t.destination}:</span>
                    <span className="text-neutral-200 font-semibold truncate" title={destination}>
                      {destination || "..."}
                    </span>
                    <span className="text-neutral-600">•</span>
                    <span className="text-emerald-400 text-[11px] font-sans">Direct I/O: Aktif</span>
                  </div>
                </div>

                <div className="flex items-center gap-3 text-right">
                  <div>
                    <div className="text-base sm:text-lg font-black text-sky-400 font-mono flex items-baseline justify-end gap-1">
                      <span>{transferStatus === "canceled" ? "0.0" : speedMb}</span>
                      <span className="text-xs font-semibold text-neutral-400">MB/s</span>
                    </div>
                    <div className="text-[10px] text-emerald-400 font-semibold flex items-center justify-end gap-1">
                      <CheckCircle2 className="w-3 h-3" />
                      <span>{stats.completed_files} {t.files} OK</span>
                    </div>
                  </div>

                  {/* Transfer Controls */}
                  <div className="flex items-center gap-1.5 ml-2">
                    {(isTransferring || transferStatus === "paused") && (
                      <button
                        onClick={onPauseToggle}
                        className={`flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-bold transition shadow-sm ${
                          isPaused
                            ? "bg-emerald-600 hover:bg-emerald-500 text-white"
                            : "bg-amber-600 hover:bg-amber-500 text-white"
                        }`}
                      >
                        {isPaused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
                        <span>{isPaused ? t.resume : t.pause}</span>
                      </button>
                    )}

                    {(isTransferring || transferStatus === "paused") && (
                      <button
                        onClick={onCancel}
                        className="px-2.5 py-1.5 rounded-lg bg-[#161e31] hover:bg-rose-950/80 text-neutral-300 hover:text-rose-300 border border-dark-border text-xs font-bold transition"
                      >
                        <X className="w-3.5 h-3.5" />
                      </button>
                    )}

                    {onResetToIdle && (transferStatus === "completed" || transferStatus === "canceled" || transferStatus === "failed") && (
                      <button
                        onClick={onResetToIdle}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-xs font-bold transition shadow-sm"
                      >
                        <RotateCcw className="w-3.5 h-3.5" />
                        <span>{t.newTransfer}</span>
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ) : (
              /* Idle Station Setup Controls */
              <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2">
                  {/* Add Buttons */}
                  <div className="flex items-center gap-2">
                    <button
                      onClick={onAddFiles}
                      className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white text-xs font-bold transition shadow-sm cursor-pointer"
                    >
                      <FilePlus className="w-3.5 h-3.5" />
                      <span>{t.addFiles}</span>
                    </button>
                    <button
                      onClick={onAddFolder}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[#161e31] hover:bg-[#1d2740] border border-dark-border text-neutral-200 text-xs font-bold transition shadow-sm cursor-pointer"
                    >
                      <FolderPlus className="w-3.5 h-3.5" />
                      <span>{t.addFolder}</span>
                    </button>
                    {stagedSources.length > 0 && (
                      <button
                        onClick={onClearStaged}
                        title={t.clearList}
                        className="p-1.5 rounded-lg bg-[#161e31] hover:bg-rose-950/80 text-neutral-400 hover:text-rose-300 border border-dark-border transition"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    )}
                  </div>

                  {/* Primary Action Buttons: Copy / Move */}
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => onStartTransfer("copy")}
                      disabled={stagedSources.length === 0}
                      className={`flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-extrabold transition shadow-md cursor-pointer ${
                        stagedSources.length > 0
                          ? "bg-gradient-to-r from-sky-500 via-blue-600 to-emerald-500 hover:brightness-110 text-white"
                          : "bg-neutral-800 text-neutral-500 cursor-not-allowed border border-neutral-700/50"
                      }`}
                    >
                      <Zap className="w-3.5 h-3.5" />
                      <span>{t.copy}</span>
                    </button>

                    <button
                      onClick={() => onStartTransfer("move")}
                      disabled={stagedSources.length === 0}
                      className={`flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg text-xs font-bold transition border cursor-pointer ${
                        stagedSources.length > 0
                          ? "bg-[#161e31] hover:bg-[#1d2740] border-emerald-600/50 text-emerald-300"
                          : "bg-neutral-800 text-neutral-500 border-transparent cursor-not-allowed"
                      }`}
                    >
                      <Truck className="w-3.5 h-3.5" />
                      <span>{t.move}</span>
                    </button>
                  </div>
                </div>

                {/* Destination Input Row */}
                <div className="flex items-center gap-2">
                  <div className="relative flex-1">
                    <input
                      type="text"
                      value={destination}
                      onChange={(e) => onChangeDestination(e.target.value)}
                      placeholder={lang === "tr" ? "Hedef Klasör (Örn: D:\\Yedekler veya Gözat...)" : "Destination Folder (e.g. D:\\Backups or Browse...)"}
                      className="w-full pl-3 pr-2 py-1.5 rounded-lg bg-[#080c14] border border-dark-border text-xs font-mono text-neutral-200 placeholder-neutral-500 focus:outline-none focus:border-blue-500 transition"
                    />
                  </div>
                  <button
                    onClick={onBrowseDestination}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-[#161e31] hover:bg-[#1d2740] text-neutral-200 text-xs font-bold transition border border-dark-border flex-shrink-0 cursor-pointer"
                  >
                    <FolderOpen className="w-3.5 h-3.5 text-amber-400" />
                    <span>{t.browseDestination}</span>
                  </button>
                </div>

                {/* Inline Fast Toggles */}
                <div className="flex items-center justify-between text-[11px] font-mono text-neutral-400 pt-1 border-t border-dark-border/60">
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-1.5">
                      <span className="text-neutral-500 font-sans">{t.conflictPolicy}:</span>
                      <select
                        value={conflictPolicy}
                        onChange={(e) => onChangeConflictPolicy(e.target.value)}
                        className="bg-[#080c14] border border-dark-border rounded px-2 py-0.5 text-neutral-200 text-[11px] focus:outline-none"
                      >
                        <option value="auto_rename">{t.conflictRename}</option>
                        <option value="older">{t.conflictOlder}</option>
                        <option value="overwrite">{t.conflictOverwrite}</option>
                        <option value="skip">{t.conflictSkip}</option>
                      </select>
                    </div>

                    <div className="flex items-center gap-1.5">
                      <span className="text-neutral-500 font-sans">{t.hashAlgorithm}:</span>
                      <select
                        value={hashAlgorithm}
                        onChange={(e) => onChangeHashAlgorithm(e.target.value)}
                        className="bg-[#080c14] border border-dark-border rounded px-2 py-0.5 text-neutral-200 text-[11px] focus:outline-none"
                      >
                        <option value="xxhash3">xxHash3-64 (SIMD 30 GB/s)</option>
                        <option value="blake3">BLAKE3</option>
                        <option value="sha256">SHA-256</option>
                        <option value="md5">MD5</option>
                      </select>
                    </div>

                    <label className="flex items-center gap-1.5 cursor-pointer text-neutral-300 font-sans">
                      <input
                        type="checkbox"
                        checked={directIo}
                        onChange={(e) => onChangeDirectIo(e.target.checked)}
                        className="rounded border-neutral-700 bg-neutral-900 text-blue-600 focus:ring-0"
                      />
                      <span className="font-semibold text-sky-400">Direct I/O</span>
                    </label>
                  </div>

                  <div className="text-neutral-400 font-sans">
                    {stagedSources.length > 0 ? (
                      <span className="text-sky-300 font-semibold font-mono">
                        {stagedSources.length} {t.selectedItems} ({stagedPreview ? stagedPreview.total_files : 0} {t.files}, {stagedTotalMb} MB)
                      </span>
                    ) : (
                      <span className="text-neutral-500">{t.noFilesQueued}</span>
                    )}
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* ── Active Tab Content ── */}
          <div className="flex-1 overflow-y-auto p-3">
            {/* 1. QUEUE TAB */}
            {sidebarTab === "queue" && (
              <div className="h-full flex flex-col">
                {/* Filter pills if files are active */}
                {files.length > 0 && (
                  <div className="flex items-center justify-between pb-2 mb-2 border-b border-dark-border text-[11px]">
                    <div className="flex items-center gap-1.5">
                      <button
                        onClick={() => setFilter("all")}
                        className={`px-2.5 py-0.5 rounded-lg transition font-medium ${
                          filter === "all" ? "bg-blue-600 text-white" : "text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        {t.all} ({files.length})
                      </button>
                      <button
                        onClick={() => setFilter("completed")}
                        className={`px-2.5 py-0.5 rounded-lg transition font-medium ${
                          filter === "completed" ? "bg-emerald-600 text-white" : "text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        {t.completed} ({stats.completed_files})
                      </button>
                      <button
                        onClick={() => setFilter("remaining")}
                        className={`px-2.5 py-0.5 rounded-lg transition font-medium ${
                          filter === "remaining" ? "bg-sky-600 text-white" : "text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        {t.remainingFilter} ({remainingFilesCount})
                      </button>
                      <button
                        onClick={() => setFilter("skipped")}
                        className={`px-2.5 py-0.5 rounded-lg transition font-medium ${
                          filter === "skipped" ? "bg-amber-600 text-white" : "text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        {t.skipped} ({stats.skipped_files})
                      </button>
                      <button
                        onClick={() => setFilter("failed")}
                        className={`px-2.5 py-0.5 rounded-lg transition font-medium ${
                          filter === "failed" ? "bg-rose-600 text-white" : "text-neutral-400 hover:text-neutral-200"
                        }`}
                      >
                        {t.failed} ({stats.failed_files})
                      </button>
                    </div>

                    <div className="text-[11px] font-mono text-neutral-400">
                      {formatBytes(stats.transferred_bytes)} / {formatBytes(stats.total_bytes)}
                    </div>
                  </div>
                )}

                {/* Queue Table */}
                {files.length > 0 ? (
                  <div className="flex-1 flex flex-col bg-[#080c14] rounded-xl border border-dark-border overflow-hidden">
                    {/* Table Header */}
                    <div className="grid grid-cols-12 gap-2 px-3 py-2 bg-[#111624] border-b border-dark-border text-[10px] font-bold text-neutral-400 uppercase font-mono tracking-wider">
                      <div className="col-span-6">{lang === "tr" ? "DOSYA ADI" : "FILENAME"}</div>
                      <div className="col-span-2 text-right">{lang === "tr" ? "BOYUT" : "SIZE"}</div>
                      <div className="col-span-2 text-center">{lang === "tr" ? "HIZ / DOĞRULAMA" : "SPEED / VERIF"}</div>
                      <div className="col-span-2 text-center">{lang === "tr" ? "DURUM" : "STATUS"}</div>
                    </div>

                    {/* Table Rows */}
                    <div className="flex-1 overflow-y-auto divide-y divide-dark-border/40">
                      {filteredFiles.map((item) => (
                        <div
                          key={item.id}
                          className="grid grid-cols-12 gap-2 px-3 py-2.5 items-center hover:bg-[#111624]/60 text-xs transition"
                        >
                          <div className="col-span-6 flex items-center gap-2 min-w-0">
                            {getFileIcon(item.relative_path || item.src_path)}
                            <div className="flex flex-col min-w-0 flex-1">
                              <span className="truncate font-mono text-neutral-200 text-[11px]" title={item.relative_path || item.src_path}>
                                {item.relative_path || item.src_path}
                              </span>
                              {item.status === "copying" && (
                                <div className="w-full h-1 bg-neutral-800 rounded-full overflow-hidden mt-1 max-w-xs">
                                  <div
                                    className="h-full bg-gradient-to-r from-sky-400 to-blue-500 rounded-full"
                                    style={{
                                      width: `${item.size > 0 ? Math.min(100, Math.round(((item.bytes_transferred || 0) / item.size) * 100)) : 0}%`,
                                    }}
                                  />
                                </div>
                              )}
                              {item.error_message && (
                                <span className="text-[10px] text-rose-400 truncate">
                                  {item.error_message}
                                </span>
                              )}
                            </div>
                          </div>

                          <div className="col-span-2 text-right font-mono text-neutral-300 text-[11px]">
                            {formatBytes(item.size)}
                          </div>

                          <div className="col-span-2 text-center font-mono text-[10px]">
                            {item.status === "completed" ? (
                              <span className="text-emerald-400 font-semibold">xxHash3 Match</span>
                            ) : item.status === "copying" ? (
                              <span className="text-sky-400 font-bold">{speedMb} MB/s</span>
                            ) : (
                              <span className="text-neutral-500">—</span>
                            )}
                          </div>

                          <div className="col-span-2 text-center">
                            {item.status === "completed" ? (
                              <span className="inline-flex items-center gap-1 text-[10px] font-extrabold px-2 py-0.5 rounded-md bg-emerald-950/60 border border-emerald-800/80 text-emerald-400">
                                <CheckCircle2 className="w-3 h-3" />
                                <span>Done</span>
                              </span>
                            ) : item.status === "copying" ? (
                              <span className="inline-flex items-center gap-1 text-[10px] font-extrabold px-2 py-0.5 rounded-md bg-blue-950/60 border border-blue-800/80 text-sky-400 animate-pulse">
                                <RefreshCw className="w-3 h-3 animate-spin" />
                                <span>Copying</span>
                              </span>
                            ) : item.status === "failed" ? (
                              <span className="inline-flex items-center gap-1 text-[10px] font-extrabold px-2 py-0.5 rounded-md bg-rose-950/60 border border-rose-800/80 text-rose-400">
                                <XCircle className="w-3 h-3" />
                                <span>Failed</span>
                              </span>
                            ) : item.status === "skipped" ? (
                              <span className="inline-flex items-center gap-1 text-[10px] font-extrabold px-2 py-0.5 rounded-md bg-amber-950/60 border border-amber-800/80 text-amber-400">
                                <span>Skipped</span>
                              </span>
                            ) : (
                              <span className="inline-flex items-center text-[10px] font-medium px-2 py-0.5 rounded-md bg-neutral-900 border border-neutral-800 text-neutral-400">
                                Queued
                              </span>
                            )}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : stagedSources.length > 0 ? (
                  /* Staged queue list */
                  <div className="flex-1 flex flex-col bg-[#080c14] rounded-xl border border-dark-border overflow-hidden">
                    <div className="p-2.5 bg-[#111624] border-b border-dark-border flex justify-between items-center text-xs font-semibold text-neutral-300">
                      <span>{t.selectedItems} ({stagedSources.length})</span>
                      <span className="font-mono text-sky-400">{stagedTotalMb} MB Toplam</span>
                    </div>
                    <div className="flex-1 overflow-y-auto divide-y divide-dark-border/40 p-1">
                      {stagedSources.map((path, idx) => (
                        <div
                          key={idx}
                          className="flex items-center justify-between p-2 rounded hover:bg-[#111624]/60 text-xs transition"
                        >
                          <div className="flex items-center gap-2 min-w-0 flex-1">
                            <FolderOpen className="w-4 h-4 text-sky-400 flex-shrink-0" />
                            <span className="truncate font-mono text-neutral-200 text-[11px]">{path}</span>
                          </div>
                          <button
                            onClick={() => onRemoveStaged(idx)}
                            className="p-1 rounded text-neutral-500 hover:text-rose-400 transition"
                          >
                            <X className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : (
                  /* Empty Queue State: Modern Dropzone */
                  <div className="h-full flex flex-col items-center justify-center p-8 text-center border-2 border-dashed border-dark-border rounded-2xl bg-[#080c14]/50">
                    <div className="w-16 h-16 rounded-2xl bg-blue-600/10 border border-blue-500/20 flex items-center justify-center mb-4 text-sky-400 shadow-inner">
                      <FolderPlus className="w-8 h-8" />
                    </div>
                    <h3 className="text-sm font-bold text-neutral-200 mb-1">
                      {t.dragDropHint}
                    </h3>
                    <p className="text-xs text-neutral-400 max-w-md mb-5 leading-relaxed">
                      {lang === "tr"
                        ? "Dosyaları buraya sürükleyip bırakabilir, butonlarla ekleyebilir veya Windows 11 sağ tık menüsünden 'DeepNode Copy' ile aktarabilirsiniz."
                        : "Drag and drop files here, use the add buttons above, or trigger via the Windows 11 context menu 'DeepNode Copy'."}
                    </p>
                    <div className="flex items-center gap-3">
                      <button
                        onClick={onAddFiles}
                        className="flex items-center gap-2 px-4 py-2 rounded-xl bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-500 hover:to-indigo-500 text-white text-xs font-bold transition shadow-md cursor-pointer"
                      >
                        <FilePlus className="w-4 h-4" />
                        <span>{t.addFiles}</span>
                      </button>
                      <button
                        onClick={onAddFolder}
                        className="flex items-center gap-2 px-4 py-2 rounded-xl bg-[#161e31] hover:bg-[#1d2740] border border-dark-border text-neutral-200 text-xs font-bold transition shadow-sm cursor-pointer"
                      >
                        <FolderPlus className="w-4 h-4" />
                        <span>{t.addFolder}</span>
                      </button>
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* 2. SPEED ANALYTICS TAB */}
            {sidebarTab === "analytics" && (
              <div className="h-full flex flex-col gap-4">
                <div className="bg-[#080c14] border border-dark-border rounded-xl p-4 flex flex-col gap-2">
                  <div className="flex justify-between items-center">
                    <span className="text-xs font-bold text-neutral-300 uppercase tracking-wider font-mono">
                      {lang === "tr" ? "Gerçek Zamanlı Donanım Aktarım Eğrisi" : "Real-Time Hardware Throughput Curve"}
                    </span>
                    <span className="text-xs font-mono text-sky-400 font-bold">
                      {speedMb} MB/s
                    </span>
                  </div>
                  <div className="w-full h-36">
                    <SpeedChart speedHistory={speedHistory} />
                  </div>
                </div>

                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                  <div className="bg-[#080c14] border border-dark-border p-3 rounded-xl flex flex-col gap-1">
                    <span className="text-[10px] text-neutral-400 uppercase font-mono">{t.speed}</span>
                    <span className="text-lg font-black font-mono text-sky-400">{speedMb} MB/s</span>
                    <span className="text-[10px] text-neutral-500 font-mono">Avg: {avgSpeedMb} MB/s</span>
                  </div>

                  <div className="bg-[#080c14] border border-dark-border p-3 rounded-xl flex flex-col gap-1">
                    <span className="text-[10px] text-neutral-400 uppercase font-mono">{t.transferred}</span>
                    <span className="text-lg font-black font-mono text-neutral-100">{formatBytes(stats.transferred_bytes)}</span>
                    <span className="text-[10px] text-neutral-500 font-mono">Total: {formatBytes(stats.total_bytes)}</span>
                  </div>

                  <div className="bg-[#080c14] border border-dark-border p-3 rounded-xl flex flex-col gap-1">
                    <span className="text-[10px] text-neutral-400 uppercase font-mono">{t.eta}</span>
                    <span className="text-lg font-black font-mono text-emerald-400">{formatEta(stats.eta_seconds, lang)}</span>
                    <span className="text-[10px] text-neutral-500 font-mono">{stats.overall_percentage.toFixed(1)}% Tamamlandı</span>
                  </div>

                  <div className="bg-[#080c14] border border-dark-border p-3 rounded-xl flex flex-col gap-1">
                    <span className="text-[10px] text-neutral-400 uppercase font-mono">{t.files}</span>
                    <span className="text-lg font-black font-mono text-neutral-100">{stats.completed_files} / {stats.total_files}</span>
                    <span className="text-[10px] text-rose-400 font-mono">Hatalı: {stats.failed_files}</span>
                  </div>
                </div>
              </div>
            )}

            {/* 3. INTEGRATED HASH VERIFICATION TAB */}
            {sidebarTab === "verify" && (
              <div className="h-full flex flex-col gap-3">
                <div className="bg-[#080c14] border border-dark-border rounded-xl p-4 flex flex-col gap-3">
                  <div className="flex items-center gap-2 text-sky-400 border-b border-dark-border pb-2.5">
                    <ShieldCheck className="w-5 h-5" />
                    <h3 className="text-xs font-bold text-neutral-100 uppercase tracking-wider">
                      {lang === "tr" ? "SIMD Bütünlük Doğrulayıcı" : "Cryptographic Hash Verifier"}
                    </h3>
                  </div>

                  <p className="text-xs text-neutral-400 leading-relaxed">
                    {lang === "tr"
                      ? "Kopyalanan veya sistemdeki herhangi bir dosyanın sağlama toplamını donanım hızlandırmalı AVX-512 xxHash3, BLAKE3 veya SHA-256 ile anında doğrulayın."
                      : "Verify bit-level cryptographic parity using hardware-accelerated AVX-512 xxHash3, BLAKE3, or SHA-256."}
                  </p>

                  <div className="flex items-center gap-3 pt-2">
                    <button
                      onClick={onOpenVerify}
                      className="px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-500 text-white text-xs font-bold transition shadow-sm cursor-pointer"
                    >
                      {lang === "tr" ? "Doğrulama Aracını Aç" : "Launch Hash Verification Tool"}
                    </button>
                    <span className="text-xs text-emerald-400 font-mono font-semibold">
                      ✓ Otomatik pano algılama hazır
                    </span>
                  </div>
                </div>
              </div>
            )}

            {/* 4. STATION SETTINGS TAB */}
            {sidebarTab === "settings" && (
              <div className="h-full flex flex-col gap-4">
                <div className="bg-[#080c14] border border-dark-border rounded-xl p-4 flex flex-col gap-4">
                  <h3 className="text-xs font-bold text-neutral-100 uppercase tracking-wider border-b border-dark-border pb-2">
                    {t.transferSettings}
                  </h3>

                  <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
                    <div>
                      <label className="text-[11px] text-neutral-400 block mb-1 font-semibold">{t.languageLabel}</label>
                      <select
                        value={lang}
                        onChange={(e) => onLangChange(e.target.value as Language)}
                        className="w-full bg-[#111624] border border-dark-border rounded-lg p-2 text-neutral-200 text-xs focus:outline-none"
                      >
                        {supportedLanguages.map((l) => (
                          <option key={l.code} value={l.code}>
                            {l.flag} {l.nativeName} ({l.englishName})
                          </option>
                        ))}
                      </select>
                    </div>

                    <div>
                      <label className="text-[11px] text-neutral-400 block mb-1 font-semibold">{t.onFinish}</label>
                      <select
                        value={onFinishAction}
                        onChange={(e) => onChangeOnFinish(e.target.value)}
                        className="w-full bg-[#111624] border border-dark-border rounded-lg p-2 text-neutral-200 text-xs focus:outline-none"
                      >
                        <option value="do_nothing">{t.doNothing}</option>
                        <option value="close_app">{t.closeApp}</option>
                        <option value="sleep">{t.sleep}</option>
                        <option value="shutdown">{t.shutdown}</option>
                      </select>
                    </div>

                    <div>
                      <label className="text-[11px] text-neutral-400 block mb-1 font-semibold">{t.conflictPolicy}</label>
                      <select
                        value={conflictPolicy}
                        onChange={(e) => onChangeConflictPolicy(e.target.value)}
                        className="w-full bg-[#111624] border border-dark-border rounded-lg p-2 text-neutral-200 text-xs focus:outline-none"
                      >
                        <option value="auto_rename">{t.conflictRename}</option>
                        <option value="older">{t.conflictOlder}</option>
                        <option value="overwrite">{t.conflictOverwrite}</option>
                        <option value="skip">{t.conflictSkip}</option>
                      </select>
                    </div>

                    <div>
                      <label className="text-[11px] text-neutral-400 block mb-1 font-semibold">{t.verifyMode}</label>
                      <select
                        value={verifyMode}
                        onChange={(e) => onChangeVerifyMode(e.target.value)}
                        className="w-full bg-[#111624] border border-dark-border rounded-lg p-2 text-neutral-200 text-xs focus:outline-none"
                      >
                        <option value="always">{t.verifyAlways}</option>
                        <option value="never">{t.verifyNever}</option>
                      </select>
                    </div>
                  </div>

                  <div className="pt-2 border-t border-dark-border flex flex-col sm:flex-row gap-3">
                    <button
                      onClick={onRegisterContextMenu}
                      className="flex-1 py-2.5 px-4 rounded-xl bg-emerald-600/90 hover:bg-emerald-500 text-white font-bold text-xs transition flex items-center justify-center gap-2 cursor-pointer shadow-sm"
                    >
                      <ExternalLink className="w-4 h-4" />
                      <span>{t.registerContextMenu}</span>
                    </button>

                    <button
                      onClick={onOpenAbout}
                      className="py-2.5 px-4 rounded-xl bg-[#161e31] hover:bg-[#1d2740] text-neutral-200 font-bold text-xs transition flex items-center justify-center gap-2 border border-dark-border cursor-pointer shadow-sm"
                    >
                      <Info className="w-4 h-4 text-sky-400" />
                      <span>{t.aboutTitle}</span>
                    </button>
                  </div>
                </div>
              </div>
            )}

            {/* 5. HISTORY & LOGS TAB */}
            {sidebarTab === "history" && (
              <div className="h-full flex flex-col bg-[#080c14] border border-dark-border rounded-xl p-3 overflow-hidden">
                <div className="flex justify-between items-center pb-2 border-b border-dark-border text-xs font-semibold text-neutral-300">
                  <span>{lang === "tr" ? "Oturum Günlükleri & Olaylar" : "Session Event Log"}</span>
                  <span className="font-mono text-[10px] text-neutral-500">{new Date().toLocaleTimeString()}</span>
                </div>
                <div className="flex-1 overflow-y-auto font-mono text-[11px] text-neutral-400 p-2 flex flex-col gap-1.5">
                  <div className="text-emerald-400">
                    [{new Date().toLocaleTimeString()}] Engine initialized: Win32 Direct I/O & SIMD xxHash3 ready.
                  </div>
                  {destination && (
                    <div className="text-sky-400">
                      [{new Date().toLocaleTimeString()}] Workstation target configured: {destination}
                    </div>
                  )}
                  {stats.completed_files > 0 && (
                    <div className="text-neutral-300">
                      [{new Date().toLocaleTimeString()}] Verified transfer batch: {stats.completed_files} files written and parity verified.
                    </div>
                  )}
                  {stats.failed_files > 0 && (
                    <div className="text-rose-400">
                      [{new Date().toLocaleTimeString()}] Attention: {stats.failed_files} file(s) encountered locked state or access error.
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>

          {/* ── Bottom Overall Progress & Status Bar ── */}
          <div className="p-3 bg-[#080c14] border-t border-dark-border flex flex-col gap-2 flex-shrink-0">
            {/* Overall progress bar */}
            <div className="w-full h-2 bg-[#111624] rounded-full overflow-hidden p-0.5 border border-dark-border">
              <div
                className={`h-full rounded-full transition-all duration-300 ${
                  transferStatus === "canceled"
                    ? "bg-rose-500/80"
                    : transferStatus === "paused"
                    ? "bg-amber-500"
                    : "bg-gradient-to-r from-blue-600 via-sky-400 to-emerald-400"
                }`}
                style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
              />
            </div>

            <div className="flex items-center justify-between text-xs font-mono text-neutral-400">
              <div className="flex items-center gap-3">
                <span>
                  {t.transferred}: <strong className="text-neutral-100">{formatBytes(stats.transferred_bytes)}</strong> / {formatBytes(stats.total_bytes)}
                </span>
                <span>•</span>
                <span>
                  {t.eta}: <strong className="text-neutral-100">{formatEta(stats.eta_seconds, lang)}</strong>
                </span>
              </div>

              <div className="flex items-center gap-2">
                {stats.failed_files > 0 && (
                  <button
                    onClick={onRetryFailed}
                    className="flex items-center gap-1 px-2.5 py-1 rounded bg-rose-950 border border-rose-800 text-rose-300 text-xs font-medium transition cursor-pointer"
                  >
                    <RefreshCw className="w-3 h-3" />
                    <span>{t.retryFailed}</span>
                  </button>
                )}

                <span className="font-bold text-sky-400">
                  {stats.overall_percentage.toFixed(1)}%
                </span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
};
