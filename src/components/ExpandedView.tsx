import React, { useState, useEffect } from "react";
import {
  Play,
  Pause,
  X,
  ShieldCheck,
  Zap,
  FolderOpen,
  Folder,
  Plus,
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
  Trash2,
  SkipForward,
  RotateCw,
  ExternalLink,
  Copy,
  CheckSquare,
  Eraser,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FileItem,
  TransferStats,
  TransferStatus,
  ScanPreviewResult,
} from "../types";
import { formatBytes, formatEta } from "./CompactView";
import { translations, Language, supportedLanguages } from "../i18n";
import { SpeedChart } from "./SpeedChart";
import { HistoryView } from "./HistoryView";

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
  onSkipCurrent?: () => void;
  onSkipFiles?: (ids: number[]) => void;
  onRemoveQueued?: (ids: number[]) => void;
  onShowInFolder?: (path: string) => void;
  onOpenFolder?: (path: string) => void;
  onRetryFailed: () => void;
  onMinimize: () => void;
  onToggleAlwaysOnTop: () => void;
  onOpenVerify: () => void;
  onOpenAbout: () => void;
  onRegisterContextMenu: () => void;
  onUnregisterContextMenu?: () => void;
  isContextMenuRegistered?: boolean;
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
  rememberPopupChoice: boolean;
  onChangeRememberPopupChoice: (enabled: boolean) => void;
  stagedSources: string[];
  destination: string;
  onChangeDestination: (dest: string) => void;
  stagedPreview: ScanPreviewResult | null;
  onAddFiles: () => void;
  onAddFolder: () => void;
  onBrowseDestination: () => void;
  onRemoveStaged: (indices: number[] | number) => void;
  onClearStaged: () => void;
  onStartTransfer: (mode: "copy" | "move", overridePolicy?: string) => void;
  lang: Language;
  onLangChange: (lang: Language) => void;
  isDraggingOver?: boolean;
  onRerunSession?: (sources: string[], destination: string) => void;
}

export const getFileIcon = (filename: string) => {
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

export interface FolderGroup {
  id: string;
  name: string;
  path: string;
  files: FileItem[];
  totalBytes: number;
  transferredBytes: number;
  status: "pending" | "copying" | "verifying" | "completed" | "failed" | "skipped";
}

interface FileRowItemProps {
  file: FileItem;
  isSelected: boolean;
  isCurrent: boolean;
  isInsideFolder?: boolean;
  speedDisplay: string;
  currentFileTransferred?: number;
  overallPercentage?: number;
  lang?: Language;
  onToggleSelect: (id: number, e: React.MouseEvent | React.ChangeEvent) => void;
  onRangeSelect: (id: number) => void;
  onSingleSelect: (id: number) => void;
  onShowInFolder?: (path: string) => void;
  onContextMenu: (e: React.MouseEvent, file: FileItem) => void;
}

const areRowPropsEqual = (prev: FileRowItemProps, next: FileRowItemProps) => {
  if (prev.lang !== next.lang) return false;
  if (prev.file !== next.file) return false;
  if (prev.isSelected !== next.isSelected) return false;
  if (prev.isCurrent !== next.isCurrent) return false;
  if (prev.isInsideFolder !== next.isInsideFolder) return false;
  if (!next.isCurrent) return true;
  const prevPct = prev.file.size > 0 ? ((prev.currentFileTransferred || 0) / prev.file.size) * 100 : 0;
  const nextPct = next.file.size > 0 ? ((next.currentFileTransferred || 0) / next.file.size) * 100 : 0;
  return (
    prevPct.toFixed(0) === nextPct.toFixed(0) &&
    prev.speedDisplay === next.speedDisplay
  );
};

const FileRowItem = React.memo<FileRowItemProps>(
  ({
    file,
    isSelected,
    isCurrent,
    isInsideFolder = false,
    speedDisplay,
    currentFileTransferred,
    lang = "tr",
    onToggleSelect,
    onRangeSelect,
    onSingleSelect,
    onShowInFolder,
    onContextMenu,
  }) => {
    const isDone = file.status === "completed";
    const isFail = file.status === "failed";
    const isSkip = file.status === "skipped";

    const filePercentage = isDone
      ? 100
      : isCurrent && file.size > 0
      ? Math.min(
          100,
          Math.max(
            0,
            (((currentFileTransferred !== undefined && currentFileTransferred > 0
              ? currentFileTransferred
              : file.bytes_transferred) || 0) /
              file.size) *
              100
          )
        )
      : file.size > 0 && file.bytes_transferred > 0
      ? Math.min(100, (file.bytes_transferred / file.size) * 100)
      : 0;

    return (
      <div
        onClick={(e) => {
          if (e.ctrlKey || e.metaKey) {
            onToggleSelect(file.id, e);
          } else if (e.shiftKey) {
            onRangeSelect(file.id);
          } else {
            onSingleSelect(file.id);
          }
        }}
        onDoubleClick={() => {
          if (onShowInFolder) onShowInFolder(file.src_path);
        }}
        onContextMenu={(e) => onContextMenu(e, file)}
        className={`grid grid-cols-12 items-center px-5 py-2 text-xs transition cursor-pointer select-none ${
          isSelected
            ? "bg-blue-900/35 border-l-2 border-l-blue-400"
            : isCurrent
            ? "bg-blue-950/25 border-l-2 border-l-blue-500"
            : "hover:bg-slate-800/20 border-l-2 border-l-transparent"
        }`}
      >
        {/* Filename with Checkbox */}
        <div className={`col-span-5 flex items-center gap-2.5 min-w-0 pr-2 ${isInsideFolder ? "pl-7" : "pl-6"}`}>
          <input
            type="checkbox"
            checked={isSelected}
            onChange={(e) => onToggleSelect(file.id, e)}
            onClick={(e) => e.stopPropagation()}
            className="rounded border-slate-700 bg-slate-900 text-blue-600 focus:ring-0 cursor-pointer flex-shrink-0"
          />
          {getFileIcon(file.relative_path || file.src_path)}
          <span
            className={`truncate font-medium ${
              isSelected
                ? "text-white font-semibold"
                : isCurrent
                ? "text-blue-300 font-bold"
                : "text-slate-200"
            }`}
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
        <div className="col-span-2 pl-4 font-mono text-xs">
          {isDone ? (
            <span className="text-emerald-400 font-semibold">xxHash3 Match</span>
          ) : isCurrent ? (
            <span className="text-cyan-400 font-bold">{speedDisplay}</span>
          ) : isSkip ? (
            <span className="text-amber-400">{lang === "tr" ? "Atlandı" : "Skipped"}</span>
          ) : isFail ? (
            <span className="text-rose-400 truncate" title={file.error_message || "Error"}>
              {file.error_message || (lang === "tr" ? "Başarısız" : "Failed")}
            </span>
          ) : (
            <span className="text-slate-500">{lang === "tr" ? "Kuyrukta" : "Queued"}</span>
          )}
        </div>

        {/* Progress Bar with Percentage */}
        <div className="col-span-3 flex items-center justify-end gap-2.5 pr-4">
          <div className="flex-1 max-w-[150px] bg-slate-800/80 rounded-full h-2 overflow-hidden border border-slate-700/50 p-[1px]">
            <div
              className={`h-full rounded-full transition-all duration-150 ${
                isDone
                  ? "w-full bg-gradient-to-r from-emerald-500 to-teal-400 shadow-sm shadow-emerald-500/20"
                  : isCurrent
                  ? "bg-gradient-to-r from-blue-500 via-cyan-400 to-teal-300 shadow-sm shadow-cyan-500/30"
                  : isFail
                  ? "w-full bg-rose-500"
                  : isSkip
                  ? "w-full bg-amber-500"
                  : "w-0 bg-slate-600"
              }`}
              style={{
                width: isDone || isFail || isSkip ? "100%" : isCurrent ? `${filePercentage.toFixed(0)}%` : "0%",
              }}
            />
          </div>
          <div className="w-12 text-right font-mono text-[11px] font-semibold flex items-center justify-end gap-1 flex-shrink-0">
            {isDone ? (
              <span className="text-emerald-400 flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                <span>100%</span>
              </span>
            ) : isCurrent ? (
              <span className="text-cyan-300">{filePercentage.toFixed(0)}%</span>
            ) : isFail ? (
              <span className="text-rose-400 text-[10px]">{lang === "tr" ? "Hata" : "Fail"}</span>
            ) : isSkip ? (
              <span className="text-amber-400 text-[10px]">{lang === "tr" ? "Atla" : "Skip"}</span>
            ) : (
              <span className="text-slate-500">0%</span>
            )}
          </div>
        </div>
      </div>
    );
  },
  areRowPropsEqual
);

interface FolderRowItemProps {
  group: FolderGroup;
  isCollapsed: boolean;
  selectedCount: number;
  isCurrent: boolean;
  speedDisplay: string;
  onToggleExpand: (folderId: string) => void;
  onToggleSelectFolder: (group: FolderGroup, e: React.MouseEvent | React.ChangeEvent) => void;
  onContextMenu: (e: React.MouseEvent, group: FolderGroup) => void;
  onShowInFolder?: (path: string) => void;
  lang: Language;
}

const areFolderPropsEqual = (prev: FolderRowItemProps, next: FolderRowItemProps) => {
  if (prev.lang !== next.lang) return false;
  if (prev.group.id !== next.group.id) return false;
  if (prev.isCollapsed !== next.isCollapsed) return false;
  if (prev.selectedCount !== next.selectedCount) return false;
  if (prev.group.files.length !== next.group.files.length) return false;
  if (prev.group.status !== next.group.status) return false;
  if (prev.isCurrent !== next.isCurrent) return false;
  if (!next.isCurrent) return true;

  const prevPct = prev.group.totalBytes > 0 ? (prev.group.transferredBytes / prev.group.totalBytes) * 100 : 0;
  const nextPct = next.group.totalBytes > 0 ? (next.group.transferredBytes / next.group.totalBytes) * 100 : 0;
  return (
    prevPct.toFixed(0) === nextPct.toFixed(0) &&
    prev.speedDisplay === next.speedDisplay
  );
};

const FolderRowItem = React.memo<FolderRowItemProps>(
  ({
    group,
    isCollapsed,
    selectedCount,
    isCurrent,
    speedDisplay,
    onToggleExpand,
    onToggleSelectFolder,
    onContextMenu,
    onShowInFolder,
    lang,
  }) => {
    const checkboxRef = React.useRef<HTMLInputElement>(null);
    const totalCount = group.files.length;
    const allSelected = totalCount > 0 && selectedCount === totalCount;
    const someSelected = selectedCount > 0 && !allSelected;

    React.useEffect(() => {
      if (checkboxRef.current) {
        checkboxRef.current.indeterminate = someSelected;
      }
    }, [someSelected]);

    const completedCount = group.files.filter((f) => f.status === "completed").length;
    const isDone = group.status === "completed";
    const isFail = group.status === "failed";
    const isSkip = group.status === "skipped";

    const folderPercentage =
      group.totalBytes > 0
        ? Math.min(100, Math.max(0, (group.transferredBytes / group.totalBytes) * 100))
        : isDone
        ? 100
        : 0;

    return (
      <div
        onDoubleClick={() => {
          if (onShowInFolder) onShowInFolder(group.path);
          else onToggleExpand(group.id);
        }}
        onContextMenu={(e) => onContextMenu(e, group)}
        className={`grid grid-cols-12 items-center px-5 py-2.5 text-xs transition cursor-pointer select-none bg-[#0a0f1d] border-b border-[#172136] hover:bg-[#11192e] ${
          isCurrent ? "border-l-2 border-l-cyan-400 bg-[#0d162b]" : "border-l-2 border-l-transparent"
        }`}
      >
        {/* Col 1-5: + / - button, Checkbox, Folder Icon, Name, Count */}
        <div className="col-span-5 flex items-center gap-2 min-w-0 pr-2">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleExpand(group.id);
            }}
            className="w-4 h-4 rounded border border-slate-600/80 bg-slate-800/90 hover:bg-slate-700 hover:border-slate-500 flex items-center justify-center text-slate-200 hover:text-white transition cursor-pointer flex-shrink-0 shadow-sm"
            title={isCollapsed ? (lang === "tr" ? "Klasörü genişlet" : "Expand folder") : (lang === "tr" ? "Klasörü daralt" : "Collapse folder")}
          >
            {isCollapsed ? <Plus className="w-3 h-3" /> : <Minus className="w-3 h-3" />}
          </button>

          <input
            ref={checkboxRef}
            type="checkbox"
            checked={allSelected}
            onChange={(e) => onToggleSelectFolder(group, e)}
            onClick={(e) => e.stopPropagation()}
            className="rounded border-slate-700 bg-slate-900 text-blue-600 focus:ring-0 cursor-pointer flex-shrink-0"
          />

          {isCollapsed ? (
            <Folder className="w-4 h-4 text-amber-400 flex-shrink-0" />
          ) : (
            <FolderOpen className="w-4 h-4 text-amber-400 flex-shrink-0" />
          )}

          <span
            className="truncate font-semibold text-slate-100 hover:text-cyan-300 transition"
            title={group.path}
            onClick={(e) => {
              e.stopPropagation();
              onToggleExpand(group.id);
            }}
          >
            {group.name}
          </span>

          <span className="text-[10px] text-slate-400 font-mono flex-shrink-0 px-1.5 py-0.5 rounded bg-slate-800/60 border border-slate-700/50">
            {completedCount}/{totalCount}
          </span>
        </div>

        {/* Col 6-7: Size */}
        <div className="col-span-2 text-right pr-6 font-mono text-slate-300 text-xs font-medium">
          {formatBytes(group.totalBytes)}
        </div>

        {/* Col 8-9: Speed / Verification */}
        <div className="col-span-2 pl-4 font-mono text-xs">
          {isDone ? (
            <span className="text-emerald-400 font-semibold">{lang === "tr" ? "Tamamlandı" : "Completed"}</span>
          ) : isCurrent ? (
            <span className="text-cyan-400 font-bold">{speedDisplay}</span>
          ) : isFail ? (
            <span className="text-rose-400 font-semibold">{lang === "tr" ? "Hata" : "Failed"}</span>
          ) : isSkip ? (
            <span className="text-amber-400 font-semibold">{lang === "tr" ? "Atlandı" : "Skipped"}</span>
          ) : (
            <span className="text-slate-500 font-mono">0 B/s</span>
          )}
        </div>

        {/* Col 10-12: Progress Bar with Percentage */}
        <div className="col-span-3 flex items-center justify-end gap-2.5 pr-4">
          <div className="flex-1 max-w-[150px] bg-slate-800/80 rounded-full h-2.5 overflow-hidden border border-slate-700/50 p-[1px]">
            <div
              className={`h-full rounded-full transition-all duration-150 ${
                isDone
                  ? "w-full bg-gradient-to-r from-emerald-500 to-teal-400 shadow-sm shadow-emerald-500/20"
                  : isCurrent
                  ? "bg-gradient-to-r from-blue-500 via-cyan-400 to-teal-300 shadow-sm shadow-cyan-500/30"
                  : isFail
                  ? "w-full bg-rose-500"
                  : isSkip
                  ? "w-full bg-amber-500"
                  : "w-0 bg-slate-600"
              }`}
              style={{
                width: isDone || isFail || isSkip ? "100%" : isCurrent ? `${folderPercentage.toFixed(0)}%` : "0%",
              }}
            />
          </div>
          <div className="w-12 text-right font-mono text-[11px] font-semibold flex items-center justify-end gap-1 flex-shrink-0">
            {isDone ? (
              <span className="text-emerald-400 flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                <span>100%</span>
              </span>
            ) : isCurrent ? (
              <span className="text-cyan-300">{folderPercentage.toFixed(0)}%</span>
            ) : isFail ? (
              <span className="text-rose-400 text-[10px]">{lang === "tr" ? "Hata" : "Fail"}</span>
            ) : isSkip ? (
              <span className="text-amber-400 text-[10px]">{lang === "tr" ? "Atla" : "Skip"}</span>
            ) : (
              <span className="text-slate-500">0%</span>
            )}
          </div>
        </div>
      </div>
    );
  },
  areFolderPropsEqual
);

export const ExpandedView: React.FC<ExpandedViewProps> = ({
  stats,
  files,
  speedHistory,
  isPaused,
  isTransferring,
  transferStatus,
  onPauseToggle,
  onCancel,
  onSkipCurrent,
  onSkipFiles,
  onRemoveQueued,
  onShowInFolder,
  onOpenFolder,
  onRetryFailed,
  onMinimize,
  onOpenAbout,
  onRegisterContextMenu,
  onUnregisterContextMenu,
  isContextMenuRegistered,
  onCloseApp,
  onMinimizeWindow,
  onToggleMaximizeWindow,
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
  rememberPopupChoice,
  onChangeRememberPopupChoice,
  stagedSources,
  destination,
  onChangeDestination,
  onAddFiles,
  onAddFolder,
  onBrowseDestination,
  onRemoveStaged,
  onClearStaged,
  onStartTransfer,
  lang,
  onLangChange,
  isDraggingOver,
  onRerunSession,
}) => {
  const t = translations[lang] || translations.tr;
  const [sidebarTab, setSidebarTab] = useState<"queue" | "analytics" | "verify" | "settings" | "history">("queue");

  const handleRerunSessionInternal = (sources: string[], dest: string) => {
    if (onRerunSession) {
      onRerunSession(sources, dest);
    }
    setSidebarTab("queue");
  };
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    item: FileItem | null;
  } | null>(null);
  const [propertiesModalItem, setPropertiesModalItem] = useState<FileItem | null>(null);

  const filesRef = React.useRef(files);
  filesRef.current = files;
  const stagedRef = React.useRef(stagedSources);
  stagedRef.current = stagedSources;
  const lastAnchorIdRef = React.useRef<number | null>(null);
  const visibleFileIdsRef = React.useRef<number[]>([]);

  const handleToggleSelect = React.useCallback((id: number, e: React.MouseEvent | React.ChangeEvent) => {
    e.stopPropagation();
    lastAnchorIdRef.current = id;
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleSingleSelect = React.useCallback((id: number) => {
    lastAnchorIdRef.current = id;
    setSelectedIds(new Set([id]));
  }, []);

  const handleRangeSelect = React.useCallback((id: number) => {
    const visibleIds = visibleFileIdsRef.current;
    if (visibleIds.length === 0) return;

    setSelectedIds((prev) => {
      let anchor = lastAnchorIdRef.current;
      if (anchor === null || !visibleIds.includes(anchor)) {
        const prevArray = Array.from(prev);
        for (let i = prevArray.length - 1; i >= 0; i--) {
          if (visibleIds.includes(prevArray[i])) {
            anchor = prevArray[i];
            break;
          }
        }
      }

      if (anchor !== null && visibleIds.includes(anchor)) {
        const from = visibleIds.indexOf(anchor);
        const to = visibleIds.indexOf(id);
        if (from !== -1 && to !== -1) {
          const [min, max] = from < to ? [from, to] : [to, from];
          const range = visibleIds.slice(min, max + 1);
          return new Set(range);
        }
      }

      lastAnchorIdRef.current = id;
      return new Set([id]);
    });
  }, []);

  const handleRowContextMenu = React.useCallback((e: React.MouseEvent, file: FileItem) => {
    e.preventDefault();
    e.stopPropagation();
    lastAnchorIdRef.current = file.id;
    setSelectedIds((prev) => {
      if (!prev.has(file.id)) {
        return new Set([file.id]);
      }
      return prev;
    });
    setContextMenu({ x: e.clientX, y: e.clientY, item: file });
  }, []);

  const [collapsedFolders, setCollapsedFolders] = useState<Set<string>>(new Set());

  const handleToggleFolderExpand = React.useCallback((folderId: string) => {
    setCollapsedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderId)) {
        next.delete(folderId);
      } else {
        next.add(folderId);
      }
      return next;
    });
  }, []);

  const handleToggleFolderSelect = React.useCallback(
    (group: FolderGroup, e: React.MouseEvent | React.ChangeEvent) => {
      e.stopPropagation();
      setSelectedIds((prev) => {
        const next = new Set(prev);
        const allSelected = group.files.length > 0 && group.files.every((f) => next.has(f.id));
        if (allSelected) {
          group.files.forEach((f) => next.delete(f.id));
        } else {
          group.files.forEach((f) => next.add(f.id));
        }
        return next;
      });
    },
    []
  );

  const handleFolderContextMenu = React.useCallback(
    (e: React.MouseEvent, group: FolderGroup) => {
      e.preventDefault();
      e.stopPropagation();
      setSelectedIds((prev) => {
        const next = new Set(prev);
        group.files.forEach((f) => next.add(f.id));
        return next;
      });

      const firstFile = group.files[0];
      setContextMenu({
        x: e.clientX,
        y: e.clientY,
        item: firstFile || {
          id: -1,
          src_path: group.path,
          dst_path: destination ? `${destination}\\${group.name}` : "",
          relative_path: group.name,
          size: group.totalBytes,
          bytes_transferred: group.transferredBytes,
          status: group.status,
          retry_attempts: 0,
        },
      });
    },
    [destination]
  );

  const { folderGroups, standaloneFiles } = React.useMemo(() => {
    if (files.length === 0) {
      return { folderGroups: [], standaloneFiles: [] };
    }

    const norm = (p: string) => p.replace(/\//g, "\\").toLowerCase().replace(/\\+$/, "");

    // 1. Candidate folders from stagedSources
    const candidateFolders = stagedSources.map((s) => ({
      original: s,
      normalized: norm(s),
      name: s.replace(/\//g, "\\").split("\\").filter(Boolean).pop() || s,
    }));

    // Sort by depth descending so nested/child folders match their files first, preventing duplication in parent folders
    const sortedCandidates = [...candidateFolders].sort((a, b) => b.normalized.length - a.normalized.length);

    const groups: FolderGroup[] = [];
    const assignedFileIds = new Set<number>();
    const folderFilesMap = new Map<string, FileItem[]>();

    for (const cf of sortedCandidates) {
      const matchingFiles = files.filter((f) => {
        if (assignedFileIds.has(f.id)) return false;
        const fNorm = norm(f.src_path);
        return fNorm.startsWith(cf.normalized + "\\");
      });

      if (matchingFiles.length > 0) {
        matchingFiles.forEach((f) => assignedFileIds.add(f.id));
        folderFilesMap.set(cf.normalized, matchingFiles);
      }
    }

    for (const cf of candidateFolders) {
      const matchingFiles = folderFilesMap.get(cf.normalized);
      if (matchingFiles && matchingFiles.length > 0) {
        const totalBytes = matchingFiles.reduce((sum, f) => sum + (f.size || 0), 0);
        let transferredBytes = 0;
        let isCurrentFolder = false;

        for (const f of matchingFiles) {
          if (f.status === "completed") {
            transferredBytes += f.size || 0;
          } else if (f.status === "copying" || f.status === "verifying") {
            isCurrentFolder = true;
            transferredBytes += Math.min(stats.current_file_transferred || 0, f.size || 0);
          }
        }

        let status: FolderGroup["status"] = "pending";
        if (matchingFiles.every((f) => f.status === "completed")) {
          status = "completed";
        } else if (isCurrentFolder) {
          status = "copying";
        } else if (matchingFiles.some((f) => f.status === "failed")) {
          status = "failed";
        } else if (matchingFiles.every((f) => f.status === "skipped")) {
          status = "skipped";
        }

        groups.push({
          id: `folder_${cf.normalized}`,
          name: cf.name,
          path: cf.original,
          files: matchingFiles,
          totalBytes,
          transferredBytes,
          status,
        });
      }
    }

    // 2. Fallback: if no candidate folders from stagedSources, group files by directory
    if (groups.length === 0 && candidateFolders.length === 0) {
      const dirMap = new Map<string, FileItem[]>();
      for (const f of files) {
        const rel = f.relative_path || "";
        const parts = rel.replace(/\//g, "\\").split("\\").filter(Boolean);
        if (parts.length > 1) {
          const dir = parts[0];
          if (!dirMap.has(dir)) dirMap.set(dir, []);
          dirMap.get(dir)!.push(f);
        } else {
          const srcParts = f.src_path.replace(/\//g, "\\").split("\\").filter(Boolean);
          if (srcParts.length > 1) {
            const dir = srcParts[srcParts.length - 2];
            if (!dirMap.has(dir)) dirMap.set(dir, []);
            dirMap.get(dir)!.push(f);
          }
        }
      }

      for (const [dirName, groupFiles] of dirMap.entries()) {
        if (groupFiles.length > 1) {
          groupFiles.forEach((f) => assignedFileIds.add(f.id));
          const totalBytes = groupFiles.reduce((sum, f) => sum + (f.size || 0), 0);
          let transferredBytes = 0;
          let isCurrentFolder = false;

          for (const f of groupFiles) {
            if (f.status === "completed") {
              transferredBytes += f.size || 0;
            } else if (f.status === "copying" || f.status === "verifying") {
              isCurrentFolder = true;
              transferredBytes += Math.min(stats.current_file_transferred || 0, f.size || 0);
            }
          }

          let status: FolderGroup["status"] = "pending";
          if (groupFiles.every((f) => f.status === "completed")) {
            status = "completed";
          } else if (isCurrentFolder) {
            status = "copying";
          } else if (groupFiles.some((f) => f.status === "failed")) {
            status = "failed";
          } else if (groupFiles.every((f) => f.status === "skipped")) {
            status = "skipped";
          }

          groups.push({
            id: `folder_${dirName}`,
            name: dirName,
            path: groupFiles[0]?.src_path || dirName,
            files: groupFiles,
            totalBytes,
            transferredBytes,
            status,
          });
        }
      }
    }

    const standalone = files.filter((f) => !assignedFileIds.has(f.id));
    return { folderGroups: groups, standaloneFiles: standalone };
  }, [files, stagedSources, stats.current_file_transferred]);

  const visibleFileIds = React.useMemo(() => {
    const ids: number[] = [];
    for (const group of folderGroups) {
      if (!collapsedFolders.has(group.id)) {
        for (const file of group.files) {
          ids.push(file.id);
        }
      }
    }
    for (const file of standaloneFiles) {
      ids.push(file.id);
    }
    return ids;
  }, [folderGroups, collapsedFolders, standaloneFiles]);
  visibleFileIdsRef.current = visibleFileIds;

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setContextMenu(null);
        setPropertiesModalItem(null);
      }
      if (e.ctrlKey && e.key.toLowerCase() === "a") {
        const activeFiles = filesRef.current;
        const activeStaged = stagedRef.current;
        if (activeFiles.length > 0) {
          e.preventDefault();
          setSelectedIds(new Set(activeFiles.map((f) => f.id)));
        } else if (activeStaged.length > 0) {
          e.preventDefault();
          setSelectedIds(new Set(activeStaged.map((_, i) => i)));
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

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
    ? (stagedSources.length === 1
        ? stagedSources[0].replace(/\\$/, "").split("\\").pop() || stagedSources[0]
        : `${stagedSources.length} ${lang === "tr" ? "Klasör/Öğe Seçildi" : "Items Staged"}`)
    : lang === "tr" ? "Hazır (Beklemede)" : "Ready (Idle)";

  const queueCount = files.length > 0 ? files.length : stagedSources.length;
  const completedCount = files.filter((f) => f.status === "completed").length;
  const hashLabel =
    {
      xxhash3: "xxHash3-64",
      xxhash128: "xxHash3-128",
      blake3: "BLAKE3",
      sha256: "SHA-256",
      md5: "MD5",
    }[hashAlgorithm] || hashAlgorithm;
  const peakSpeed = speedHistory.length > 0 ? Math.max(...speedHistory) : 0;

  return (
    <div className="w-full h-full flex flex-col bg-[#070a12] border border-[#1e293b]/70 rounded-2xl text-slate-100 select-none overflow-hidden font-sans">
      {/* ── Frameless Integrated Title Bar ── */}
      <div
        data-tauri-drag-region="deep"
        onMouseDown={(e) => {
          if (e.button === 0 && !(e.target as HTMLElement)?.closest('button, input, select, a, [role="button"]')) {
            getCurrentWindow().startDragging().catch(() => {});
          }
        }}
        onDoubleClick={(e) => {
          if (!(e.target as HTMLElement)?.closest('button, input, select, a, [role="button"]')) {
            if (onToggleMaximizeWindow) onToggleMaximizeWindow();
            else getCurrentWindow().toggleMaximize();
          }
        }}
        className="flex items-center justify-between px-4 py-2.5 bg-[#070a12] border-b border-[#172136] flex-shrink-0 cursor-grab active:cursor-grabbing"
      >
        {/* Left: App Logo, Title, Engine Badges */}
        <div data-tauri-drag-region="deep" className="flex items-center gap-2.5">
          <img data-tauri-drag-region="deep" src="/branding/app-icon.png" alt="DeepNode Copy" className="w-5 h-5 rounded-md shadow-sm" />
          <span data-tauri-drag-region="deep" className="text-xs font-semibold text-slate-100 tracking-tight">
            DeepNode Copy
          </span>
          <span data-tauri-drag-region="deep" className="text-[11px] text-slate-400 font-mono hidden sm:inline">
            — Multi-Station Engine
          </span>
          <span data-tauri-drag-region="deep" className="inline-flex items-center gap-1.5 text-[11px] px-2.5 py-0.5 rounded-full bg-emerald-950/60 text-emerald-400 border border-emerald-500/40 font-bold font-mono">
            <span data-tauri-drag-region="deep" className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
            <span data-tauri-drag-region="deep">{hashLabel}</span>
          </span>
          {directIo && (
            <span data-tauri-drag-region="deep" className="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-blue-950/60 text-blue-400 border border-blue-500/40 font-bold font-mono">
              <Zap className="w-2.5 h-2.5 text-blue-400" />
              <span>Direct I/O</span>
            </span>
          )}
        </div>

        {/* Right: Settings, Info, and Window Controls */}
        <div data-tauri-drag-region="false" className="flex items-center gap-1.5">
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
      <div data-tauri-drag-region="false" className="flex flex-1 min-h-0 overflow-hidden">
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
              <div
                data-tauri-drag-region="deep"
                onMouseDown={(e) => {
                  if (e.button === 0 && !(e.target as HTMLElement)?.closest('button, input, select, a, [role="button"], textarea')) {
                    getCurrentWindow().startDragging().catch(() => {});
                  }
                }}
                className="p-4 bg-[#0a0e1a] border-b border-[#172136] flex items-center justify-between cursor-grab active:cursor-grabbing select-none"
              >
                <div data-tauri-drag-region="deep" className="flex flex-col min-w-0 flex-1 pr-4">
                  <h2 data-tauri-drag-region="deep" className="text-base font-bold text-white tracking-tight truncate">
                    Active Batch: {activeBatchName}
                  </h2>
                  <div data-tauri-drag-region="deep" className="text-xs text-slate-400 font-mono mt-0.5 truncate">
                    Destination: {destination || (lang === "tr" ? "Seçilmedi" : "Not Selected")} • Direct I/O: {directIo ? "Active" : "Standard"}
                  </div>
                </div>

                <div data-tauri-drag-region="deep" className="text-right flex-shrink-0 font-mono">
                  <div data-tauri-drag-region="deep" className="text-2xl font-black text-cyan-400 tracking-tight">
                    {transferStatus === "canceled" ? "0.0 MB/s" : speedDisplay}
                  </div>
                  <div data-tauri-drag-region="deep" className="text-xs text-emerald-400 font-medium flex items-center gap-1.5 justify-end mt-0.5">
                    <CheckCircle2 className="w-3.5 h-3.5" />
                    <span>
                      {completedCount} {lang === "tr" ? "dosya tamamlandı" : "files completed"}
                    </span>
                  </div>
                </div>
              </div>

              {/* Row 1: Destination & Transfer Actions Bar */}
              <div className="px-4 py-2.5 bg-[#080c16] border-b border-[#172136] flex items-center justify-between gap-3 flex-wrap">
                <div className="flex items-center gap-2 flex-1 min-w-[280px]">
                  {/* Destination Input & Pick Button */}
                  <div
                    data-drop-target="destination"
                    className={`flex items-center bg-[#0c101d] border rounded-xl px-2.5 py-1.5 text-xs font-mono text-slate-300 flex-1 max-w-md transition ${
                      isDraggingOver ? "border-cyan-400 ring-2 ring-cyan-400/25" : "border-slate-800"
                    }`}
                  >
                    <span className="text-[11px] text-slate-500 mr-2 uppercase tracking-wider font-semibold font-sans">{lang === "tr" ? "HEDEF" : "TARGET"}:</span>
                    <input
                      type="text"
                      value={destination}
                      onChange={(e) => onChangeDestination(e.target.value)}
                      placeholder={lang === "tr" ? "Hedef Klasör (Örn: D:\\Yedekler)..." : "Target Folder..."}
                      className="bg-transparent border-none outline-none text-xs text-slate-200 font-mono flex-1 min-w-0"
                    />
                    <button
                      onClick={onBrowseDestination}
                      className="ml-2 px-2.5 py-0.5 rounded-lg bg-[#162035] hover:bg-blue-600 hover:text-white text-slate-300 text-[11px] font-sans font-medium transition cursor-pointer"
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
                        className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-xl bg-[#121827] hover:bg-rose-950/80 border border-slate-700/80 hover:border-rose-900 text-slate-300 hover:text-rose-400 text-xs font-medium transition cursor-pointer"
                      >
                        <X className="w-3.5 h-3.5" />
                        <span>{t.cancel}</span>
                      </button>
                    </>
                  )}
                </div>
              </div>

              {/* Row 2: Queue Management Toolbar */}
              <div className="px-4 py-2 bg-[#0a0e19] border-b border-[#172136] flex items-center justify-between gap-2 text-xs flex-wrap">
                <div className="flex items-center gap-1.5">
                  <button
                    onClick={onAddFiles}
                    className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-blue-600/90 hover:bg-blue-500 text-white font-medium text-xs transition cursor-pointer shadow-sm"
                    title={lang === "tr" ? "Listeye dosya ekle" : "Add files to list"}
                  >
                    <FilePlus className="w-3 h-3" />
                    <span>{lang === "tr" ? "Dosya Ekle" : "Add Files"}</span>
                  </button>

                  <button
                    onClick={onAddFolder}
                    className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-[#141b2d] hover:bg-[#1e293b] border border-slate-700/60 text-slate-300 hover:text-white font-medium text-xs transition cursor-pointer"
                    title={lang === "tr" ? "Listeye klasör ekle" : "Add folder to list"}
                  >
                    <FolderPlus className="w-3 h-3" />
                    <span>{lang === "tr" ? "Klasör Ekle" : "Add Folder"}</span>
                  </button>

                  <div className="h-4 w-[1px] bg-slate-800 mx-1" />

                  <button
                    onClick={() => {
                      if (selectedIds.size > 0) {
                        if (files.length > 0 && onRemoveQueued) {
                          onRemoveQueued(Array.from(selectedIds));
                        } else if (stagedSources.length > 0) {
                          const remaining = stagedSources.filter((_, idx) => !selectedIds.has(idx));
                          if (remaining.length === 0) {
                            onClearStaged();
                          } else {
                            onRemoveStaged(Array.from(selectedIds));
                          }
                        }
                        setSelectedIds(new Set());
                      }
                    }}
                    disabled={selectedIds.size === 0}
                    className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border transition cursor-pointer text-xs font-medium ${
                      selectedIds.size > 0
                        ? "bg-rose-950/40 border-rose-900/60 text-rose-300 hover:bg-rose-900/60"
                        : "bg-[#141b2d]/50 border-slate-800 text-slate-500 cursor-not-allowed"
                    }`}
                    title={lang === "tr" ? "Seçilen dosyaları listeden kaldır" : "Remove selected files from queue"}
                  >
                    <Trash2 className="w-3 h-3" />
                    <span>{lang === "tr" ? "Listeden Çıkar" : "Remove"}</span>
                  </button>

                  <button
                    onClick={() => {
                      if (selectedIds.size > 0 && onSkipFiles) {
                        onSkipFiles(Array.from(selectedIds));
                      } else if (onSkipCurrent && isTransferring) {
                        onSkipCurrent();
                      }
                    }}
                    disabled={!isTransferring && selectedIds.size === 0}
                    className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg border transition cursor-pointer text-xs font-medium ${
                      isTransferring || selectedIds.size > 0
                        ? "bg-amber-950/40 border-amber-900/60 text-amber-300 hover:bg-amber-900/60"
                        : "bg-[#141b2d]/50 border-slate-800 text-slate-500 cursor-not-allowed"
                    }`}
                    title={lang === "tr" ? "Seçilen dosyayı veya mevcut kopyalamayı atla" : "Skip selected file or active copy"}
                  >
                    <SkipForward className="w-3.5 h-3.5" />
                    <span>{lang === "tr" ? "Atla" : "Skip"}</span>
                  </button>

                  {files.some((f) => f.status === "failed" || f.status === "skipped") && (
                    <button
                      onClick={onRetryFailed}
                      className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-emerald-950/50 hover:bg-emerald-900/60 border border-emerald-800/60 text-emerald-300 font-medium text-xs transition cursor-pointer"
                      title={lang === "tr" ? "Hatalı ve atlanan dosyaları yeniden dene" : "Retry failed and skipped files"}
                    >
                      <RotateCw className="w-3 h-3" />
                      <span>{lang === "tr" ? "Tekrar Dene" : "Retry"}</span>
                    </button>
                  )}

                  <button
                    onClick={() => {
                      if (isTransferring) return;
                      setSelectedIds(new Set());
                      onResetToIdle();
                    }}
                    disabled={isTransferring}
                    className={`flex items-center gap-1 px-2 py-1 rounded-lg border text-xs transition ${
                      isTransferring
                        ? "bg-[#141b2d]/50 border-slate-800 text-slate-600 cursor-not-allowed"
                        : "bg-[#141b2d] hover:bg-[#1e293b] border-slate-800 text-slate-400 hover:text-slate-200 cursor-pointer"
                    }`}
                    title={
                      isTransferring
                        ? lang === "tr"
                          ? "Aktarım sürerken kuyruk temizlenemez"
                          : "Queue cannot be cleared during an active transfer"
                        : lang === "tr"
                        ? "Kuyruğu Temizle"
                        : "Clear Queue"
                    }
                  >
                    <Eraser className="w-3 h-3" />
                    <span>{lang === "tr" ? "Temizle" : "Clear"}</span>
                  </button>
                </div>

                {/* Right: Selection count & helper */}
                <div className="flex items-center gap-2 text-[11px] font-mono text-slate-400">
                  {selectedIds.size > 0 ? (
                    <span className="text-cyan-400 font-medium">
                      {selectedIds.size} / {queueCount} {lang === "tr" ? "seçildi" : "selected"}
                    </span>
                  ) : (
                    <span>
                      {queueCount} {lang === "tr" ? "öğe kuyrukta" : "items in queue"}
                    </span>
                  )}
                  <button
                    onClick={() => {
                      if (files.length > 0) {
                        if (selectedIds.size === files.length) setSelectedIds(new Set());
                        else setSelectedIds(new Set(files.map((f) => f.id)));
                      } else if (stagedSources.length > 0) {
                        if (selectedIds.size === stagedSources.length) setSelectedIds(new Set());
                        else setSelectedIds(new Set(stagedSources.map((_, i) => i)));
                      }
                    }}
                    className="px-2 py-0.5 rounded bg-slate-800/60 hover:bg-slate-700 text-slate-300 text-[10px] transition cursor-pointer"
                  >
                    {(files.length > 0 && selectedIds.size === files.length) || (stagedSources.length > 0 && selectedIds.size === stagedSources.length)
                      ? (lang === "tr" ? "Seçimi Kaldır" : "Deselect")
                      : (lang === "tr" ? "Tümünü Seç" : "Select All")}
                  </button>
                </div>
              </div>

              {/* Table Column Headers (Matching Mockup 4 1:1) */}
              <div className="grid grid-cols-12 px-5 py-2.5 bg-[#090d18] border-b border-[#172136] text-[11px] font-mono font-bold text-slate-400 tracking-wider items-center">
                <div className="col-span-5 flex items-center gap-2.5">
                  <input
                    type="checkbox"
                    checked={(files.length > 0 && selectedIds.size === files.length) || (stagedSources.length > 0 && selectedIds.size === stagedSources.length)}
                    onChange={(e) => {
                      if (e.target.checked) {
                        if (files.length > 0) setSelectedIds(new Set(files.map((f) => f.id)));
                        else setSelectedIds(new Set(stagedSources.map((_, i) => i)));
                      } else {
                        setSelectedIds(new Set());
                      }
                    }}
                    className="rounded border-slate-700 bg-slate-900 text-blue-600 focus:ring-0 cursor-pointer"
                  />
                  <span>{lang === "tr" ? "DOSYA ADI" : "FILENAME"}</span>
                </div>
                <div className="col-span-2 text-right pr-6">{lang === "tr" ? "BOYUT" : "SIZE"}</div>
                <div className="col-span-2 pl-4">{lang === "tr" ? "HIZ / DOĞRULAMA" : "SPEED / VERIF"}</div>
                <div className="col-span-3 text-right pr-4">{lang === "tr" ? "İLERLEME" : "PROGRESS"}</div>
              </div>

              {/* Table Body */}
              <div
                onContextMenu={(e) => {
                  if ((e.target as HTMLElement)?.closest('.grid-cols-12')) return;
                  e.preventDefault();
                  setContextMenu({ x: e.clientX, y: e.clientY, item: null });
                }}
                className="flex-1 overflow-y-auto divide-y divide-[#172136]/50"
              >
                {files.length === 0 && stagedSources.length === 0 ? (
                  // Empty Dropzone with native drag & drop visual feedback
                  <div
                    className={`h-full flex flex-col items-center justify-center p-8 text-center transition-all ${
                      isDraggingOver
                        ? "bg-blue-950/40 border-2 border-dashed border-cyan-400 scale-[0.99] rounded-2xl shadow-xl shadow-cyan-500/10"
                        : "text-slate-400"
                    }`}
                  >
                    <div className="w-16 h-16 rounded-2xl bg-blue-950/30 border border-blue-500/30 flex items-center justify-center text-blue-400 mb-4 shadow-lg shadow-blue-500/10">
                      <FolderPlus className="w-8 h-8" />
                    </div>
                    <h3 className="text-sm font-bold text-white mb-1">
                      {isDraggingOver
                        ? (lang === "tr" ? "Klasör veya dosyaları buraya bırakın" : "Release to drop files or folders here")
                        : (lang === "tr" ? "Dosya veya klasörleri buraya sürükleyip bırakın" : "Drag and drop files or folders here")}
                    </h3>
                    <p className="text-xs text-slate-400 max-w-md mb-4">
                      {lang === "tr"
                        ? "Dosyaları veya klasörleri doğrudan Windows Gezgini'nden buraya sürükleyebilirsiniz."
                        : "Drag files or folders directly from Windows Explorer here."}
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
                  // Actual transfer files with folder grouping & memoized high-performance rows
                  <>
                    {folderGroups.map((group) => {
                      const isCollapsed = collapsedFolders.has(group.id);
                      const isGroupCurrent = group.files.some(
                        (f) => f.status === "copying" || f.status === "verifying"
                      );
                      const selectedInFolder = group.files.filter((f) =>
                        selectedIds.has(f.id)
                      ).length;

                      return (
                        <React.Fragment key={group.id}>
                          <FolderRowItem
                            group={group}
                            isCollapsed={isCollapsed}
                            selectedCount={selectedInFolder}
                            isCurrent={isGroupCurrent}
                            speedDisplay={speedDisplay}
                            onToggleExpand={handleToggleFolderExpand}
                            onToggleSelectFolder={handleToggleFolderSelect}
                            onContextMenu={handleFolderContextMenu}
                            onShowInFolder={onShowInFolder}
                            lang={lang}
                          />
                          {!isCollapsed &&
                            group.files.map((file) => {
                              const isCurrent =
                                file.status === "copying" || file.status === "verifying";
                              const isSelected = selectedIds.has(file.id);

                              return (
                                <FileRowItem
                                  key={file.id}
                                  file={file}
                                  isSelected={isSelected}
                                  isCurrent={isCurrent}
                                  isInsideFolder={true}
                                  speedDisplay={speedDisplay}
                                  currentFileTransferred={stats.current_file_transferred}
                                  overallPercentage={stats.overall_percentage}
                                  lang={lang}
                                  onToggleSelect={handleToggleSelect}
                                  onRangeSelect={handleRangeSelect}
                                  onSingleSelect={handleSingleSelect}
                                  onShowInFolder={onShowInFolder}
                                  onContextMenu={handleRowContextMenu}
                                />
                              );
                            })}
                        </React.Fragment>
                      );
                    })}

                    {standaloneFiles.map((file) => {
                      const isCurrent =
                        file.status === "copying" || file.status === "verifying";
                      const isSelected = selectedIds.has(file.id);

                      return (
                        <FileRowItem
                          key={file.id}
                          file={file}
                          isSelected={isSelected}
                          isCurrent={isCurrent}
                          isInsideFolder={false}
                          speedDisplay={speedDisplay}
                          currentFileTransferred={stats.current_file_transferred}
                          overallPercentage={stats.overall_percentage}
                          lang={lang}
                          onToggleSelect={handleToggleSelect}
                          onRangeSelect={handleRangeSelect}
                          onSingleSelect={handleSingleSelect}
                          onShowInFolder={onShowInFolder}
                          onContextMenu={handleRowContextMenu}
                        />
                      );
                    })}
                  </>
                ) : (
                  // Staged items preview before start
                  stagedSources.map((src, idx) => {
                    const isSelected = selectedIds.has(idx);
                    return (
                      <div
                        key={idx}
                        onClick={(e) => {
                          if (e.ctrlKey || e.metaKey) {
                            lastAnchorIdRef.current = idx;
                            const next = new Set(selectedIds);
                            if (next.has(idx)) next.delete(idx);
                            else next.add(idx);
                            setSelectedIds(next);
                          } else if (e.shiftKey) {
                            const anchor =
                              lastAnchorIdRef.current !== null &&
                              lastAnchorIdRef.current >= 0 &&
                              lastAnchorIdRef.current < stagedSources.length
                                ? lastAnchorIdRef.current
                                : idx;
                            const [min, max] = anchor < idx ? [anchor, idx] : [idx, anchor];
                            const range: number[] = [];
                            for (let i = min; i <= max; i++) range.push(i);
                            setSelectedIds(new Set(range));
                          } else {
                            lastAnchorIdRef.current = idx;
                            setSelectedIds(new Set([idx]));
                          }
                        }}
                        onDoubleClick={() => {
                          if (onShowInFolder) onShowInFolder(src);
                        }}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setSelectedIds(new Set([idx]));
                          setContextMenu({
                            x: e.clientX,
                            y: e.clientY,
                            item: {
                              id: idx,
                              src_path: src,
                              dst_path: destination ? `${destination}\\${src.split("\\").pop()}` : "",
                              relative_path: src.split("\\").pop() || src,
                              size: 0,
                              bytes_transferred: 0,
                              status: "pending",
                              retry_attempts: 0,
                            },
                          });
                        }}
                        className={`grid grid-cols-12 items-center px-5 py-2.5 text-xs transition cursor-pointer select-none ${
                          isSelected ? "bg-blue-900/35 border-l-2 border-l-blue-400" : "hover:bg-slate-800/20"
                        }`}
                      >
                        <div className="col-span-5 flex items-center gap-2.5 min-w-0 pr-2">
                          <input
                            type="checkbox"
                            checked={isSelected}
                            onChange={(e) => {
                              e.stopPropagation();
                              const next = new Set(selectedIds);
                              if (e.target.checked) next.add(idx);
                              else next.delete(idx);
                              setSelectedIds(next);
                            }}
                            className="rounded border-slate-700 bg-slate-900 text-blue-600 focus:ring-0 cursor-pointer"
                          />
                          {getFileIcon(src)}
                          <span className="truncate text-slate-200 font-medium" title={src}>
                            {src.split("\\").pop() || src}
                          </span>
                        </div>
                        <div className="col-span-2 text-right pr-6 font-mono text-slate-400">-</div>
                        <div className="col-span-2 pl-4 font-mono text-xs text-slate-500">
                          {lang === "tr" ? "Hazır" : "Ready"}
                        </div>
                        <div className="col-span-3 flex items-center justify-end gap-2.5 pr-4">
                          <div className="flex-1 max-w-[150px] bg-slate-800/80 rounded-full h-2 overflow-hidden border border-slate-700/50 p-[1px]">
                            <div className="h-full rounded-full w-0 bg-slate-600" />
                          </div>
                          <span className="w-12 text-right font-mono text-[11px] text-slate-500 font-semibold">0%</span>
                        </div>
                      </div>
                    );
                  })
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
                  <div className="text-xs text-slate-400">
                    Peak: {peakSpeed > 0 ? `${formatBytes(peakSpeed)}/s` : "—"}
                  </div>
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
                    {hashLabel}
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
                    <option value="ondifference">{lang === "tr" ? "Farklılık Halinde Doğrula" : "On Difference Only"}</option>
                    <option value="never">{lang === "tr" ? "Doğrulama (Maksimum Ham Hız)" : "Never Verify"}</option>
                  </select>
                </div>

                <div className="pt-2">
                  <span
                    className={`text-xs font-semibold flex items-center gap-1.5 ${
                      verifyMode === "never" ? "text-amber-400" : "text-emerald-400"
                    }`}
                  >
                    <CheckCircle2 className="w-4 h-4" />
                    <span>
                      {verifyMode === "always"
                        ? lang === "tr"
                          ? "Her dosya kopyalandıktan sonra hash ile doğrulanır."
                          : "Every copied file is verified by hash."
                        : verifyMode === "ondifference"
                        ? lang === "tr"
                          ? "Doğrulama yalnızca kaynak/hedef farklı göründüğünde çalışır."
                          : "Verification runs only when the source and target appear different."
                        : lang === "tr"
                        ? "Hash doğrulaması kapalı; yalnızca kopyalama bütünlüğü denetlenir."
                        : "Hash verification is off; only copy integrity is checked."}
                    </span>
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
                    <option value="close_app">{lang === "tr" ? "Uygulamayı Kapat" : "Close App"}</option>
                    <option value="sleep">{lang === "tr" ? "Bilgisayarı Uyut" : "Sleep PC"}</option>
                    <option value="hibernate">{lang === "tr" ? "Bilgisayarı Hazırda Beklet" : "Hibernate PC"}</option>
                    <option value="shutdown">{lang === "tr" ? "Bilgisayarı Kapat" : "Shutdown PC"}</option>
                  </select>
                </div>

                {/* Popup Selection Memory */}
                <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] flex flex-col justify-between">
                  <div>
                    <label className="text-xs font-bold text-slate-200">
                      {lang === "tr" ? "Seçim Penceresi" : "Selection Popup"}
                    </label>
                    <p className="text-[11px] text-slate-400 mt-0.5">
                      {lang === "tr"
                        ? "Kopyala / taşı penceresi son seçilen işlemi varsayılan olarak seçili gösterir (TeraCopy davranışı)."
                        : "The copy / move popup preselects the last chosen action, like TeraCopy."}
                    </p>
                  </div>
                  <label className="flex items-center gap-2 pt-2 cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={rememberPopupChoice}
                      onChange={(e) => onChangeRememberPopupChoice(e.target.checked)}
                      className="w-4 h-4 rounded bg-[#121827] border-slate-700 text-blue-600 focus:ring-0 cursor-pointer"
                    />
                    <span className="text-xs font-semibold text-slate-200">
                      {lang === "tr" ? "Son seçimi hatırla" : "Remember last choice"}
                    </span>
                  </label>
                </div>
              </div>

              {/* Windows Shell Integration */}
              <div className="p-4 rounded-2xl bg-[#0a0e1a] border border-[#172136] max-w-3xl flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <h4 className="text-xs font-bold text-white">
                      {lang === "tr" ? "Windows Sağ Tık Menü Entegrasyonu" : "Windows Context Menu Integration"}
                    </h4>
                    <span
                      className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-semibold border ${
                        isContextMenuRegistered
                          ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                          : "bg-slate-800 text-slate-400 border-slate-700"
                      }`}
                    >
                      <span
                        className={`w-1.5 h-1.5 rounded-full ${
                          isContextMenuRegistered ? "bg-emerald-400 animate-pulse" : "bg-slate-500"
                        }`}
                      />
                      {isContextMenuRegistered
                        ? lang === "tr"
                          ? "Etkin"
                          : "Active"
                        : lang === "tr"
                        ? "Kayıtlı Değil"
                        : "Not Registered"}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-400">
                    {lang === "tr"
                      ? "Dosya, klasör ve sürücülere sağ tıklandığında 'DeepNode Copy ile Kopyala' ve 'DeepNode Copy ile Taşı' seçeneklerini ekler (Windows 10 ve 11 desteklenmektedir)."
                      : "Adds 'Copy with DeepNode Copy' and 'Move with DeepNode Copy' when right-clicking files, folders, or drives in Windows Explorer (Windows 10 & 11 supported)."}
                  </p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {isContextMenuRegistered && onUnregisterContextMenu && (
                    <button
                      onClick={onUnregisterContextMenu}
                      className="px-3.5 py-1.5 rounded-xl border border-rose-500/30 bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 font-semibold text-xs transition cursor-pointer"
                    >
                      {lang === "tr" ? "Menüyü Kaldır" : "Remove Menu"}
                    </button>
                  )}
                  <button
                    onClick={onRegisterContextMenu}
                    className="px-3.5 py-1.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-sm"
                  >
                    {isContextMenuRegistered
                      ? lang === "tr"
                        ? "Yeniden Kaydet / Güncelle"
                        : "Re-register / Update"
                      : lang === "tr"
                      ? "Menüyü Kaydet"
                      : "Register Menu"}
                  </button>
                </div>
              </div>
            </div>
          )}

          {/* ── Tab 5: Transfer History & Archival Reports ── */}
          {sidebarTab === "history" && (
            <HistoryView
              lang={lang}
              onRerunSession={handleRerunSessionInternal}
              onShowInFolder={onShowInFolder}
              onOpenFolder={onOpenFolder}
            />
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

      {/* ── High-Speed Queue Context Menu ── */}
      {contextMenu && (
        <>
          {/* Transparent full-screen overlay to intercept clicks and close menu instantly */}
          <div
            className="fixed inset-0 z-[9998]"
            onPointerDown={(e) => {
              e.stopPropagation();
              setContextMenu(null);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              setContextMenu(null);
            }}
          />
          <div
            style={{
              position: "fixed",
              left: Math.min(contextMenu.x, window.innerWidth - 240),
              top: Math.min(contextMenu.y, window.innerHeight - 360),
              zIndex: 9999,
            }}
            onClick={(e) => e.stopPropagation()}
            onContextMenu={(e) => e.stopPropagation()}
            className="w-56 bg-[#0c1220]/95 backdrop-blur-md border border-[#1e293b] rounded-xl shadow-2xl p-1.5 flex flex-col gap-0.5 text-xs text-slate-200 select-none animate-in fade-in zoom-in-95 duration-100"
          >
          {contextMenu.item ? (
            <>
              {/* Action 1: Skip */}
              <button
                onClick={() => {
                  if (selectedIds.size > 0 && onSkipFiles) {
                    onSkipFiles(Array.from(selectedIds));
                  } else if (onSkipCurrent && isTransferring) {
                    onSkipCurrent();
                  }
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-amber-500/15 text-amber-300 hover:text-amber-200 transition cursor-pointer text-left font-medium"
              >
                <SkipForward className="w-3.5 h-3.5 flex-shrink-0" />
                <span>{lang === "tr" ? "Bu Dosyayı Atla" : "Skip File"}</span>
              </button>

              {/* Action 2: Remove from List */}
              <button
                onClick={() => {
                  if (files.length > 0) {
                    const ids = selectedIds.size > 0 ? Array.from(selectedIds) : [contextMenu.item!.id];
                    if (onRemoveQueued) onRemoveQueued(ids);
                  } else if (stagedSources.length > 0) {
                    if (selectedIds.size > 0) {
                      onRemoveStaged(Array.from(selectedIds));
                    } else if (contextMenu.item) {
                      onRemoveStaged(contextMenu.item.id);
                    }
                  }
                  setSelectedIds(new Set());
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-rose-500/15 text-rose-300 hover:text-rose-200 transition cursor-pointer text-left font-medium"
              >
                <Trash2 className="w-3.5 h-3.5 flex-shrink-0" />
                <span>{lang === "tr" ? "Listeden Çıkar" : "Remove from Queue"}</span>
              </button>

              {/* Action 3: Retry (if failed or skipped) */}
              {(contextMenu.item.status === "failed" || contextMenu.item.status === "skipped") && (
                <button
                  onClick={() => {
                    onRetryFailed();
                    setContextMenu(null);
                  }}
                  className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-emerald-500/15 text-emerald-300 hover:text-emerald-200 transition cursor-pointer text-left font-medium"
                >
                  <RotateCw className="w-3.5 h-3.5 flex-shrink-0" />
                  <span>{lang === "tr" ? "Tekrar Dene" : "Retry"}</span>
                </button>
              )}

              <div className="my-1 border-t border-[#172136]" />

              {/* Action 4: Add Files */}
              <button
                onClick={() => {
                  onAddFiles();
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left font-medium"
              >
                <FilePlus className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Dosya Ekle..." : "Add Files..."}</span>
              </button>

              {/* Action 5: Add Folder */}
              <button
                onClick={() => {
                  onAddFolder();
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left font-medium"
              >
                <FolderPlus className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Klasör Ekle..." : "Add Folder..."}</span>
              </button>

              <div className="my-1 border-t border-[#172136]" />

              {/* Action 6: Open File Location */}
              <button
                onClick={() => {
                  if (onShowInFolder) onShowInFolder(contextMenu.item!.src_path);
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left"
              >
                <FolderOpen className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Dosya Konumunu Aç (Kaynak)" : "Open Source Location"}</span>
              </button>

              {/* Action 7: Open Target Folder */}
              <button
                onClick={() => {
                  const target = contextMenu.item!.dst_path || destination;
                  if (target && onOpenFolder) onOpenFolder(target);
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left"
              >
                <ExternalLink className="w-3.5 h-3.5 text-cyan-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Hedef Konumunu Aç" : "Open Target Folder"}</span>
              </button>

              {/* Action 8: Copy File Path */}
              <button
                onClick={() => {
                  navigator.clipboard.writeText(contextMenu.item!.src_path);
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-300 hover:text-white transition cursor-pointer text-left"
              >
                <Copy className="w-3.5 h-3.5 text-slate-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Dosya Yolunu Kopyala" : "Copy Path"}</span>
              </button>

              <div className="my-1 border-t border-[#172136]" />

              {/* Action 9: Select All */}
              <button
                onClick={() => {
                  if (files.length > 0) setSelectedIds(new Set(files.map((f) => f.id)));
                  else if (stagedSources.length > 0) setSelectedIds(new Set(stagedSources.map((_, i) => i)));
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-300 hover:text-white transition cursor-pointer text-left"
              >
                <CheckSquare className="w-3.5 h-3.5 text-slate-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Tümünü Seç" : "Select All"}</span>
              </button>

              {/* Action 10: Properties */}
              <button
                onClick={() => {
                  setPropertiesModalItem(contextMenu.item);
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-blue-600/20 text-blue-300 hover:text-blue-200 transition cursor-pointer text-left font-medium"
              >
                <Info className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Özellikler..." : "Properties..."}</span>
              </button>
            </>
          ) : (
            <>
              {/* Empty background context menu */}
              <button
                onClick={() => {
                  onAddFiles();
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left font-medium"
              >
                <FilePlus className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Dosya Ekle..." : "Add Files..."}</span>
              </button>

              <button
                onClick={() => {
                  onAddFolder();
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-200 hover:text-white transition cursor-pointer text-left font-medium"
              >
                <FolderPlus className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Klasör Ekle..." : "Add Folder..."}</span>
              </button>

              <div className="my-1 border-t border-[#172136]" />

              <button
                onClick={() => {
                  if (files.length > 0) setSelectedIds(new Set(files.map((f) => f.id)));
                  else if (stagedSources.length > 0) setSelectedIds(new Set(stagedSources.map((_, i) => i)));
                  setContextMenu(null);
                }}
                className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-slate-800/70 text-slate-300 hover:text-white transition cursor-pointer text-left"
              >
                <CheckSquare className="w-3.5 h-3.5 text-slate-400 flex-shrink-0" />
                <span>{lang === "tr" ? "Tümünü Seç" : "Select All"}</span>
              </button>

              <button
                onClick={() => {
                  if (isTransferring) return;
                  onClearStaged();
                  setContextMenu(null);
                }}
                disabled={isTransferring}
                className={`flex items-center gap-2.5 px-3 py-2 rounded-lg transition text-left font-medium ${
                  isTransferring
                    ? "text-slate-600 cursor-not-allowed"
                    : "hover:bg-rose-500/15 text-rose-300 hover:text-rose-200 cursor-pointer"
                }`}
              >
                <Trash2 className="w-3.5 h-3.5 flex-shrink-0" />
                <span>{lang === "tr" ? "Kuyruğu Temizle" : "Clear Queue"}</span>
              </button>
            </>
          )}
        </div>
      </>
    )}

      {/* ── File Properties Modal ── */}
      {propertiesModalItem && (
        <div
          className="fixed inset-0 z-[10000] flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
          onClick={() => setPropertiesModalItem(null)}
        >
          <div
            className="w-full max-w-lg bg-[#0a0e1a] border border-[#1e293b] rounded-2xl shadow-2xl p-5 flex flex-col gap-4 text-xs font-sans text-slate-200 animate-in fade-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between border-b border-[#172136] pb-3">
              <div className="flex items-center gap-2.5">
                <div className="w-8 h-8 rounded-xl bg-blue-950/60 border border-blue-500/30 flex items-center justify-center text-blue-400">
                  <Info className="w-4 h-4" />
                </div>
                <div>
                  <h3 className="text-sm font-bold text-white tracking-tight">
                    {lang === "tr" ? "Dosya Detayları & Özellikler" : "File Details & Properties"}
                  </h3>
                  <p className="text-[11px] text-slate-400">
                    {propertiesModalItem.relative_path || propertiesModalItem.src_path.split("\\").pop()}
                  </p>
                </div>
              </div>
              <button
                onClick={() => setPropertiesModalItem(null)}
                className="w-7 h-7 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white flex items-center justify-center transition cursor-pointer"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Modal Details Grid */}
            <div className="flex flex-col gap-2.5 py-1 font-mono text-[11px]">
              {/* Filename */}
              <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-1">
                <span className="text-[10px] uppercase font-bold text-slate-500 tracking-wider">
                  {lang === "tr" ? "Dosya Adı" : "File Name"}
                </span>
                <span className="text-white font-semibold break-all">
                  {propertiesModalItem.relative_path || propertiesModalItem.src_path.split("\\").pop()}
                </span>
              </div>

              {/* Source Path */}
              <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-1">
                <div className="flex items-center justify-between">
                  <span className="text-[10px] uppercase font-bold text-slate-500 tracking-wider">
                    {lang === "tr" ? "Kaynak Yol" : "Source Path"}
                  </span>
                  <button
                    onClick={() => onShowInFolder && onShowInFolder(propertiesModalItem.src_path)}
                    className="text-cyan-400 hover:text-cyan-300 text-[10px] flex items-center gap-1 cursor-pointer"
                  >
                    <FolderOpen className="w-3 h-3" />
                    <span>{lang === "tr" ? "Gezginde Göster" : "Locate"}</span>
                  </button>
                </div>
                <span className="text-slate-300 break-all select-all">{propertiesModalItem.src_path}</span>
              </div>

              {/* Destination Path */}
              <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-1">
                <span className="text-[10px] uppercase font-bold text-slate-500 tracking-wider">
                  {lang === "tr" ? "Hedef Yol" : "Destination Path"}
                </span>
                <span className="text-slate-300 break-all select-all">
                  {propertiesModalItem.dst_path || (destination ? `${destination}\\${propertiesModalItem.relative_path || propertiesModalItem.src_path.split("\\").pop()}` : "-")}
                </span>
              </div>

              {/* Stats Row: Size, Status, Verification */}
              <div className="grid grid-cols-3 gap-2">
                <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-0.5">
                  <span className="text-[10px] uppercase font-bold text-slate-500">
                    {lang === "tr" ? "Boyut" : "Size"}
                  </span>
                  <span className="text-white font-bold">{formatBytes(propertiesModalItem.size)}</span>
                  <span className="text-[10px] text-slate-500 font-mono">
                    {propertiesModalItem.size.toLocaleString()} B
                  </span>
                </div>

                <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-0.5">
                  <span className="text-[10px] uppercase font-bold text-slate-500">
                    {lang === "tr" ? "Durum" : "Status"}
                  </span>
                  <span
                    className={`font-bold capitalize ${
                      propertiesModalItem.status === "completed"
                        ? "text-emerald-400"
                        : propertiesModalItem.status === "failed"
                        ? "text-rose-400"
                        : propertiesModalItem.status === "skipped"
                        ? "text-amber-400"
                        : propertiesModalItem.status === "copying"
                        ? "text-cyan-400"
                        : "text-slate-400"
                    }`}
                  >
                    {propertiesModalItem.status}
                  </span>
                  <span className="text-[10px] text-slate-500">
                    {propertiesModalItem.retry_attempts > 0 ? `${propertiesModalItem.retry_attempts} retries` : "Initial pass"}
                  </span>
                </div>

                <div className="p-2.5 rounded-xl bg-[#070a12] border border-[#172136] flex flex-col gap-0.5">
                  <span className="text-[10px] uppercase font-bold text-slate-500">
                    {lang === "tr" ? "Doğrulama" : "Verification"}
                  </span>
                  <span className="text-emerald-400 font-bold flex items-center gap-1">
                    <ShieldCheck className="w-3 h-3 flex-shrink-0" />
                    <span>{verifyMode === "never" ? "-" : hashLabel}</span>
                  </span>
                  <span className="text-[10px] text-slate-500 font-mono">
                    {propertiesModalItem.status === "completed"
                      ? verifyMode === "never"
                        ? lang === "tr"
                          ? "Doğrulama kapalı"
                          : "Verification off"
                        : propertiesModalItem.dst_hash
                        ? lang === "tr"
                          ? "Doğrulandı"
                          : "Verified"
                        : lang === "tr"
                        ? "Tamamlandı"
                        : "Completed"
                      : lang === "tr"
                      ? "Bekliyor"
                      : "Pending"}
                  </span>
                </div>
              </div>

              {propertiesModalItem.error_message && (
                <div className="p-2.5 rounded-xl bg-rose-950/30 border border-rose-900/50 flex flex-col gap-1 text-rose-300">
                  <span className="text-[10px] uppercase font-bold text-rose-400 tracking-wider">
                    {lang === "tr" ? "Hata Detayı" : "Error Detail"}
                  </span>
                  <span className="break-all">{propertiesModalItem.error_message}</span>
                </div>
              )}
            </div>

            {/* Modal Actions */}
            <div className="flex items-center justify-end gap-2 border-t border-[#172136] pt-3">
              <button
                onClick={() => {
                  if (onShowInFolder) onShowInFolder(propertiesModalItem.src_path);
                }}
                className="px-3.5 py-1.5 rounded-xl bg-[#141b2d] hover:bg-[#1e293b] border border-slate-700 text-slate-200 font-medium text-xs flex items-center gap-1.5 transition cursor-pointer"
              >
                <FolderOpen className="w-3.5 h-3.5 text-blue-400" />
                <span>{lang === "tr" ? "Gezginde Göster" : "Show in Explorer"}</span>
              </button>

              <button
                onClick={() => setPropertiesModalItem(null)}
                className="px-4 py-1.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-md shadow-blue-600/20"
              >
                {lang === "tr" ? "Kapat" : "Close"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

