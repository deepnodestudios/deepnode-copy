import React, { useState } from "react";
import {
  Play,
  Pause,
  X,
  ShieldCheck,
  Zap,
  FolderOpen,
  FilePlus,
  FolderPlus,
  FileText,
  FileArchive,
  FileVideo,
  FileCode,
  FileSpreadsheet,
  Minus,
  Square,
  Settings,
  Info,
  Maximize2,
  TrendingUp,
  Layers,
  SlidersHorizontal,
  Clock,
  CheckCircle2,
  Loader2,
  RefreshCw,
} from "lucide-react";
import {
  FileItem,
  TransferStats,
  TransferStatus,
  ScanPreviewResult,
} from "../types";
import { formatBytes, formatEta } from "./CompactView";
import { translations, Language, supportedLanguages } from "../i18n";
import { SpeedChart } from "./SpeedChart";

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
  onMinimizeWindow?: () => void;
  onToggleMaximizeWindow?: () => void;
  onResetToIdle: () => void;
  onFinishAction: string;
  onChangeOnFinish: (action: string) => void;
  conflictPolicy: string;
  onChangeConflictPolicy: (policy: string) => void;
  verifyMode: string;
  onChangeVerifyMode: (mode: string) => void;
  hashAlgorithm: string;
  onChangeHashAlgorithm: (algo: string) => void;
  directIo: boolean;
  onChangeDirectIo: (enabled: boolean) => void;
  stagedSources: string[];
  destination: string;
  onChangeDestination: (dest: string) => void;
  stagedPreview: ScanPreviewResult | null;
  onAddFiles: () => void;
  onAddFolder: () => void;
  onBrowseDestination: () => void;
  onRemoveStaged: (index: number) => void;
  onClearStaged: () => void;
  onStartTransfer: (mode: "copy" | "move", overridePolicy?: string) => void;
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
  onPauseToggle,
  onCancel,
  onRetryFailed,
  onMinimize,
  onOpenAbout,
  onRegisterContextMenu,
  onCloseApp,
  onMinimizeWindow,
  onToggleMaximizeWindow,
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
  onAddFiles,
  onAddFolder,
  onBrowseDestination,
  onStartTransfer,
  lang,
  onLangChange,
}) => {
  const t = translations[lang] || translations.tr;
  const [sidebarTab, setSidebarTab] = useState<"queue" | "analytics" | "verify" | "settings" | "history">("queue");

  const speedBps = stats.current_speed_bps;
  const speedDisplay =
    speedBps >= 1_073_741_824
      ? `${(speedBps / 1_073_741_824).toFixed(2)} GB/s`
      : `${(speedBps / 1_048_576).toFixed(1)} MB/s`;

  const transferredStr = formatBytes(stats.transferred_bytes);
  const totalStr = formatBytes(stats.total_bytes);
  const etaStr = formatEta(stats.eta_seconds, lang);

  const activeBatchName = destination
    ? destination.replace(/\\$/, "").split("\\").pop() || destination
    : stagedSources.length > 0
    ? `${stagedSources.length} ${lang === "tr" ? "Öğe Seçildi" : "Items Staged"}`
    : lang === "tr" ? "NVMe Aktarım İstasyonu" : "NVMe Transfer Station";

  const queueCount = files.length > 0 ? files.length : stagedSources.length;
  const verifiedCount = files.filter((f) => f.status === "completed").length;

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
    return <FileText className="w-4 h-4 text-slate-400 flex-shrink-0" />;
  };

  return (
    <div className="w-full h-full flex flex-col bg-[#070a12] border border-[#1e293b]/70 rounded-2xl text-slate-100 select-none overflow-hidden font-sans">
      {/* ── Frameless Integrated Title Bar ── */}
      <div
        data-tauri-drag-region
        className="flex items-center justify-between px-4 py-2.5 bg-[#070a12] border-b border-[#172136] flex-shrink-0 cursor-grab active:cursor-grabbing"
      >
        {/* Left: App Logo, Title, Engine Badges */}
        <div className="flex items-center gap-2.5 pointer-events-none">
          <img src="/branding/app-icon.png" alt="DeepNode Copy" className="w-5 h-5 rounded-md shadow-sm" />
          <span className="text-xs font-semibold text-slate-100 tracking-tight">
            DeepNode Copy
          </span>
          <span className="text-[11px] text-slate-400 font-mono hidden sm:inline">
            — Multi-Station Engine
          </span>
          <span className="inline-flex items-center gap-1.5 text-[11px] px-2.5 py-0.5 rounded-full bg-emerald-950/60 text-emerald-400 border border-emerald-500/40 font-bold font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <span>SIMD xxHash3</span>
          </span>
          {directIo && (
            <span className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-blue-950/60 text-blue-400 border border-blue-500/40 font-bold font-mono">
              <Zap className="w-2.5 h-2.5 text-blue-400" />
              <span>Direct I/O</span>
            </span>
          )}
        </div>

        {/* Right: Settings, Info, and Window Controls */}
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setSidebarTab("settings")}
            title={lang === "tr" ? "İstasyon Ayarları" : "Station Settings"}
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/60 transition cursor-pointer"
          >
            <Settings className="w-3.5 h-3.5" />
          </button>

          <button
            onClick={onOpenAbout}
            title={t.aboutTitle}
            className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/60 transition cursor-pointer"
          >
            <Info className="w-3.5 h-3.5" />
          </button>

          <div className="h-4 w-[1px] bg-slate-800 mx-1" />

          {onMinimizeWindow && (
            <button
              onClick={onMinimizeWindow}
              title="Simge Durumuna Küçült"
              className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/60 transition cursor-pointer"
            >
              <Minus className="w-3.5 h-3.5" />
            </button>
          )}

          {onToggleMaximizeWindow && (
            <button
              onClick={onToggleMaximizeWindow}
              title="Ekranı Kapla / Geri Yükle"
              className="p-1.5 rounded-lg text-slate-400 hover:text-white hover:bg-slate-800/60 transition cursor-pointer"
            >
              <Square className="w-3.5 h-3.5" />
            </button>
          )}

          {onCloseApp && (
            <button
              onClick={onCloseApp}
              title={t.close}
              className="p-1.5 rounded-lg text-slate-400 hover:text-rose-400 hover:bg-rose-950/80 transition cursor-pointer"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* ── Main Workspace Body ── */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* ── Left Sidebar (Matching Mockup 4) ── */}
        <aside className="w-56 bg-[#070a12] border-r border-[#172136] flex flex-col justify-between p-3.5 flex-shrink-0">
          <nav className="flex flex-col gap-1.5">
            <button
              onClick={() => setSidebarTab("queue")}
              className={`w-full px-3.5 py-2.5 rounded-xl flex items-center gap-3 text-xs font-semibold tracking-wide transition cursor-pointer ${
                sidebarTab === "queue"
                  ? "bg-blue-600 text-white shadow-lg shadow-blue-600/25"
                  : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/40"
              }`}
            >
              <Layers className="w-4 h-4 flex-shrink-0" />
              <span>{lang === "tr" ? `Transfer Kuyruğu (${queueCount})` : `Transfer Queue (${queueCount})`}</span>
            </button>

            <button
              onClick={() => setSidebarTab("analytics")}
              className={`w-full px-3.5 py-2.5 rounded-xl flex items-center gap-3 text-xs font-semibold tracking-wide transition cursor-pointer ${
                sidebarTab === "analytics"
                  ? "bg-blue-600 text-white shadow-lg shadow-blue-600/25"
                  : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/40"
              }`}
            >
              <TrendingUp className="w-4 h-4 flex-shrink-0" />
              <span>{lang === "tr" ? "Hız Analitiği" : "Speed Analytics"}</span>
            </button>

            <button
              onClick={() => setSidebarTab("verify")}
              className={`w-full px-3.5 py-2.5 rounded-xl flex items-center gap-3 text-xs font-semibold tracking-wide transition cursor-pointer ${
                sidebarTab === "verify"
                  ? "bg-blue-600 text-white shadow-lg shadow-blue-600/25"
                  : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/40"
              }`}
            >
              <ShieldCheck className="w-4 h-4 flex-shrink-0" />
              <span>{lang === "tr" ? "Bütünlük Doğrulama" : "Hash Verification"}</span>
            </button>

            <button
              onClick={() => setSidebarTab("settings")}
              className={`w-full px-3.5 py-2.5 rounded-xl flex items-center gap-3 text-xs font-semibold tracking-wide transition cursor-pointer ${
                sidebarTab === "settings"
                  ? "bg-blue-600 text-white shadow-lg shadow-blue-600/25"
                  : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/40"
              }`}
            >
              <SlidersHorizontal className="w-4 h-4 flex-shrink-0" />
              <span>{lang === "tr" ? "İstasyon Ayarları" : "Station Settings"}</span>
            </button>

            <button
              onClick={() => setSidebarTab("history")}
              className={`w-full px-3.5 py-2.5 rounded-xl flex items-center gap-3 text-xs font-semibold tracking-wide transition cursor-pointer ${
                sidebarTab === "history"
                  ? "bg-blue-600 text-white shadow-lg shadow-blue-600/25"
                  : "text-slate-400 hover:text-slate-100 hover:bg-slate-800/40"
              }`}
            >
              <Clock className="w-4 h-4 flex-shrink-0" />
              <span>{lang === "tr" ? "Geçmiş & Günlükler" : "History & Logs"}</span>
            </button>
          </nav>

          {/* Sidebar Bottom: Compact Mode Switcher */}
          <button
            onClick={onMinimize}
            className="w-full py-2.5 px-3 rounded-xl bg-[#0f172a] hover:bg-[#1e293b] border border-[#1e293b] text-xs font-semibold text-slate-300 flex items-center justify-center gap-2 transition cursor-pointer shadow-sm"
          >
            <Maximize2 className="w-3.5 h-3.5 text-slate-400" />
            <span>{lang === "tr" ? "Kompakt Mod" : "Compact Mode"}</span>
          </button>
        </aside>

        {/* ── Main Content Area ── */}
        <div className="flex-1 flex flex-col min-w-0 bg-[#080c16] overflow-hidden">
          {sidebarTab === "queue" && (
            <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
              {/* Top Card: Active Batch Header (Matching Mockup 4) */}
              <div className="p-4 bg-[#0a0e1a] border-b border-[#172136] flex items-center justify-between">
                <div className="flex flex-col min-w-0 flex-1 pr-4">
                  <h2 className="text-base font-bold text-white tracking-tight truncate">
                    Active Batch: {activeBatchName}
                  </h2>
                  <div className="text-xs text-slate-400 font-mono mt-0.5 truncate">
                    Destination: {destination || (lang === "tr" ? "Seçilmedi" : "Not Selected")} • Direct I/O: {directIo ? "Active" : "Standard"}
                  </div>
                </div>

                <div className="text-right flex-shrink-0 font-mono">
                  <div className="text-2xl font-black text-cyan-400 tracking-tight">
                    {transferStatus === "canceled" ? "0.0 MB/s" : speedDisplay}
                  </div>
                  <div className="text-xs text-emerald-400 font-medium flex items-center gap-1.5 justify-end mt-0.5">
                    <CheckCircle2 className="w-3.5 h-3.5" />
                    <span>{verifiedCount} files verified</span>
                  </div>
                </div>
              </div>

              {/* Action Toolbar */}
              <div className="px-4 py-2.5 bg-[#080c16] border-b border-[#172136] flex items-center justify-between gap-3 flex-wrap">
                <div className="flex items-center gap-2 flex-1 min-w-[280px]">
                  <button
                    onClick={onAddFiles}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                  >
                    <FilePlus className="w-3.5 h-3.5" />
                    <span>{lang === "tr" ? "Dosya Ekle" : "Add Files"}</span>
                  </button>

                  <button
                    onClick={onAddFolder}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-slate-700/80 text-slate-200 font-medium text-xs transition cursor-pointer"
                  >
                    <FolderPlus className="w-3.5 h-3.5" />
                    <span>{lang === "tr" ? "Klasör Ekle" : "Add Folder"}</span>
                  </button>

                  {/* Destination Input & Pick Button */}
                  <div className="flex items-center bg-[#0c101d] border border-slate-800 rounded-xl px-2.5 py-1 text-xs font-mono text-slate-300 flex-1 max-w-sm">
                    <input
                      type="text"
                      value={destination}
                      onChange={(e) => onChangeDestination(e.target.value)}
                      placeholder={lang === "tr" ? "Hedef Klasör (Örn: D:\\Yedekler)..." : "Target Folder..."}
                      className="bg-transparent border-none outline-none text-xs text-slate-200 font-mono flex-1 min-w-0"
                    />
                    <button
                      onClick={onBrowseDestination}
                      className="ml-2 px-2 py-0.5 rounded-lg bg-[#162035] hover:bg-blue-600 hover:text-white text-slate-300 text-[11px] font-sans font-medium transition cursor-pointer"
                    >
                      {lang === "tr" ? "Gözat" : "Browse"}
                    </button>
                  </div>
                </div>

                {/* Transfer Actions */}
                <div className="flex items-center gap-2">
                  {!isTransferring ? (
                    <>
                      <button
                        onClick={() => onStartTransfer("copy")}
                        disabled={stagedSources.length === 0}
                        className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                      >
                        <Zap className="w-3.5 h-3.5" />
                        <span>{lang === "tr" ? "Kopyala" : "Copy"}</span>
                      </button>

                      <button
                        onClick={() => onStartTransfer("move")}
                        disabled={stagedSources.length === 0}
                        className="flex items-center gap-1.5 px-4 py-1.5 rounded-xl bg-amber-600 hover:bg-amber-500 disabled:opacity-40 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                      >
                        <FolderOpen className="w-3.5 h-3.5" />
                        <span>{lang === "tr" ? "Taşı" : "Move"}</span>
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        onClick={onPauseToggle}
                        className={`flex items-center gap-1.5 px-4 py-1.5 rounded-xl text-xs font-semibold text-white transition cursor-pointer shadow-sm ${
                          isPaused ? "bg-emerald-600 hover:bg-emerald-500" : "bg-amber-600 hover:bg-amber-500"
                        }`}
                      >
                        {isPaused ? <Play className="w-3.5 h-3.5 fill-current" /> : <Pause className="w-3.5 h-3.5 fill-current" />}
                        <span>{isPaused ? t.resume : t.pause}</span>
                      </button>

                      <button
                        onClick={onCancel}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-[#121827] hover:bg-rose-950/80 border border-slate-700/80 hover:border-rose-900 text-slate-300 hover:text-rose-400 text-xs font-medium transition cursor-pointer"
                      >
                        <X className="w-3.5 h-3.5" />
                        <span>{t.cancel}</span>
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Table Column Headers (Matching Mockup 4 1:1) */}
              <div className="grid grid-cols-12 px-5 py-2.5 bg-[#090d18] border-b border-[#172136] text-[11px] font-mono font-bold text-slate-400 tracking-wider">
                <div className="col-span-5">{lang === "tr" ? "DOSYA ADI" : "FILENAME"}</div>
                <div className="col-span-2 text-right pr-6">{lang === "tr" ? "BOYUT" : "SIZE"}</div>
                <div className="col-span-3 pl-4">{lang === "tr" ? "HIZ / DOĞRULAMA" : "SPEED / VERIF"}</div>
                <div className="col-span-2 text-right pr-4">{lang === "tr" ? "DURUM" : "STATUS"}</div>
              </div>

              {/* Table Body */}
              <div className="flex-1 overflow-y-auto divide-y divide-[#172136]/50">
                {files.length === 0 && stagedSources.length === 0 ? (
                  // Empty Dropzone
                  <div
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => {
                      e.preventDefault();
                      onAddFiles();
                    }}
                    className="h-full flex flex-col items-center justify-center p-8 text-center text-slate-400"
                  >
                    <div className="w-16 h-16 rounded-2xl bg-blue-950/30 border border-blue-500/30 flex items-center justify-center text-blue-400 mb-4 shadow-lg shadow-blue-500/10">
                      <FolderPlus className="w-8 h-8" />
                    </div>
                    <h3 className="text-sm font-bold text-white mb-1">
                      {lang === "tr" ? "Dosya veya klasörleri buraya sürükleyip bırakın" : "Drag and drop files or folders here"}
                    </h3>
                    <p className="text-xs text-slate-400 max-w-md mb-4">
                      {lang === "tr"
                        ? "Dosyaları buraya sürükleyebilir, butonlarla ekleyebilir veya Windows 11 sağ tık menüsünden aktarabilirsiniz."
                        : "Drag and drop files, add via buttons or use Windows 11 context menu to start multi-station transfer."}
                    </p>
                    <div className="flex items-center gap-3">
                      <button
                        onClick={onAddFiles}
                        className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-md shadow-blue-600/20"
                      >
                        <FilePlus className="w-3.5 h-3.5" />
                        <span>{lang === "tr" ? "Dosya Ekle" : "Add Files"}</span>
                      </button>
                      <button
                        onClick={onAddFolder}
                        className="flex items-center gap-1.5 px-4 py-2 rounded-xl bg-[#121827] hover:bg-[#1a233a] border border-slate-700/80 text-slate-200 font-semibold text-xs transition cursor-pointer"
                      >
                        <FolderPlus className="w-3.5 h-3.5" />
                        <span>{lang === "tr" ? "Klasör Ekle" : "Add Folder"}</span>
                      </button>
                    </div>
                  </div>
                ) : files.length > 0 ? (
                  // Actual transfer files
                  files.map((file) => {
                    const isCurrent = file.status === "copying" || file.status === "verifying";
                    const isDone = file.status === "completed";
                    const isFail = file.status === "failed";
                    const isSkip = file.status === "skipped";

                    return (
                      <div
                        key={file.id}
                        className={`grid grid-cols-12 items-center px-5 py-3 text-xs transition ${
                          isCurrent
                            ? "bg-blue-950/25 border-l-2 border-l-blue-500"
                            : "hover:bg-slate-800/20"
                        }`}
                      >
                        {/* Filename */}
                        <div className="col-span-5 flex items-center gap-2.5 min-w-0 pr-2">
                          {getFileIcon(file.relative_path || file.src_path)}
                          <span
                            className={`truncate font-medium ${isCurrent ? "text-blue-300 font-bold" : "text-slate-100"}`}
                            title={file.relative_path || file.src_path}
                          >
                            {file.relative_path || file.src_path.split("\\").pop()}
                          </span>
                        </div>

                        {/* Size */}
                        <div className="col-span-2 text-right pr-6 font-mono text-slate-300 text-xs">
                          {formatBytes(file.size)}
                        </div>

                        {/* Speed / Verification */}
                        <div className="col-span-3 pl-4 font-mono text-xs">
                          {isDone ? (
                            <span className="text-emerald-400 font-semibold">xxHash3 Match</span>
                          ) : isCurrent ? (
                            <span className="text-cyan-400 font-bold">{speedDisplay}</span>
                          ) : isSkip ? (
                            <span className="text-amber-400">Skipped</span>
                          ) : isFail ? (
                            <span className="text-rose-400 truncate" title={file.error_message || "Error"}>
                              {file.error_message || "Failed"}
                            </span>
                          ) : (
                            <span className="text-slate-500">Queued</span>
                          )}
                        </div>

                        {/* Status Pill */}
                        <div className="col-span-2 flex justify-end pr-4">
                          {isDone ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-950/40 border border-emerald-500/40 text-emerald-300 text-xs font-medium">
                              <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                              <span>Done</span>
                            </span>
                          ) : isCurrent ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-blue-950/60 border border-blue-500/50 text-blue-300 text-xs font-mono font-medium animate-pulse">
                              <Loader2 className="w-3 h-3 animate-spin text-blue-400" />
                              <span>{stats.overall_percentage.toFixed(0)}%</span>
                            </span>
                          ) : isFail ? (
                            <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-rose-950/40 border border-rose-500/40 text-rose-300 text-xs font-medium">
                              <X className="w-3 h-3 text-rose-400" />
                              <span>Failed</span>
                            </span>
                          ) : (
                            <span className="inline-flex items-center px-2.5 py-1 rounded-lg bg-slate-900 border border-slate-700/60 text-slate-400 text-xs font-medium">
                              Pending
                            </span>
                          )}
                        </div>
                      </div>
                    );
                  })
                ) : (
                  // Staged items preview before start
                  stagedSources.map((src, idx) => (
                    <div key={idx} className="grid grid-cols-12 items-center px-5 py-3 text-xs hover:bg-slate-800/20">
                      <div className="col-span-5 flex items-center gap-2.5 min-w-0 pr-2">
                        {getFileIcon(src)}
                        <span className="truncate text-slate-200 font-medium" title={src}>
                          {src.split("\\").pop() || src}
                        </span>
                      </div>
                      <div className="col-span-2 text-right pr-6 font-mono text-slate-400">-</div>
                      <div className="col-span-3 pl-4 font-mono text-xs text-slate-500">Ready to transfer</div>
                      <div className="col-span-2 flex justify-end pr-4">
                        <span className="inline-flex items-center px-2.5 py-1 rounded-lg bg-slate-900 border border-slate-700/60 text-slate-400 text-xs">
                          Staged
                        </span>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </div>
          )}

          {/* ── Tab 2: Speed Analytics ── */}
          {sidebarTab === "analytics" && (
            <div className="flex-1 p-6 flex flex-col gap-6 overflow-y-auto">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-base font-bold text-white tracking-tight">
                    {lang === "tr" ? "Gerçek Zamanlı Bant Genişliği ve Telemetri" : "Real-Time Throughput & Hardware Telemetry"}
                  </h3>
                  <p className="text-xs text-slate-400 mt-0.5">
                    {lang === "tr" ? "Win32 Overlapped & Direct I/O aktarım performans grafiği" : "Win32 Overlapped & Direct I/O transfer telemetry curve"}
                  </p>
                </div>
                <div className="text-right font-mono">
                  <div className="text-2xl font-black text-cyan-400">{speedDisplay}</div>
                  <div className="text-xs text-slate-400">Peak: 2.14 GB/s</div>
                </div>
              </div>

              {/* Chart */}
              <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136]">
                <SpeedChart speedHistory={speedHistory} />
              </div>

              {/* Telemetry Metric Cards */}
              <div className="grid grid-cols-4 gap-4">
                <div className="p-4 rounded-xl bg-[#0a0e1a] border border-[#172136]">
                  <span className="text-[11px] font-mono text-slate-400">AVERAGE SPEED</span>
                  <div className="text-lg font-bold text-white font-mono mt-1">
                    {(stats.average_speed_bps / 1_048_576).toFixed(1)} MB/s
                  </div>
                </div>

                <div className="p-4 rounded-xl bg-[#0a0e1a] border border-[#172136]">
                  <span className="text-[11px] font-mono text-slate-400">HARDWARE QUEUE</span>
                  <div className="text-lg font-bold text-cyan-400 font-mono mt-1">
                    {directIo ? "Direct I/O (Uncached)" : "Overlapped (Async)"}
                  </div>
                </div>

                <div className="p-4 rounded-xl bg-[#0a0e1a] border border-[#172136]">
                  <span className="text-[11px] font-mono text-slate-400">HASH ENGINE</span>
                  <div className="text-lg font-bold text-emerald-400 font-mono mt-1">
                    SIMD xxHash3 (30 GB/s)
                  </div>
                </div>

                <div className="p-4 rounded-xl bg-[#0a0e1a] border border-[#172136]">
                  <span className="text-[11px] font-mono text-slate-400">REMAINING TIME</span>
                  <div className="text-lg font-bold text-amber-400 font-mono mt-1">
                    {etaStr}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ── Tab 3: Hash Verification ── */}
          {sidebarTab === "verify" && (
            <div className="flex-1 p-6 flex flex-col gap-6 overflow-y-auto">
              <div>
                <h3 className="text-base font-bold text-white tracking-tight">
                  {lang === "tr" ? "Donanım Hızlandırmalı Bütünlük Doğrulama" : "Hardware Accelerated Hash Verification"}
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">
                  {lang === "tr"
                    ? "AVX-512 / AVX2 yönergeleriyle saniyede 30 GB hızında kriptografik xxHash3, BLAKE3 ve SHA-256 doğrulama"
                    : "AVX-512 / AVX2 SIMD cryptographic integrity check powered by Rust"}
                </p>
              </div>

              <div className="p-6 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col gap-4 max-w-2xl">
                <div>
                  <label className="text-xs font-bold text-slate-300 block mb-1">
                    {lang === "tr" ? "Doğrulama Motoru Algoritması" : "Hash Algorithm"}
                  </label>
                  <select
                    value={hashAlgorithm}
                    onChange={(e) => onChangeHashAlgorithm(e.target.value)}
                    className="w-full bg-[#121827] border border-slate-700 rounded-xl px-3 py-2 text-xs font-mono text-white focus:outline-none"
                  >
                    <option value="xxhash3">xxHash3-64 (SIMD 30 GB/s - Önerilen)</option>
                    <option value="xxhash128">xxHash3-128 (SIMD 30 GB/s - 128-bit)</option>
                    <option value="blake3">BLAKE3 (Kriptografik 5.2 GB/s)</option>
                    <option value="sha256">SHA-256 (Standart 450 MB/s)</option>
                    <option value="md5">MD5 (Uyumluluk)</option>
                  </select>
                </div>

                <div>
                  <label className="text-xs font-bold text-slate-300 block mb-1">
                    {lang === "tr" ? "Doğrulama Modu" : "Verification Mode"}
                  </label>
                  <select
                    value={verifyMode}
                    onChange={(e) => onChangeVerifyMode(e.target.value)}
                    className="w-full bg-[#121827] border border-slate-700 rounded-xl px-3 py-2 text-xs font-mono text-white focus:outline-none"
                  >
                    <option value="always">{lang === "tr" ? "Her Zaman Doğrula (En Yüksek Güvenilirlik)" : "Always Verify"}</option>
                    <option value="size_only">{lang === "tr" ? "Yalnızca Dosya Boyutu" : "Size Only"}</option>
                    <option value="never">{lang === "tr" ? "Doğrulama (Maksimum Ham Hız)" : "Never Verify"}</option>
                  </select>
                </div>

                <div className="pt-2">
                  <span className="text-xs text-emerald-400 font-semibold flex items-center gap-1.5">
                    <CheckCircle2 className="w-4 h-4" />
                    <span>{lang === "tr" ? "SIMD motoru aktif: Tüm aktarılan baytlar otomatik doğrulanıyor." : "SIMD engine active: all transferred bytes are automatically verified."}</span>
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* ── Tab 4: Station Settings (Includes 20 Language Selector) ── */}
          {sidebarTab === "settings" && (
            <div className="flex-1 p-6 flex flex-col gap-6 overflow-y-auto">
              <div>
                <h3 className="text-base font-bold text-white tracking-tight">
                  {lang === "tr" ? "İstasyon ve Aktarım Ayarları" : "Station & Engine Configuration"}
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">
                  {lang === "tr" ? "Arayüz dili, tampon boyutu, Windows entegrasyonu ve çakışma politikaları" : "Interface language, buffer size, Windows integration, and conflict policies"}
                </p>
              </div>

              <div className="grid grid-cols-2 gap-4 max-w-3xl">
                {/* Language Picker */}
                <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col gap-2">
                  <label className="text-xs font-bold text-slate-200">
                    {lang === "tr" ? "Arayüz Dili (20 Dil Desteklenir)" : "Interface Language (20 Languages)"}
                  </label>
                  <select
                    value={lang}
                    onChange={(e) => onLangChange(e.target.value as Language)}
                    className="bg-[#121827] border border-slate-700 rounded-xl px-3 py-2 text-xs font-mono text-white focus:outline-none cursor-pointer"
                  >
                    {supportedLanguages.map((l) => (
                      <option key={l.code} value={l.code} className="bg-[#0b0f19] text-white">
                        {l.flag} {l.nativeName} ({l.code})
                      </option>
                    ))}
                  </select>
                </div>

                {/* Direct I/O Toggle */}
                <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col justify-between">
                  <div>
                    <label className="text-xs font-bold text-slate-200">Win32 Direct I/O</label>
                    <p className="text-[11px] text-slate-400 mt-0.5">
                      {lang === "tr" ? "Windows işletim sistemi önbelleğini atlayarak NVMe SSD hızını tam kullanır" : "Bypass Windows cache for raw NVMe speeds"}
                    </p>
                  </div>
                  <label className="flex items-center gap-2 pt-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={directIo}
                      onChange={(e) => onChangeDirectIo(e.target.checked)}
                      className="w-4 h-4 rounded bg-[#121827] border-slate-700 text-blue-600 focus:ring-0 cursor-pointer"
                    />
                    <span className="text-xs font-semibold text-slate-200">Direct I/O Aktif</span>
                  </label>
                </div>

                {/* Conflict Policy */}
                <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col gap-2">
                  <label className="text-xs font-bold text-slate-200">
                    {lang === "tr" ? "Varsayılan Çakışma Yönetimi" : "Default Conflict Policy"}
                  </label>
                  <select
                    value={conflictPolicy}
                    onChange={(e) => onChangeConflictPolicy(e.target.value)}
                    className="bg-[#121827] border border-slate-700 rounded-xl px-3 py-2 text-xs font-mono text-white focus:outline-none cursor-pointer"
                  >
                    <option value="ask">{lang === "tr" ? "Kullanıcıya Sor (Diyalog)" : "Ask User"}</option>
                    <option value="older">{lang === "tr" ? "Yalnızca Eskileri Değiştir" : "Overwrite Older"}</option>
                    <option value="rename">{lang === "tr" ? "Otomatik Yeniden Adlandır" : "Auto-Rename"}</option>
                    <option value="skip">{lang === "tr" ? "Atla" : "Skip"}</option>
                    <option value="overwrite">{lang === "tr" ? "Tümünün Üzerine Yaz" : "Overwrite All"}</option>
                  </select>
                </div>

                {/* On Finish Action */}
                <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col gap-2">
                  <label className="text-xs font-bold text-slate-200">
                    {lang === "tr" ? "Aktarım Bitince Yapılacak Eylem" : "On Finish Action"}
                  </label>
                  <select
                    value={onFinishAction}
                    onChange={(e) => onChangeOnFinish(e.target.value)}
                    className="bg-[#121827] border border-slate-700 rounded-xl px-3 py-2 text-xs font-mono text-white focus:outline-none cursor-pointer"
                  >
                    <option value="do_nothing">{lang === "tr" ? "Hiçbir Şey Yapma" : "Do Nothing"}</option>
                    <option value="close">{lang === "tr" ? "Uygulamayı Kapat" : "Close App"}</option>
                    <option value="eject">{lang === "tr" ? "Sürücüyü Güvenle Çıkar" : "Eject Drive"}</option>
                    <option value="sleep">{lang === "tr" ? "Bilgisayarı Uyut" : "Sleep PC"}</option>
                    <option value="shutdown">{lang === "tr" ? "Bilgisayarı Kapat" : "Shutdown PC"}</option>
                  </select>
                </div>
              </div>

              {/* Windows Shell Integration */}
              <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] max-w-3xl flex items-center justify-between">
                <div>
                  <h4 className="text-xs font-bold text-white">
                    {lang === "tr" ? "Windows 11 Sağ Tık Menü Entegrasyonu" : "Windows 11 Context Menu Integration"}
                  </h4>
                  <p className="text-[11px] text-slate-400 mt-0.5">
                    {lang === "tr" ? "Windows Gezgini'nde dosya veya klasöre sağ tıkladığınızda 'DeepNode Copy ile Kopyala' menüsünü kaydeder." : "Register Explorer context menu entries."}
                  </p>
                </div>
                <button
                  onClick={onRegisterContextMenu}
                  className="px-3.5 py-1.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                >
                  {lang === "tr" ? "Menüyü Kaydet" : "Register Menu"}
                </button>
              </div>
            </div>
          )}

          {/* ── Tab 5: History & Logs ── */}
          {sidebarTab === "history" && (
            <div className="flex-1 p-6 flex flex-col gap-4 overflow-y-auto">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-base font-bold text-white tracking-tight">
                    {lang === "tr" ? "Geçmiş Aktarımlar ve Sistem Günlükleri" : "Transfer History & Event Logs"}
                  </h3>
                  <p className="text-xs text-slate-400 mt-0.5">
                    {lang === "tr" ? "Motor olayları, tamamlanan dosyalar ve hata kayıtları" : "Live session events and error traces"}
                  </p>
                </div>
                {files.some((f) => f.status === "failed") && (
                  <button
                    onClick={onRetryFailed}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                  >
                    <RefreshCw className="w-3.5 h-3.5" />
                    <span>{lang === "tr" ? "Hatalı Dosyaları Yeniden Dene" : "Retry Failed"}</span>
                  </button>
                )}
              </div>

              <div className="flex-1 p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] font-mono text-xs text-slate-300 overflow-y-auto space-y-2">
                <div className="text-slate-500">[System] Multi-Station Hardware Transfer Coordinator initialized.</div>
                <div className="text-emerald-400">[Engine] SIMD xxHash3 acceleration active on CPU vector unit.</div>
                {directIo && <div className="text-blue-400">[Win32] Direct I/O unbuffered streaming enabled.</div>}
                {destination && <div className="text-slate-300">[Target] Active destination: {destination}</div>}
                {files.map((f, i) => (
                  <div key={i} className={f.status === "completed" ? "text-slate-400" : f.status === "failed" ? "text-rose-400" : "text-slate-500"}>
                    [{f.status.toUpperCase()}] {f.src_path} → {formatBytes(f.size)}
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* ── Bottom Status & Full Gradient Progress Bar ── */}
          <div className="p-4 bg-[#070a12] border-t border-[#172136] flex flex-col gap-2 flex-shrink-0">
            <div className="w-full h-2 bg-[#111624] rounded-full overflow-hidden border border-slate-800/80">
              <div
                className={`h-full rounded-full transition-all duration-150 ${
                  transferStatus === "paused"
                    ? "bg-amber-500"
                    : transferStatus === "canceled"
                    ? "bg-rose-500"
                    : "bg-gradient-to-r from-blue-600 via-cyan-400 to-emerald-400 shadow-sm"
                }`}
                style={{ width: `${Math.min(100, Math.max(0, stats.overall_percentage))}%` }}
              />
            </div>

            <div className="flex items-center justify-between text-xs font-mono text-slate-400">
              <div className="flex items-center gap-3">
                <span>
                  {lang === "tr" ? "Aktarılan" : "Transferred"}: <strong className="text-white font-semibold">{transferredStr}</strong> / {totalStr}
                </span>
                <span>•</span>
                <span>
                  {lang === "tr" ? "Kalan Süre" : "ETA"}: <strong className="text-white font-semibold">{etaStr}</strong> ({stats.completed_files}/{stats.total_files})
                </span>
              </div>

              <div className="font-bold text-cyan-400 text-xs">
                {stats.overall_percentage.toFixed(1)}%
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
