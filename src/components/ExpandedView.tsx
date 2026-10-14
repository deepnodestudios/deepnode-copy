import React, { useState } from "react";
import {
  Play,
  Pause,
  X,
  Minimize2,
  RefreshCw,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  FileCheck,
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
  ArrowRight,
  ChevronUp,
  Clock,
  RotateCcw,
  Globe,
  Info,
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
  const t = translations[lang];
  const [filter, setFilter] = useState<"all" | "completed" | "remaining" | "skipped" | "failed">("all");
  const [activeTab, setActiveTab] = useState<"files" | "settings">("files");

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

  return (
    <div className="w-full h-full flex flex-col bg-neutral-900 border border-neutral-800 rounded-lg shadow-2xl overflow-hidden select-none">
      {/* Top Header */}
      <div className="flex items-center justify-between px-3.5 py-2.5 bg-neutral-950/80 border-b border-neutral-800">
        <div className="flex items-center gap-2">
          <FileCheck className="w-4 h-4 text-indigo-400" />
          <h2 className="text-xs font-semibold text-neutral-200">{t.appTitle}</h2>
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-indigo-950 border border-indigo-800 text-indigo-300 font-mono">
            v0.1.0-ultra
          </span>
        </div>

        <div className="flex items-center gap-1.5">
          {/* Language selector */}
          <div className="flex items-center bg-neutral-800 hover:bg-neutral-700/80 rounded px-1.5 py-0.5 border border-neutral-700/80 transition">
            <Globe className="w-3 h-3 text-neutral-400 mr-1 flex-shrink-0" />
            <select
              value={lang}
              onChange={(e) => onLangChange(e.target.value as Language)}
              className="bg-transparent text-[10px] font-mono text-neutral-200 focus:outline-none cursor-pointer"
              title={t.languageLabel}
            >
              {supportedLanguages.map((l) => (
                <option key={l.code} value={l.code} className="bg-neutral-900 text-neutral-200">
                  {l.flag} {l.nativeName} ({l.code})
                </option>
              ))}
            </select>
          </div>

          {/* Always on top toggle */}
          <button
            onClick={onToggleAlwaysOnTop}
            title={t.alwaysOnTop}
            className={`p-1 rounded text-xs transition ${
              alwaysOnTop
                ? "bg-indigo-600 text-white"
                : "bg-neutral-800 hover:bg-neutral-700 text-neutral-400"
            }`}
          >
            <Pin className="w-3.5 h-3.5" />
          </button>

          {/* Clipboard Verify */}
          <button
            onClick={onOpenVerify}
            title={t.clipboardTest}
            className="p-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-indigo-400 transition"
          >
            <ShieldCheck className="w-3.5 h-3.5" />
          </button>

          {/* About Dialog */}
          <button
            onClick={onOpenAbout}
            title={t.aboutTitle}
            className="p-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-indigo-400 transition"
          >
            <Info className="w-3.5 h-3.5" />
          </button>

          {/* Compact toggle */}
          <button
            onClick={onMinimize}
            title={t.compactMode}
            className="p-1 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-300 hover:text-indigo-400 transition"
          >
            <Minimize2 className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Station Control Panel or Active/Finished Transfer Header */}
      <div className="p-3 bg-neutral-950/50 border-b border-neutral-800 flex flex-col gap-2.5">
        {isTransferring || transferStatus === "completed" || transferStatus === "canceled" || transferStatus === "failed" ? (
          /* Active / Finished Transfer Header */
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between text-xs font-mono">
              <div className="flex items-center gap-1.5 min-w-0 flex-1">
                <span className="text-neutral-500 font-sans text-[11px] flex-shrink-0">{t.source}:</span>
                <span className="text-neutral-300 truncate max-w-[40%]" title={stagedSources[0] || ""}>
                  {stagedSources[0] ? stagedSources[0].split("\\").pop() || stagedSources[0] : "..."}
                </span>
                <ArrowRight className="w-3 h-3 text-neutral-600 flex-shrink-0" />
                <span className="text-neutral-500 font-sans text-[11px] flex-shrink-0">{t.destination}:</span>
                <span className="text-neutral-200 font-semibold truncate max-w-[40%]" title={destination}>
                  {destination ? destination.split("\\").pop() || destination : "..."}
                </span>
              </div>

              {/* Status Badge */}
              <div className="flex items-center gap-2">
                {transferStatus === "transferring" && (
                  <span className="text-indigo-400 font-bold ml-2 text-sm">
                    {stats.overall_percentage.toFixed(1)}%
                  </span>
                )}
                {transferStatus === "paused" && (
                  <span className="text-amber-400 font-semibold text-xs px-2 py-0.5 rounded bg-amber-950/80 border border-amber-800">
                    {t.statusPaused}
                  </span>
                )}
                {transferStatus === "completed" && (
                  <span className="text-emerald-400 font-semibold text-xs px-2 py-0.5 rounded bg-emerald-950/80 border border-emerald-800 flex items-center gap-1">
                    <CheckCircle2 className="w-3.5 h-3.5" />
                    <span>{t.statusFinished}</span>
                  </span>
                )}
                {transferStatus === "canceled" && (
                  <span className="text-rose-400 font-semibold text-xs px-2 py-0.5 rounded bg-rose-950/80 border border-rose-800 flex items-center gap-1">
                    <XCircle className="w-3.5 h-3.5" />
                    <span>{t.statusCanceled}</span>
                  </span>
                )}
                {transferStatus === "failed" && (
                  <span className="text-rose-400 font-semibold text-xs px-2 py-0.5 rounded bg-rose-950/80 border border-rose-800 flex items-center gap-1">
                    <AlertTriangle className="w-3.5 h-3.5" />
                    <span>{t.failed}</span>
                  </span>
                )}
              </div>
            </div>

            {/* Current file progress or Finished Summary */}
            {isTransferring && stats.current_file_size > 0 ? (
              <div className="flex flex-col gap-0.5">
                <div className="flex items-center justify-between text-[11px] text-neutral-400 font-mono">
                  <span className="truncate max-w-[70%]" title={stats.current_file_name}>
                    {stats.current_file_name}
                  </span>
                  <span>
                    {formatBytes(stats.current_file_transferred)} / {formatBytes(stats.current_file_size)}
                  </span>
                </div>
                <div className="w-full h-1.5 bg-neutral-800 rounded-full overflow-hidden p-0.5 border border-neutral-700/60">
                  <div
                    className="h-full bg-sky-400 rounded-full transition-all duration-100"
                    style={{
                      width: `${Math.min(100, Math.max(0, (stats.current_file_transferred / stats.current_file_size) * 100))}%`,
                    }}
                  />
                </div>
              </div>
            ) : transferStatus === "canceled" ? (
              <div className="flex items-center justify-between text-[11px] font-sans text-rose-300 bg-rose-950/30 px-2.5 py-1 rounded border border-rose-900/40">
                <span>{t.canceledSummary}</span>
                <span className="font-mono text-neutral-400">{formatBytes(stats.transferred_bytes)} / {formatBytes(stats.total_bytes)}</span>
              </div>
            ) : transferStatus === "completed" ? (
              <div className="flex items-center justify-between text-[11px] font-sans text-emerald-300 bg-emerald-950/30 px-2.5 py-1 rounded border border-emerald-900/40">
                <span>{t.completedSummary}</span>
                <span className="font-mono text-neutral-400">{stats.completed_files} {t.files} ({formatBytes(stats.transferred_bytes)})</span>
              </div>
            ) : null}
          </div>
        ) : (
          /* Source and Destination row when idle */
          <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
            {/* Add Files / Add Folder buttons */}
            <div className="flex items-center gap-1.5">
              <button
                onClick={onAddFiles}
                className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-indigo-600/80 hover:bg-indigo-500 text-white text-xs font-medium transition shadow-sm"
              >
                <FilePlus className="w-3.5 h-3.5" />
                <span>{t.addFiles}</span>
              </button>
              <button
                onClick={onAddFolder}
                className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-medium transition border border-neutral-700"
              >
                <FolderPlus className="w-3.5 h-3.5" />
                <span>{t.addFolder}</span>
              </button>
              {stagedSources.length > 0 && (
                <button
                  onClick={onClearStaged}
                  title={t.clearList}
                  className="p-1.5 rounded-lg bg-neutral-800 hover:bg-rose-900/60 text-neutral-400 hover:text-rose-300 transition"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                </button>
              )}
            </div>

            {/* Destination picker */}
            <div className="flex-1 flex items-center gap-1.5 min-w-0">
              <div className="relative flex-1">
                <input
                  type="text"
                  value={destination}
                  onChange={(e) => onChangeDestination(e.target.value)}
                  placeholder="Hedef Klasör (Örn: D:\Yedekler veya Gözat...)"
                  className="w-full pl-2.5 pr-2 py-1.5 rounded-lg bg-neutral-900 border border-neutral-700/80 text-xs font-mono text-neutral-200 placeholder-neutral-500 focus:outline-none focus:border-indigo-500"
                />
              </div>
              <button
                onClick={onBrowseDestination}
                className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-medium transition border border-neutral-700 flex-shrink-0"
              >
                <FolderOpen className="w-3.5 h-3.5 text-amber-400" />
                <span>{t.browseDestination}</span>
              </button>
            </div>

            {/* Execution Buttons: COPY & MOVE */}
            <div className="flex items-center gap-1.5">
              <button
                onClick={() => onStartTransfer("copy")}
                disabled={isTransferring || stagedSources.length === 0}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition shadow-md ${
                  isTransferring || stagedSources.length === 0
                    ? "bg-emerald-900/40 text-neutral-500 cursor-not-allowed"
                    : "bg-emerald-600 hover:bg-emerald-500 text-white shadow-emerald-950"
                }`}
              >
                <Zap className="w-3.5 h-3.5" />
                <span>{t.copy}</span>
              </button>
              <button
                onClick={() => onStartTransfer("move")}
                disabled={isTransferring || stagedSources.length === 0}
                className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition border ${
                  isTransferring || stagedSources.length === 0
                    ? "border-neutral-800 text-neutral-500 cursor-not-allowed"
                    : "border-amber-600/80 bg-amber-600/20 hover:bg-amber-600/30 text-amber-300"
                }`}
              >
                <Truck className="w-3.5 h-3.5" />
                <span>{t.move}</span>
              </button>
            </div>
          </div>
        )}

        {/* Quick parameters bar */}
        <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] font-mono text-neutral-400 pt-1 border-t border-neutral-800/60">
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1">
              <span className="text-neutral-500">{t.conflictPolicy}:</span>
              <select
                value={conflictPolicy}
                onChange={(e) => onChangeConflictPolicy(e.target.value)}
                className="bg-neutral-900 border border-neutral-700/80 rounded px-1.5 py-0.5 text-neutral-200 text-[11px] focus:outline-none"
              >
                <option value="auto_rename">{t.conflictRename}</option>
                <option value="older">{t.conflictOlder}</option>
                <option value="overwrite">{t.conflictOverwrite}</option>
                <option value="skip">{t.conflictSkip}</option>
              </select>
            </div>

            <div className="flex items-center gap-1">
              <span className="text-neutral-500">{t.hashAlgorithm}:</span>
              <select
                value={hashAlgorithm}
                onChange={(e) => onChangeHashAlgorithm(e.target.value)}
                className="bg-neutral-900 border border-neutral-700/80 rounded px-1.5 py-0.5 text-neutral-200 text-[11px] focus:outline-none"
              >
                <option value="xxhash3">xxHash3-64 (SIMD 30 GB/s)</option>
                <option value="blake3">BLAKE3 (SIMD)</option>
                <option value="sha256">SHA-256</option>
                <option value="md5">MD5</option>
              </select>
            </div>

            <div className="flex items-center gap-1">
              <span className="text-neutral-500">{t.verifyMode}:</span>
              <select
                value={verifyMode}
                onChange={(e) => onChangeVerifyMode(e.target.value)}
                className="bg-neutral-900 border border-neutral-700/80 rounded px-1.5 py-0.5 text-neutral-200 text-[11px] focus:outline-none"
              >
                <option value="always">{t.verifyAlways}</option>
                <option value="never">{t.verifyNever}</option>
              </select>
            </div>

            <label className="flex items-center gap-1.5 cursor-pointer text-neutral-300">
              <input
                type="checkbox"
                checked={directIo}
                onChange={(e) => onChangeDirectIo(e.target.checked)}
                className="rounded border-neutral-700 bg-neutral-900 text-indigo-600 focus:ring-0"
              />
              <span>Direct I/O</span>
            </label>
          </div>

          <div className="text-neutral-400">
            {stagedSources.length > 0 ? (
              <span className="text-indigo-300 font-semibold">
                {t.selectedItems}: {stagedSources.length} ({stagedPreview ? stagedPreview.total_files : 0} {t.files}, {stagedTotalMb} MB)
              </span>
            ) : (
              <span className="text-neutral-500">{t.noFilesQueued}</span>
            )}
          </div>
        </div>
      </div>

      {/* Speed Chart & Metrics (Active during transfer or keeps history) */}
      <div className="p-3 bg-neutral-900 border-b border-neutral-800 flex flex-col gap-2">
        <SpeedChart speedHistory={speedHistory} />

        <div className="grid grid-cols-4 gap-2 text-xs font-mono text-neutral-400">
          <div className="bg-neutral-950/50 p-2 rounded border border-neutral-800/80">
            <span className="text-[10px] text-neutral-400 block">{t.speed}</span>
            <strong className="text-neutral-100 text-xs">
              {transferStatus === "canceled" ? "0.0 MB/s" : `${speedMb} MB/s`}
            </strong>
            <span className="text-[10px] text-neutral-400 block">Avg: {avgSpeedMb} MB/s</span>
          </div>

          <div className="bg-neutral-950/50 p-2 rounded border border-neutral-800/80">
            <span className="text-[10px] text-neutral-400 block">{t.transferred}</span>
            <strong className="text-neutral-100 text-xs">{formatBytes(stats.transferred_bytes)}</strong>
            <span className="text-[10px] text-neutral-400 block">Total: {formatBytes(stats.total_bytes)}</span>
          </div>

          <div className="bg-neutral-950/50 p-2 rounded border border-neutral-800/80">
            <span className="text-[10px] text-neutral-400 block">{t.eta}</span>
            <strong className="text-neutral-100 text-xs">
              {transferStatus === "canceled" ? "0 sn" : formatEta(stats.eta_seconds, lang)}
            </strong>
            <span className="text-[10px] text-neutral-400 block">{stats.overall_percentage.toFixed(1)}%</span>
          </div>

          <div className="bg-neutral-950/50 p-2 rounded border border-neutral-800/80">
            <span className="text-[10px] text-neutral-400 block">{t.files}</span>
            <strong className="text-neutral-100 text-xs">
              {stats.completed_files} / {stats.total_files}
            </strong>
            <span className="text-[10px] text-rose-400 block">
              Failed: {stats.failed_files}
            </span>
          </div>
        </div>

        {/* Overall progress bar */}
        <div className="w-full h-2.5 bg-neutral-800 rounded-full overflow-hidden p-0.5 border border-neutral-700/50 mt-1">
          <div
            className={`h-full rounded-full transition-all duration-300 ${
              transferStatus === "canceled"
                ? "bg-rose-500/80"
                : transferStatus === "paused"
                ? "bg-amber-500"
                : "bg-gradient-to-r from-indigo-500 via-sky-400 to-emerald-400"
            }`}
            style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
          />
        </div>
      </div>

      {/* Navigation Tabs (Files / Staged / Settings) */}
      <div className="flex items-center justify-between px-3 py-1.5 bg-neutral-950/60 border-b border-neutral-800">
        <div className="flex items-center gap-1">
          <button
            onClick={() => setActiveTab("files")}
            className={`px-3 py-1 rounded text-xs font-medium transition ${
              activeTab === "files"
                ? "bg-neutral-800 text-neutral-100 border border-neutral-700"
                : "text-neutral-400 hover:text-neutral-200"
            }`}
          >
            {isTransferring || files.length > 0 ? `${t.transferTab} (${files.length})` : `${t.stagedTab} (${stagedSources.length})`}
          </button>
          <button
            onClick={() => setActiveTab("settings")}
            className={`flex items-center gap-1 px-3 py-1 rounded text-xs font-medium transition ${
              activeTab === "settings"
                ? "bg-neutral-800 text-neutral-100 border border-neutral-700"
                : "text-neutral-400 hover:text-neutral-200"
            }`}
          >
            <Settings className="w-3.5 h-3.5" />
            <span>{t.settingsTab}</span>
          </button>
        </div>

        {activeTab === "files" && files.length > 0 && (
          <div className="flex items-center gap-1 text-[11px]">
            <button
              onClick={() => setFilter("all")}
              className={`px-2 py-0.5 rounded transition ${
                filter === "all" ? "bg-indigo-600 text-white" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {t.all}
            </button>
            <button
              onClick={() => setFilter("completed")}
              className={`px-2 py-0.5 rounded transition ${
                filter === "completed" ? "bg-emerald-700 text-white" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {t.completed} ({stats.completed_files})
            </button>
            <button
              onClick={() => setFilter("remaining")}
              className={`px-2 py-0.5 rounded transition ${
                filter === "remaining" ? "bg-sky-700 text-white" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {t.remainingFilter} ({remainingFilesCount})
            </button>
            <button
              onClick={() => setFilter("skipped")}
              className={`px-2 py-0.5 rounded transition ${
                filter === "skipped" ? "bg-amber-700 text-white" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {t.skipped} ({stats.skipped_files})
            </button>
            <button
              onClick={() => setFilter("failed")}
              className={`px-2 py-0.5 rounded transition ${
                filter === "failed" ? "bg-rose-700 text-white" : "text-neutral-400 hover:text-neutral-200"
              }`}
            >
              {t.failed} ({stats.failed_files})
            </button>
          </div>
        )}
      </div>

      {/* Main Body */}
      <div className="flex-1 overflow-y-auto p-2 bg-neutral-900/50">
        {activeTab === "files" ? (
          <div>
            {/* If transfer has files */}
            {files.length > 0 ? (
              <div className="flex flex-col gap-1">
                {filteredFiles.map((item) => (
                  <div
                    key={item.id}
                    className="flex items-center justify-between p-2 rounded bg-neutral-950/60 border border-neutral-800/80 hover:border-neutral-700 text-xs transition"
                  >
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      {item.status === "completed" && (
                        <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0" />
                      )}
                      {item.status === "copying" && (
                        <RefreshCw className="w-4 h-4 text-indigo-400 animate-spin flex-shrink-0" />
                      )}
                      {item.status === "verifying" && (
                        <ShieldCheck className="w-4 h-4 text-sky-400 animate-pulse flex-shrink-0" />
                      )}
                      {item.status === "pending" && (
                        <Clock className="w-4 h-4 text-neutral-500 flex-shrink-0" />
                      )}
                      {item.status === "skipped" && (
                        <AlertTriangle className="w-4 h-4 text-amber-400 flex-shrink-0" />
                      )}
                      {item.status === "failed" && (
                        <XCircle className="w-4 h-4 text-rose-400 flex-shrink-0" />
                      )}
                      {item.status === "canceled" && (
                        <XCircle className="w-4 h-4 text-rose-400 flex-shrink-0" />
                      )}
                      <div className="flex flex-col min-w-0 flex-1">
                        <span className="truncate font-mono text-neutral-200">
                          {item.relative_path || item.src_path}
                        </span>
                        {(item.status === "copying" || item.status === "canceled") && (item.bytes_transferred || 0) > 0 && (
                          <div className="w-full mt-1.5 pr-4">
                            <div className="flex items-center justify-between text-[10px] text-neutral-400 mb-0.5 font-mono">
                              <span>{formatBytes(item.bytes_transferred || 0)} / {formatBytes(item.size)}</span>
                              <span>
                                {item.size > 0 ? Math.min(100, Math.round(((item.bytes_transferred || 0) / item.size) * 100)) : 0}%
                                {item.status === "canceled" ? ` (${t.statusCanceled})` : ""}
                              </span>
                            </div>
                            <div className="w-full h-1.5 bg-neutral-800 rounded-full overflow-hidden">
                              <div
                                className={`h-full rounded-full transition-all duration-100 ${
                                  item.status === "canceled"
                                    ? "bg-rose-500/80"
                                    : "bg-gradient-to-r from-indigo-500 to-sky-400"
                                }`}
                                style={{
                                  width: `${item.size > 0 ? Math.min(100, Math.round(((item.bytes_transferred || 0) / item.size) * 100)) : 0}%`,
                                }}
                              />
                            </div>
                          </div>
                        )}
                        {item.error_message && (
                          <span className="text-[10px] text-rose-400 truncate">
                            {item.error_message}
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-3 text-[11px] font-mono pl-2">
                      <span className="text-neutral-400">{formatBytes(item.size)}</span>
                      <span className={`uppercase text-[10px] font-semibold ${
                        item.status === "completed" ? "text-emerald-400" :
                        item.status === "copying" ? "text-indigo-400" :
                        item.status === "verifying" ? "text-sky-400" :
                        item.status === "failed" ? "text-rose-400" :
                        item.status === "canceled" ? "text-rose-400" :
                        item.status === "skipped" ? "text-amber-400" :
                        "text-neutral-500"
                      }`}>
                        {item.status === "completed" ? t.completed :
                         item.status === "copying" ? t.statusTransferring :
                         item.status === "verifying" ? t.statusVerifying :
                         item.status === "failed" ? t.failed :
                         item.status === "canceled" ? t.statusCanceled :
                         item.status === "skipped" ? t.skipped :
                         t.statusReady}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            ) : stagedSources.length > 0 ? (
              /* If files are staged in queue */
              <div className="flex flex-col gap-1">
                {stagedSources.map((path, idx) => (
                  <div
                    key={idx}
                    className="flex items-center justify-between p-2 rounded bg-neutral-950/60 border border-neutral-800/80 hover:border-neutral-700 text-xs transition"
                  >
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      <FolderOpen className="w-4 h-4 text-indigo-400 flex-shrink-0" />
                      <span className="truncate font-mono text-neutral-200">{path}</span>
                    </div>
                    <button
                      onClick={() => onRemoveStaged(idx)}
                      title="Kaldır"
                      className="p-1 rounded text-neutral-500 hover:text-rose-400 hover:bg-neutral-800 transition"
                    >
                      <X className="w-3.5 h-3.5" />
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              /* Completely Empty State: Drag and drop zone */
              <div className="h-full flex flex-col items-center justify-center py-6 px-4 text-center border-2 border-dashed border-neutral-800/80 rounded-xl m-2 bg-neutral-950/20">
                <FolderPlus className="w-10 h-10 text-neutral-600 mb-3" />
                <h3 className="text-xs font-semibold text-neutral-300 mb-1">{t.dragDropHint}</h3>
                <p className="text-[11px] text-neutral-500 max-w-sm mb-4">
                  Dosyaları sürükleyip bırakabilir, yukarıdaki butonlarla ekleyebilir veya Windows 11 sağ tık menüsünden "DeepNode Copy..." ile başlatabilirsiniz.
                </p>
                <div className="flex items-center gap-2">
                  <button
                    onClick={onAddFiles}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium transition"
                  >
                    <FilePlus className="w-3.5 h-3.5" />
                    <span>{t.addFiles}</span>
                  </button>
                  <button
                    onClick={onAddFolder}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-medium transition border border-neutral-700"
                  >
                    <FolderPlus className="w-3.5 h-3.5" />
                    <span>{t.addFolder}</span>
                  </button>
                </div>
              </div>
            )}
          </div>
        ) : (
          /* Settings Tab */
          <div className="p-3 bg-neutral-950/40 rounded-lg border border-neutral-800 flex flex-col gap-3 text-xs">
            <h3 className="font-semibold text-neutral-200 text-xs border-b border-neutral-800 pb-2">
              {t.transferSettings}
            </h3>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              <div>
                <label className="text-[11px] text-neutral-400 block mb-1">{t.languageLabel}</label>
                <select
                  value={lang}
                  onChange={(e) => onLangChange(e.target.value as Language)}
                  className="w-full bg-neutral-900 border border-neutral-700 rounded p-1.5 text-neutral-200 text-xs"
                >
                  {supportedLanguages.map((l) => (
                    <option key={l.code} value={l.code}>
                      {l.flag} {l.nativeName} ({l.englishName})
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="text-[11px] text-neutral-400 block mb-1">{t.onFinish}</label>
                <select
                  value={onFinishAction}
                  onChange={(e) => onChangeOnFinish(e.target.value)}
                  className="w-full bg-neutral-900 border border-neutral-700 rounded p-1.5 text-neutral-200 text-xs"
                >
                  <option value="do_nothing">{t.doNothing}</option>
                  <option value="close_app">{t.closeApp}</option>
                  <option value="sleep">{t.sleep}</option>
                  <option value="shutdown">{t.shutdown}</option>
                </select>
              </div>

              <div>
                <label className="text-[11px] text-neutral-400 block mb-1">{t.conflictPolicy}</label>
                <select
                  value={conflictPolicy}
                  onChange={(e) => onChangeConflictPolicy(e.target.value)}
                  className="w-full bg-neutral-900 border border-neutral-700 rounded p-1.5 text-neutral-200 text-xs"
                >
                  <option value="auto_rename">{t.conflictRename}</option>
                  <option value="older">{t.conflictOlder}</option>
                  <option value="overwrite">{t.conflictOverwrite}</option>
                  <option value="skip">{t.conflictSkip}</option>
                </select>
              </div>
            </div>

            <div className="pt-2 border-t border-neutral-800/80 flex flex-col sm:flex-row gap-2">
              <button
                onClick={onRegisterContextMenu}
                className="flex-1 py-2 px-3 rounded-lg bg-emerald-600/80 hover:bg-emerald-500 text-white font-medium text-xs transition flex items-center justify-center gap-2 cursor-pointer shadow-sm"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                <span>{t.registerContextMenu}</span>
              </button>

              <button
                onClick={onOpenAbout}
                className="py-2 px-4 rounded-lg bg-neutral-800 hover:bg-neutral-700 text-neutral-200 font-medium text-xs transition flex items-center justify-center gap-2 border border-neutral-700/80 cursor-pointer shadow-sm"
              >
                <Info className="w-3.5 h-3.5 text-indigo-400" />
                <span>{t.aboutTitle}</span>
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Bottom Footer Controls */}
      <div className="p-2.5 bg-neutral-950/80 border-t border-neutral-800 flex items-center justify-between">
        <div className="flex items-center gap-2">
          {stats.failed_files > 0 && (
            <button
              onClick={onRetryFailed}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-950 border border-rose-800 text-rose-300 hover:bg-rose-900 text-xs font-medium transition"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              <span>{t.retryFailed}</span>
            </button>
          )}

          {transferStatus === "canceled" && (
            <button
              onClick={() => onStartTransfer("copy")}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-indigo-600/80 hover:bg-indigo-500 text-white text-xs font-semibold transition"
            >
              <RotateCcw className="w-3.5 h-3.5" />
              <span>{t.restart}</span>
            </button>
          )}
        </div>

        <div className="flex items-center gap-2">
          {isTransferring && (
            <button
              onClick={onPauseToggle}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-amber-600 hover:bg-amber-500 text-white text-xs font-semibold transition"
            >
              {isPaused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
              <span>{isPaused ? t.resume : t.pause}</span>
            </button>
          )}

          {isTransferring && (
            <button
              onClick={onCancel}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-neutral-800 hover:bg-rose-700 text-neutral-200 hover:text-white text-xs font-semibold transition border border-neutral-700"
            >
              <X className="w-3.5 h-3.5" />
              <span>{t.cancel}</span>
            </button>
          )}

          {(transferStatus === "completed" || transferStatus === "canceled" || transferStatus === "failed") && (
            <>
              {onResetToIdle && (
                <button
                  onClick={onResetToIdle}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-medium transition border border-neutral-700"
                >
                  <FolderPlus className="w-3.5 h-3.5 text-neutral-400" />
                  <span>{t.newTransfer}</span>
                </button>
              )}

              {onCloseApp && (
                <button
                  onClick={onCloseApp}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-600/90 hover:bg-rose-500 text-white text-xs font-semibold transition"
                >
                  <span>{t.close}</span>
                </button>
              )}
            </>
          )}

          <button
            onClick={onMinimize}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 hover:text-white text-xs font-medium transition border border-neutral-700"
            title={t.less}
          >
            <ChevronUp className="w-3.5 h-3.5 text-neutral-400" />
            <span>{t.less}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
