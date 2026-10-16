import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Clock,
  Search,
  Trash2,
  RotateCw,
  FileText,
  FileSpreadsheet,
  ShieldCheck,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  FolderOpen,
  Folder,
  Copy,
  Check,
  RefreshCw,
  ExternalLink,
} from "lucide-react";
import { SessionRecord, HistoryFileRecord } from "../types";
import { formatBytes } from "./CompactView";
import { Language } from "../i18n";
import { getFileIcon } from "./ExpandedView";

interface HistoryViewProps {
  lang: Language;
  onRerunSession?: (sources: string[], destination: string) => void;
  onShowInFolder?: (path: string) => void;
  onOpenFolder?: (path: string) => void;
}

export const HistoryView: React.FC<HistoryViewProps> = ({
  lang,
  onRerunSession,
  onShowInFolder,
  onOpenFolder,
}) => {
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<number | null>(null);
  const [sessionFiles, setSessionFiles] = useState<HistoryFileRecord[]>([]);
  const [filterText, setFilterText] = useState("");
  const [fileFilterText, setFileFilterText] = useState("");
  const [fileStatusFilter, setFileStatusFilter] = useState<"all" | "completed" | "failed" | "skipped">("all");
  const [loadingSessions, setLoadingSessions] = useState(false);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [copiedHash, setCopiedHash] = useState<string | null>(null);
  const [confirmClearAll, setConfirmClearAll] = useState(false);

  // Load sessions with debounce and request ID sequence protection to prevent stale overwrites
  const searchRequestIdRef = React.useRef(0);
  const filesRequestIdRef = React.useRef(0);

  useEffect(() => {
    const currentReqId = ++searchRequestIdRef.current;
    const query = filterText.trim();

    const timer = setTimeout(
      async () => {
        setLoadingSessions(true);
        try {
          const records = await invoke<SessionRecord[]>("get_history_sessions", {
            limit: 100,
            offset: 0,
            filter: query ? query : null,
          });
          if (currentReqId === searchRequestIdRef.current) {
            setSessions(records || []);
            if (records && records.length > 0) {
              setSelectedSessionId((cur) => {
                if (cur !== null && records.some((r) => r.id === cur)) {
                  return cur;
                }
                return records[0].id;
              });
            } else {
              setSelectedSessionId(null);
            }
          }
        } catch (e) {
          if (currentReqId === searchRequestIdRef.current) {
            console.error("Failed to load history sessions:", e);
          }
        } finally {
          if (currentReqId === searchRequestIdRef.current) {
            setLoadingSessions(false);
          }
        }
      },
      query ? 250 : 0
    );

    return () => {
      clearTimeout(timer);
    };
  }, [filterText]);

  // Load files for selected session with request ID protection
  useEffect(() => {
    if (!selectedSessionId) {
      setSessionFiles([]);
      return;
    }
    const currentFilesReqId = ++filesRequestIdRef.current;
    const loadFiles = async () => {
      setLoadingFiles(true);
      try {
        const files = await invoke<HistoryFileRecord[]>("get_history_session_files", {
          sessionId: selectedSessionId,
        });
        if (currentFilesReqId === filesRequestIdRef.current) {
          setSessionFiles(files || []);
        }
      } catch (e) {
        if (currentFilesReqId === filesRequestIdRef.current) {
          console.error("Failed to load history session files:", e);
        }
      } finally {
        if (currentFilesReqId === filesRequestIdRef.current) {
          setLoadingFiles(false);
        }
      }
    };
    loadFiles();
  }, [selectedSessionId]);

  const selectedSession = sessions.find((s) => s.id === selectedSessionId) || null;

  // Single session delete
  const handleDeleteSession = async (sessionId: number, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    try {
      await invoke("delete_history_session", { sessionId });
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (selectedSessionId === sessionId) {
        const remaining = sessions.filter((s) => s.id !== sessionId);
        setSelectedSessionId(remaining.length > 0 ? remaining[0].id : null);
      }
    } catch (err) {
      console.error("Failed to delete session:", err);
    }
  };

  // Clear all history
  const handleClearAll = async () => {
    try {
      await invoke("clear_history");
      setSessions([]);
      setSelectedSessionId(null);
      setSessionFiles([]);
      setConfirmClearAll(false);
    } catch (err) {
      console.error("Failed to clear history:", err);
    }
  };

  // Export report
  const handleExport = async (format: "html" | "csv" | "checksum") => {
    if (!selectedSession) return;
    try {
      const content = await invoke<string>("export_history_session", {
        sessionId: selectedSession.id,
        format,
      });

      const mimeType =
        format === "html"
          ? "text/html;charset=utf-8"
          : format === "csv"
          ? "text/csv;charset=utf-8"
          : "text/plain;charset=utf-8";

      const ext = format === "checksum" ? "xxh3" : format;
      const filename = `DeepNodeCopy_History_${selectedSession.session_uuid.slice(0, 8)}.${ext}`;

      const blob = new Blob([content], { type: mimeType });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error("Failed to export history session:", err);
      alert(lang === "tr" ? `Rapor dışa aktarma hatası: ${err}` : `Export error: ${err}`);
    }
  };

  // Copy hash to clipboard
  const handleCopyHash = (hash: string) => {
    navigator.clipboard.writeText(hash);
    setCopiedHash(hash);
    setTimeout(() => setCopiedHash(null), 2000);
  };

  // Rerun past transfer
  const handleRerun = async (onlyFailed = false) => {
    if (!selectedSession) return;
    let targets: string[] = [];

    let filesToUse = sessionFiles;
    if (filesToUse.length === 0) {
      try {
        const fetched = await invoke<HistoryFileRecord[]>("get_history_session_files", {
          sessionId: selectedSession.id,
        });
        if (fetched && fetched.length > 0) {
          filesToUse = fetched;
          setSessionFiles(fetched);
        }
      } catch (err) {
        console.error("Failed to load session files for rerun:", err);
      }
    }

    if (onlyFailed) {
      targets = filesToUse
        .filter((f) => f.status === "failed")
        .map((f) => f.src_path);
    } else if (filesToUse.length > 0) {
      // Unique source paths from authentic file records
      targets = Array.from(new Set(filesToUse.map((f) => f.src_path)));
    }

    if (targets.length === 0) return;
    if (onRerunSession) {
      onRerunSession(targets, selectedSession.destination);
    }
  };

  // Filtered files in selected session
  const filteredFiles = sessionFiles.filter((f) => {
    if (fileStatusFilter !== "all" && f.status.toLowerCase() !== fileStatusFilter) {
      return false;
    }
    if (fileFilterText.trim()) {
      const q = fileFilterText.toLowerCase();
      return (
        f.src_path.toLowerCase().includes(q) ||
        f.dst_path.toLowerCase().includes(q) ||
        (f.error_message && f.error_message.toLowerCase().includes(q)) ||
        (f.dst_hash && f.dst_hash.toLowerCase().includes(q))
      );
    }
    return true;
  });

  const completedCount = sessionFiles.filter((f) => f.status === "completed").length;
  const failedCount = sessionFiles.filter((f) => f.status === "failed").length;
  const skippedCount = sessionFiles.filter((f) => f.status === "skipped").length;

  const formatDate = (isoString: string) => {
    try {
      const d = new Date(isoString);
      return d.toLocaleString(lang === "tr" ? "tr-TR" : "en-US", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      });
    } catch {
      return isoString;
    }
  };

  const getStatusBadge = (status: string) => {
    const s = status.toLowerCase();
    if (s === "completed") {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-500/15 text-emerald-400 border border-emerald-500/30">
          <CheckCircle2 className="w-3 h-3" />
          {lang === "tr" ? "Tamamlandı" : "Completed"}
        </span>
      );
    }
    if (s === "failed") {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-rose-500/15 text-rose-400 border border-rose-500/30">
          <XCircle className="w-3 h-3" />
          {lang === "tr" ? "Hata Var" : "Failed"}
        </span>
      );
    }
    if (s === "canceled") {
      return (
        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-amber-500/15 text-amber-400 border border-amber-500/30">
          <AlertTriangle className="w-3 h-3" />
          {lang === "tr" ? "İptal Edildi" : "Canceled"}
        </span>
      );
    }
    return (
      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-slate-500/15 text-slate-400 border border-slate-500/30">
        {status}
      </span>
    );
  };

  return (
    <div className="flex-1 flex overflow-hidden bg-[#070a12]">
      {/* ── Left Pane: Session Master List ── */}
      <div className="w-80 md:w-96 flex flex-col border-r border-[#172136] bg-[#090e1c] flex-shrink-0">
        {/* Top Header & Search */}
        <div className="p-3.5 border-b border-[#172136] flex flex-col gap-2.5">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Clock className="w-4 h-4 text-blue-400" />
              <h3 className="text-xs font-bold uppercase tracking-wider text-slate-200">
                {lang === "tr" ? "Aktarım Geçmişi" : "Transfer History"}
              </h3>
              <span className="px-1.5 py-0.2 rounded-md bg-blue-500/20 text-blue-300 text-[10px] font-mono">
                {sessions.length}
              </span>
            </div>

            {sessions.length > 0 && (
              <button
                onClick={() => setConfirmClearAll(true)}
                className="text-[11px] text-slate-400 hover:text-rose-400 transition flex items-center gap-1 cursor-pointer"
                title={lang === "tr" ? "Tüm geçmişi temizle" : "Clear all history"}
              >
                <Trash2 className="w-3 h-3" />
                <span>{lang === "tr" ? "Temizle" : "Clear"}</span>
              </button>
            )}
          </div>

          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              placeholder={lang === "tr" ? "Geçmişte ara (hedef, kaynak)..." : "Search history..."}
              className="w-full pl-8 pr-3 py-1.5 rounded-lg bg-[#0d1527] border border-[#1c2a47] text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500/70"
            />
          </div>
        </div>

        {/* Clear All Confirmation Banner */}
        {confirmClearAll && (
          <div className="p-3 bg-rose-950/40 border-b border-rose-800/50 flex flex-col gap-2 animate-fadeIn">
            <div className="text-xs text-rose-200 font-medium">
              {lang === "tr"
                ? "Tüm aktarım geçmişi kalıcı olarak silinecek. Emin misiniz?"
                : "All transfer history will be deleted. Are you sure?"}
            </div>
            <div className="flex items-center gap-2 justify-end">
              <button
                onClick={() => setConfirmClearAll(false)}
                className="px-2.5 py-1 rounded-md text-xs text-slate-300 hover:bg-slate-800 transition cursor-pointer"
              >
                {lang === "tr" ? "Vazgeç" : "Cancel"}
              </button>
              <button
                onClick={handleClearAll}
                className="px-2.5 py-1 rounded-md text-xs bg-rose-600 hover:bg-rose-500 text-white font-semibold transition cursor-pointer"
              >
                {lang === "tr" ? "Evet, Temizle" : "Yes, Clear"}
              </button>
            </div>
          </div>
        )}

        {/* Sessions List */}
        <div className="flex-1 overflow-y-auto divide-y divide-[#172136]/50">
          {loadingSessions ? (
            <div className="p-8 flex flex-col items-center justify-center gap-2 text-slate-500 text-xs">
              <RotateCw className="w-4 h-4 animate-spin text-blue-400" />
              <span>{lang === "tr" ? "Geçmiş yükleniyor..." : "Loading history..."}</span>
            </div>
          ) : sessions.length === 0 ? (
            <div className="p-8 flex flex-col items-center justify-center gap-3 text-center text-slate-500">
              <Clock className="w-8 h-8 text-slate-600 stroke-[1.5]" />
              <div className="text-xs font-medium text-slate-400">
                {lang === "tr" ? "Henüz kayıtlı aktarım yok" : "No transfer history found"}
              </div>
              <div className="text-[11px] text-slate-600 max-w-[200px]">
                {lang === "tr"
                  ? "Gerçekleştirilen dosya kopyalama ve taşıma işlemleri otomatik olarak burada arşivlenir."
                  : "Completed copy and move operations are automatically archived here."}
              </div>
            </div>
          ) : (
            sessions.map((s) => {
              const isSelected = s.id === selectedSessionId;
              const dateStr = formatDate(s.start_time);

              return (
                <div
                  key={s.id}
                  onClick={() => setSelectedSessionId(s.id)}
                  className={`p-3 transition cursor-pointer relative group ${
                    isSelected
                      ? "bg-blue-950/40 border-l-4 border-l-blue-500"
                      : "hover:bg-slate-800/30 border-l-4 border-l-transparent"
                  }`}
                >
                  <div className="flex items-center justify-between gap-1.5 mb-1.5">
                    <div className="flex items-center gap-1.5 min-w-0">
                      <span
                        className={`text-[9px] font-bold uppercase px-1.5 py-0.5 rounded flex-shrink-0 ${
                          s.mode === "move"
                            ? "bg-purple-500/20 text-purple-300"
                            : "bg-blue-500/20 text-blue-300"
                        }`}
                      >
                        {s.mode}
                      </span>
                      {getStatusBadge(s.status)}
                    </div>
                    <div className="flex items-center gap-1.5 flex-shrink-0">
                      <span className="text-[10px] text-slate-500 font-mono tracking-tight">{dateStr}</span>
                      <button
                        onClick={(e) => handleDeleteSession(s.id, e)}
                        className="w-5 h-5 flex items-center justify-center rounded text-slate-500 hover:text-rose-400 hover:bg-rose-500/15 opacity-0 group-hover:opacity-100 pointer-events-none group-hover:pointer-events-auto transition cursor-pointer"
                        title={lang === "tr" ? "Bu kaydı sil" : "Delete record"}
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>

                  <div className="text-xs font-semibold text-slate-200 truncate flex items-center gap-1.5 mt-1">
                    <Folder className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                    <span className="truncate" title={s.destination}>
                      {s.destination || (lang === "tr" ? "Hedefsiz" : "No target")}
                    </span>
                  </div>

                  <div className="flex items-center justify-between mt-2 text-[11px] text-slate-400 font-mono">
                    <div>
                      <span className="text-slate-300 font-semibold">{s.total_files}</span>{" "}
                      {lang === "tr" ? "dosya" : "files"} • {formatBytes(s.transferred_bytes || s.total_bytes)}
                    </div>
                    {s.average_speed_bps > 0 && (
                      <div className="text-cyan-400 font-semibold">
                        {formatBytes(s.average_speed_bps)}/s
                      </div>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* ── Right Pane: Session Detail & Files ── */}
      {selectedSession ? (
        <div className="flex-1 flex flex-col overflow-hidden bg-[#070a12]">
          {/* Top Session Action & Metadata Bar */}
          <div className="p-4 border-b border-[#172136] bg-[#0b1021] flex flex-col gap-3">
            <div className="flex items-center justify-between flex-wrap gap-2">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-xl bg-blue-600/15 border border-blue-500/30 text-blue-400">
                  <Clock className="w-5 h-5" />
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <h2 className="text-sm font-bold text-white tracking-tight">
                      {formatDate(selectedSession.start_time)}
                    </h2>
                    <span
                      className={`text-[10px] font-bold uppercase px-2 py-0.5 rounded-md ${
                        selectedSession.mode === "move"
                          ? "bg-purple-500/20 text-purple-300 border border-purple-500/30"
                          : "bg-blue-500/20 text-blue-300 border border-blue-500/30"
                      }`}
                    >
                      {selectedSession.mode.toUpperCase()}
                    </span>
                    {getStatusBadge(selectedSession.status)}
                  </div>
                  <div className="text-xs text-slate-400 font-mono mt-0.5 flex items-center gap-1.5 truncate max-w-xl">
                    <span className="text-slate-500">ID:</span>
                    <span>{selectedSession.session_uuid}</span>
                  </div>
                </div>
              </div>

              {/* Action Buttons */}
              <div className="flex items-center gap-2 flex-wrap">
                {/* Rerun button */}
                <button
                  onClick={() => handleRerun(false)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs transition cursor-pointer shadow-md shadow-blue-600/20"
                  title={lang === "tr" ? "Bu dosyaları tekrar kopyalamak üzere kuyruğa al" : "Load files back into transfer queue"}
                >
                  <RotateCw className="w-3.5 h-3.5" />
                  <span>{lang === "tr" ? "Tekrar Kopyala" : "Rerun Transfer"}</span>
                </button>

                {failedCount > 0 && (
                  <button
                    onClick={() => handleRerun(true)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-rose-600 hover:bg-rose-500 text-white font-semibold text-xs transition cursor-pointer shadow-md shadow-rose-600/20"
                    title={lang === "tr" ? "Yalnızca başarısız dosyaları kuyruğa al" : "Retry only failed items"}
                  >
                    <RefreshCw className="w-3.5 h-3.5" />
                    <span>{lang === "tr" ? "Hatalıları Tekrarla" : "Retry Failed"}</span>
                  </button>
                )}

                {/* Export Dropdown / Buttons */}
                <button
                  onClick={() => handleExport("html")}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium border border-slate-700 transition cursor-pointer"
                  title={lang === "tr" ? "İnteraktif HTML raporu indir" : "Export interactive HTML report"}
                >
                  <FileText className="w-3.5 h-3.5 text-blue-400" />
                  <span>HTML</span>
                </button>

                <button
                  onClick={() => handleExport("csv")}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium border border-slate-700 transition cursor-pointer"
                  title={lang === "tr" ? "CSV tablosu olarak dışa aktar" : "Export CSV spreadsheet"}
                >
                  <FileSpreadsheet className="w-3.5 h-3.5 text-emerald-400" />
                  <span>CSV</span>
                </button>

                <button
                  onClick={() => handleExport("checksum")}
                  className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium border border-slate-700 transition cursor-pointer"
                  title={lang === "tr" ? "xxHash3 sağlama listesi indir (.xxh3)" : "Export xxHash3 checklist (.xxh3)"}
                >
                  <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
                  <span>.xxh3</span>
                </button>

                <button
                  onClick={() => handleDeleteSession(selectedSession.id)}
                  className="p-1.5 rounded-xl bg-slate-800 hover:bg-rose-950/60 hover:text-rose-400 text-slate-400 border border-slate-700 transition cursor-pointer"
                  title={lang === "tr" ? "Bu kaydı sil" : "Delete record"}
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            </div>

            {/* Target & Source Path Banner */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-2 text-xs">
              <div className="p-2.5 rounded-xl bg-[#070b16] border border-[#17223b] flex items-center justify-between">
                <div className="min-w-0 flex items-center gap-2">
                  <span className="text-[10px] uppercase font-bold text-slate-500 flex-shrink-0">
                    {lang === "tr" ? "Hedef:" : "Dest:"}
                  </span>
                  <span className="text-slate-200 font-mono truncate" title={selectedSession.destination}>
                    {selectedSession.destination || "-"}
                  </span>
                </div>
                {selectedSession.destination && onOpenFolder && (
                  <button
                    onClick={() => onOpenFolder(selectedSession.destination)}
                    className="p-1 text-slate-400 hover:text-blue-400 transition cursor-pointer flex-shrink-0"
                    title={lang === "tr" ? "Klasörü Aç" : "Open Folder"}
                  >
                    <FolderOpen className="w-3.5 h-3.5" />
                  </button>
                )}
              </div>

              <div className="p-2.5 rounded-xl bg-[#070b16] border border-[#17223b] flex items-center justify-between">
                <div className="min-w-0 flex items-center gap-2">
                  <span className="text-[10px] uppercase font-bold text-slate-500 flex-shrink-0">
                    {lang === "tr" ? "Kaynak:" : "Source:"}
                  </span>
                  <span className="text-slate-200 font-mono truncate" title={selectedSession.source_summary}>
                    {selectedSession.source_summary || "-"}
                  </span>
                </div>
              </div>
            </div>

            {/* KPI Cards */}
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-1">
              <div className="p-2.5 rounded-xl bg-[#0a0f21] border border-[#19243f]">
                <div className="text-[10px] uppercase font-bold text-slate-500">
                  {lang === "tr" ? "Toplam Boyut" : "Total Size"}
                </div>
                <div className="text-sm font-bold text-white font-mono mt-0.5">
                  {formatBytes(selectedSession.transferred_bytes || selectedSession.total_bytes)}
                </div>
              </div>

              <div className="p-2.5 rounded-xl bg-[#0a0f21] border border-[#19243f]">
                <div className="text-[10px] uppercase font-bold text-slate-500">
                  {lang === "tr" ? "Dosyalar" : "Files"}
                </div>
                <div className="text-sm font-bold text-slate-200 font-mono mt-0.5">
                  <span className="text-emerald-400">{selectedSession.completed_files}</span>
                  <span className="text-slate-500"> / </span>
                  <span>{selectedSession.total_files}</span>
                  {selectedSession.failed_files > 0 && (
                    <span className="text-rose-400 ml-1 text-xs">({selectedSession.failed_files} hata)</span>
                  )}
                </div>
              </div>

              <div className="p-2.5 rounded-xl bg-[#0a0f21] border border-[#19243f]">
                <div className="text-[10px] uppercase font-bold text-slate-500">
                  {lang === "tr" ? "Ortalama Hız" : "Average Speed"}
                </div>
                <div className="text-sm font-bold text-cyan-400 font-mono mt-0.5">
                  {selectedSession.average_speed_bps > 0
                    ? `${formatBytes(selectedSession.average_speed_bps)}/s`
                    : "-"}
                </div>
              </div>

              <div className="p-2.5 rounded-xl bg-[#0a0f21] border border-[#19243f]">
                <div className="text-[10px] uppercase font-bold text-slate-500">
                  {lang === "tr" ? "Bütünlük Sağlaması" : "Integrity Verification"}
                </div>
                <div className="text-sm font-bold text-emerald-400 font-mono mt-0.5 flex items-center gap-1">
                  <ShieldCheck className="w-3.5 h-3.5" />
                  <span>xxHash3</span>
                </div>
              </div>
            </div>
          </div>

          {/* Files Table Section */}
          <div className="flex-1 flex flex-col overflow-hidden">
            {/* Filter Pills & File Search */}
            <div className="p-3 border-b border-[#172136] bg-[#080d1a] flex items-center justify-between gap-3 flex-wrap">
              <div className="flex items-center gap-1.5">
                <button
                  onClick={() => setFileStatusFilter("all")}
                  className={`px-2.5 py-1 rounded-lg text-xs font-semibold transition cursor-pointer ${
                    fileStatusFilter === "all"
                      ? "bg-blue-600 text-white"
                      : "text-slate-400 hover:text-white hover:bg-slate-800"
                  }`}
                >
                  {lang === "tr" ? "Tümü" : "All"} ({sessionFiles.length})
                </button>
                <button
                  onClick={() => setFileStatusFilter("completed")}
                  className={`px-2.5 py-1 rounded-lg text-xs font-semibold transition cursor-pointer ${
                    fileStatusFilter === "completed"
                      ? "bg-emerald-600 text-white"
                      : "text-slate-400 hover:text-emerald-400 hover:bg-slate-800"
                  }`}
                >
                  {lang === "tr" ? "Tamamlanan" : "Completed"} ({completedCount})
                </button>
                {failedCount > 0 && (
                  <button
                    onClick={() => setFileStatusFilter("failed")}
                    className={`px-2.5 py-1 rounded-lg text-xs font-semibold transition cursor-pointer ${
                      fileStatusFilter === "failed"
                        ? "bg-rose-600 text-white"
                        : "text-slate-400 hover:text-rose-400 hover:bg-slate-800"
                    }`}
                  >
                    {lang === "tr" ? "Hatalı" : "Failed"} ({failedCount})
                  </button>
                )}
                {skippedCount > 0 && (
                  <button
                    onClick={() => setFileStatusFilter("skipped")}
                    className={`px-2.5 py-1 rounded-lg text-xs font-semibold transition cursor-pointer ${
                      fileStatusFilter === "skipped"
                        ? "bg-amber-600 text-white"
                        : "text-slate-400 hover:text-amber-400 hover:bg-slate-800"
                    }`}
                  >
                    {lang === "tr" ? "Atlanan" : "Skipped"} ({skippedCount})
                  </button>
                )}
              </div>

              <div className="relative w-64">
                <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
                <input
                  type="text"
                  value={fileFilterText}
                  onChange={(e) => setFileFilterText(e.target.value)}
                  placeholder={lang === "tr" ? "Dosyalar içinde ara..." : "Filter files..."}
                  className="w-full pl-8 pr-3 py-1 rounded-lg bg-[#0d1527] border border-[#1c2a47] text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-blue-500/70"
                />
              </div>
            </div>

            {/* Table Header */}
            <div className="grid grid-cols-12 px-4 py-2 bg-[#060913] border-b border-[#172136] text-[11px] font-semibold text-slate-400 uppercase tracking-wider">
              <div className="col-span-5">{lang === "tr" ? "Dosya Adı & Yol" : "Filename & Path"}</div>
              <div className="col-span-2 text-right">{lang === "tr" ? "Boyut" : "Size"}</div>
              <div className="col-span-2 text-center">{lang === "tr" ? "Durum" : "Status"}</div>
              <div className="col-span-3 text-right">{lang === "tr" ? "xxHash3 Sağlaması" : "xxHash3 Hash"}</div>
            </div>

            {/* Table Body */}
            <div className="flex-1 overflow-y-auto divide-y divide-[#172136]/40">
              {loadingFiles ? (
                <div className="p-8 flex items-center justify-center gap-2 text-slate-500 text-xs">
                  <RotateCw className="w-4 h-4 animate-spin text-blue-400" />
                  <span>{lang === "tr" ? "Dosyalar listeleniyor..." : "Loading files..."}</span>
                </div>
              ) : filteredFiles.length === 0 ? (
                <div className="p-8 text-center text-slate-500 text-xs">
                  {lang === "tr" ? "Bu filtreye uygun dosya bulunamadı." : "No files matched filter."}
                </div>
              ) : (
                filteredFiles.map((f) => {
                  const fileName = f.src_path.split("\\").pop() || f.src_path;
                  const hash = f.dst_hash || f.src_hash || "-";

                  return (
                    <div
                      key={f.id}
                      className="grid grid-cols-12 px-4 py-2 items-center hover:bg-slate-800/25 transition text-xs group"
                    >
                      {/* Name & Source */}
                      <div className="col-span-5 min-w-0 flex items-center gap-2 pr-2">
                        {getFileIcon(fileName)}
                        <div className="min-w-0 flex-1">
                          <div
                            className="font-medium text-slate-200 truncate cursor-pointer hover:text-blue-400"
                            title={f.src_path}
                            onClick={() => onShowInFolder && onShowInFolder(f.dst_path || f.src_path)}
                          >
                            {fileName}
                          </div>
                          <div className="text-[10px] text-slate-500 truncate font-mono" title={f.src_path}>
                            {f.src_path}
                          </div>
                          {f.error_message && (
                            <div className="text-[10px] text-rose-400 truncate mt-0.5 font-sans" title={f.error_message}>
                              {f.error_message}
                            </div>
                          )}
                        </div>
                      </div>

                      {/* Size */}
                      <div className="col-span-2 text-right font-mono text-slate-300 text-xs">
                        {formatBytes(f.size)}
                      </div>

                      {/* Status */}
                      <div className="col-span-2 flex justify-center">
                        {f.status === "completed" ? (
                          <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-emerald-400">
                            <CheckCircle2 className="w-3.5 h-3.5" />
                            {lang === "tr" ? "Başarılı" : "Success"}
                          </span>
                        ) : f.status === "failed" ? (
                          <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-rose-400">
                            <XCircle className="w-3.5 h-3.5" />
                            {lang === "tr" ? "Hata" : "Failed"}
                          </span>
                        ) : f.status === "skipped" ? (
                          <span className="inline-flex items-center gap-1 text-[11px] font-semibold text-amber-400">
                            <AlertTriangle className="w-3.5 h-3.5" />
                            {lang === "tr" ? "Atlandı" : "Skipped"}
                          </span>
                        ) : (
                          <span className="text-slate-400 text-xs">{f.status}</span>
                        )}
                      </div>

                      {/* Hash & Copy */}
                      <div className="col-span-3 flex items-center justify-end gap-1.5 font-mono text-[11px]">
                        {hash !== "-" ? (
                          <button
                            onClick={() => handleCopyHash(hash)}
                            className="flex items-center gap-1 px-2 py-0.5 rounded bg-blue-500/10 hover:bg-blue-500/20 text-cyan-400 hover:text-cyan-300 transition cursor-pointer border border-blue-500/20"
                            title={lang === "tr" ? "xxHash3 sağlamasını kopyala" : "Copy xxHash3 checksum"}
                          >
                            <span className="truncate max-w-[120px]">{hash}</span>
                            {copiedHash === hash ? (
                              <Check className="w-3 h-3 text-emerald-400 flex-shrink-0" />
                            ) : (
                              <Copy className="w-3 h-3 text-slate-400 flex-shrink-0" />
                            )}
                          </button>
                        ) : (
                          <span className="text-slate-600">-</span>
                        )}

                        {onShowInFolder && (
                          <button
                            onClick={() => onShowInFolder(f.dst_path || f.src_path)}
                            className="p-1 rounded text-slate-500 hover:text-blue-400 opacity-0 group-hover:opacity-100 transition cursor-pointer"
                            title={lang === "tr" ? "Klasörde Göster" : "Show in Folder"}
                          >
                            <ExternalLink className="w-3.5 h-3.5" />
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </div>
      ) : (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-slate-500">
          <Clock className="w-12 h-12 text-slate-700 stroke-[1.2]" />
          <div className="text-sm font-semibold text-slate-400">
            {lang === "tr" ? "Bir aktarım oturumu seçin" : "Select a transfer session"}
          </div>
          <p className="text-xs text-slate-600 max-w-sm text-center">
            {lang === "tr"
              ? "Geçmiş aktarımların dosyalarını, kriptografik xxHash3 doğrulamalarını inceleyebilir veya HTML/CSV raporu alabilirsiniz."
              : "Review past transferred files, cryptographic xxHash3 verification, or generate HTML/CSV reports."}
          </p>
        </div>
      )}
    </div>
  );
};
