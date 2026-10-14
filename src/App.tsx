import React, { useState, useEffect } from "react";
import * as Sentry from "@sentry/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  FileItem,
  TransferStats,
  EngineEvent,
  ScanPreviewResult,
  LaunchArgs,
  TransferStatus,
  StartTransferResult,
  ConflictInfo,
  CurrentTransferState,
} from "./types";
import { CompactView } from "./components/CompactView";
import { ExpandedView } from "./components/ExpandedView";
import { VerifyModal } from "./components/VerifyModal";
import { CopyPasteModal } from "./components/CopyPasteModal";
import { ConflictModal } from "./components/ConflictModal";
import { Language, translations } from "./i18n";

export const App: React.FC = () => {
  const [isCompact, setIsCompact] = useState(false);
  const [transferStatus, setTransferStatus] = useState<TransferStatus>("idle");
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [verifyModalOpen, setVerifyModalOpen] = useState(false);
  const [copyPasteModalOpen, setCopyPasteModalOpen] = useState(false);
  const [activeConflict, setActiveConflict] = useState<ConflictInfo | null>(null);
  const [lang, setLangState] = useState<Language>(() => {
    try {
      const saved = localStorage.getItem("deepnode_copy_lang");
      if (saved && saved in translations) {
        return saved as Language;
      }
    } catch {
      // ignore
    }
    return "tr";
  });

  const setLang = (newLang: Language) => {
    setLangState(newLang);
    try {
      localStorage.setItem("deepnode_copy_lang", newLang);
    } catch {
      // ignore
    }
  };

  const t = translations[lang] || translations.tr;
  const isCanceledRef = React.useRef(false);

  // Transfer settings
  const [onFinishAction, setOnFinishAction] = useState("do_nothing");
  const [conflictPolicy, setConflictPolicy] = useState("ask");
  const [verifyMode, setVerifyMode] = useState("always");
  const [hashAlgorithm, setHashAlgorithm] = useState("xxhash3");
  const [directIo, setDirectIo] = useState(false);

  // Staged sources and destination workstation
  const [stagedSources, setStagedSources] = useState<string[]>([]);
  const [destination, setDestination] = useState<string>("");
  const [stagedPreview, setStagedPreview] = useState<ScanPreviewResult | null>(null);

  const [stats, setStats] = useState<TransferStats>({
    total_files: 0,
    completed_files: 0,
    skipped_files: 0,
    failed_files: 0,
    total_bytes: 0,
    transferred_bytes: 0,
    current_speed_bps: 0,
    average_speed_bps: 0,
    eta_seconds: 0,
    overall_percentage: 0,
    current_file_name: "",
    current_file_transferred: 0,
    current_file_size: 0,
  });

  const [files, setFiles] = useState<FileItem[]>([]);
  const [speedHistory, setSpeedHistory] = useState<number[]>([]);

  const isTransferring = transferStatus === "transferring" || transferStatus === "paused";
  const isPaused = transferStatus === "paused";

  const updatePreview = async (sources: string[]) => {
    if (sources.length === 0) {
      setStagedPreview(null);
      return;
    }
    try {
      const res = await invoke<ScanPreviewResult>("scan_items_preview", { sources });
      setStagedPreview(res);
    } catch (e) {
      console.error("Preview scan error:", e);
    }
  };

  useEffect(() => {
    let unlistenFn: (() => void) | null = null;

    const setupApp = async () => {
      // 1. Listen for real-time engine events from Tauri Rust backend FIRST
      try {
        unlistenFn = await listen<EngineEvent>("engine_event", (event) => {
          const data = event.payload;

          switch (data.type) {
            case "TransferStarted":
              isCanceledRef.current = false;
              setTransferStatus("transferring");
              setStats((prev) => ({
                ...prev,
                total_files: data.payload.total_files,
                total_bytes: data.payload.total_bytes,
              }));
              if (data.payload.items && data.payload.items.length > 0) {
                setFiles(data.payload.items);
                const first = data.payload.items[0];
                if (first) {
                  setStats((prev) => ({
                    ...prev,
                    current_file_name: first.relative_path || first.src_path,
                    current_file_size: first.size,
                  }));
                }
              }
              break;

            case "ProgressUpdated":
              setStats(data.payload);
              setSpeedHistory((prev) => {
                const speedMb = data.payload.current_speed_bps / 1_048_576;
                const updated = [...prev, speedMb];
                return updated.length > 50 ? updated.slice(updated.length - 50) : updated;
              });
              break;

            case "FileProgress":
              setFiles((prev) =>
                prev.map((f) =>
                  f.id === data.payload.id
                    ? { ...f, bytes_transferred: data.payload.transferred, status: "copying" }
                    : f
                )
              );
              break;

            case "FileStarted":
              setFiles((prev) => {
                const existing = prev.find((f) => f.id === data.payload.id);
                if (existing) {
                  return prev.map((f) =>
                    f.id === data.payload.id ? { ...data.payload, status: "copying" } : f
                  );
                }
                return [...prev, { ...data.payload, status: "copying" }];
              });
              setStats((prev) => ({
                ...prev,
                current_file_name: data.payload.relative_path || data.payload.src_path,
                current_file_size: data.payload.size,
              }));
              break;

            case "FileCompleted":
              setFiles((prev) =>
                prev.map((f) =>
                  f.id === data.payload.id
                    ? { ...data.payload, status: "completed", bytes_transferred: data.payload.size }
                    : f
                )
              );
              break;

            case "FileSkipped":
              setFiles((prev) =>
                prev.map((f) => (f.id === data.payload.item.id ? data.payload.item : f))
              );
              break;

            case "FileFailed":
              setFiles((prev) =>
                prev.map((f) => (f.id === data.payload.item.id ? data.payload.item : f))
              );
              break;

            case "TransferPaused":
              setTransferStatus("paused");
              break;

            case "TransferResumed":
              setTransferStatus("transferring");
              break;

            case "TransferCompleted":
              if (isCanceledRef.current) {
                setTransferStatus("canceled");
                break;
              }
              setStats(data.payload);
              setTransferStatus((prev) => (prev === "canceled" ? "canceled" : "completed"));
              Sentry.addBreadcrumb({
                category: "transfer",
                message: `Transfer completed: ${data.payload.completed_files}/${data.payload.total_files} files (${data.payload.transferred_bytes} bytes)`,
                level: "info",
              });
              break;

            case "TransferCanceled":
              isCanceledRef.current = true;
              setTransferStatus("canceled");
              setFiles((prev) =>
                prev.map((f) =>
                  f.status === "copying" || f.status === "pending"
                    ? { ...f, status: "canceled" }
                    : f
                )
              );
              setStats((prev) => ({
                ...prev,
                current_speed_bps: 0,
                eta_seconds: 0,
              }));
              Sentry.addBreadcrumb({
                category: "transfer",
                message: "Transfer canceled by user",
                level: "info",
              });
              break;

            case "TransferFailed":
              setTransferStatus("failed");
              Sentry.captureMessage(`DeepNode Transfer Failed: ${data.payload}`, "error");
              alert(`${t.statusFailed}: ${data.payload}`);
              break;

            case "ConflictDetected":
              setActiveConflict(data.payload);
              break;

            default:
              break;
          }
        });
      } catch (err) {
        console.error("Error setting up event listener:", err);
      }

      // 2. Check current transfer state (if auto-started by Rust) or launch args
      try {
        const curState = await invoke<CurrentTransferState>("get_current_transfer_state");
        if (
          curState &&
          (curState.status === "transferring" ||
            curState.status === "paused" ||
            curState.status === "completed" ||
            curState.status === "canceled")
        ) {
          if (curState.status === "canceled") {
            isCanceledRef.current = true;
          }
          setTransferStatus(curState.status as TransferStatus);
          setIsCompact(true);
          invoke("toggle_compact_mode", { compact: true }).catch(() => {});
          if (curState.stats) {
            setStats(curState.stats);
          }
          if (curState.files && curState.files.length > 0) {
            setFiles(curState.files);
          }
          if (curState.launch_args.sources && curState.launch_args.sources.length > 0) {
            setStagedSources(curState.launch_args.sources);
          }
          if (curState.launch_args.destination) {
            setDestination(curState.launch_args.destination);
          }
          return;
        }

        const args = curState?.launch_args || (await invoke<LaunchArgs>("get_launch_args"));
        if (args.sources && args.sources.length > 0) {
          setStagedSources(args.sources);
          updatePreview(args.sources);
          if (args.destination) {
            setDestination(args.destination);
          }
          if (args.is_popup) {
            setCopyPasteModalOpen(true);
          } else if (args.auto_start && args.destination) {
            // Automatically launch transfer
            setIsCompact(true);
            invoke("toggle_compact_mode", { compact: true }).catch(() => {});
            setTransferStatus("transferring");

            try {
              const init = await invoke<StartTransferResult>("start_transfer", {
                sources: args.sources,
                destination: args.destination,
                mode: args.mode || "copy",
                verifyMode: "always",
                hashAlgorithm: "xxhash3",
                conflictPolicy: args.conflict_policy || "ask",
                directIo: false,
                onFinish: args.on_finish || "do_nothing",
              });

              if (init && init.items) {
                setStats((prev) => ({
                  ...prev,
                  total_files: init.total_files,
                  total_bytes: init.total_bytes,
                  current_file_name:
                    prev.current_file_name ||
                    init.items[0]?.relative_path ||
                    init.items[0]?.src_path ||
                    "",
                  current_file_size: prev.current_file_size || init.items[0]?.size || 0,
                }));
                setFiles(init.items);
              }
            } catch (err) {
              setTransferStatus("failed");
              console.error("Auto transfer error:", err);
            }
          }
        }
      } catch (err) {
        console.error("Error reading launch args:", err);
      }
    };

    setupApp();

    return () => {
      if (unlistenFn) unlistenFn();
    };
  }, []);

  const handleAddFiles = async () => {
    try {
      const picked = await invoke<string[]>("pick_source_files");
      if (picked && picked.length > 0) {
        const updated = Array.from(new Set([...stagedSources, ...picked]));
        setStagedSources(updated);
        updatePreview(updated);
      }
    } catch (e) {
      console.error("pick_source_files error:", e);
    }
  };

  const handleAddFolder = async () => {
    try {
      const folder = await invoke<string | null>("pick_source_folder");
      if (folder) {
        const updated = Array.from(new Set([...stagedSources, folder]));
        setStagedSources(updated);
        updatePreview(updated);
      }
    } catch (e) {
      console.error("pick_source_folder error:", e);
    }
  };

  const handleBrowseDestination = async () => {
    try {
      const folder = await invoke<string | null>("pick_destination_folder");
      if (folder) {
        setDestination(folder);
      }
    } catch (e) {
      console.error("pick_destination_folder error:", e);
    }
  };

  const handleRemoveStaged = (index: number) => {
    const updated = stagedSources.filter((_, idx) => idx !== index);
    setStagedSources(updated);
    updatePreview(updated);
  };

  const handleClearStaged = () => {
    setStagedSources([]);
    setStagedPreview(null);
    setFiles([]);
  };

  const handleStartTransfer = async (mode: "copy" | "move", overridePolicy?: string) => {
    if (stagedSources.length === 0) {
      alert(t.noFilesFound);
      return;
    }

    let target = destination;
    if (!target) {
      const picked = await invoke<string | null>("pick_destination_folder");
      if (!picked) return;
      target = picked;
      setDestination(picked);
    }

    setIsCompact(true);
    invoke("toggle_compact_mode", { compact: true }).catch(() => {});
    setTransferStatus("transferring");
    setFiles([]);
    setSpeedHistory([]);

    try {
      const init = await invoke<StartTransferResult>("start_transfer", {
        sources: stagedSources,
        destination: target,
        mode,
        verifyMode,
        hashAlgorithm,
        conflictPolicy: overridePolicy || conflictPolicy,
        directIo,
        onFinish: onFinishAction,
      });

      if (init && init.items) {
        setStats((prev) => ({
          ...prev,
          total_files: init.total_files,
          total_bytes: init.total_bytes,
          current_file_name: prev.current_file_name || init.items[0]?.relative_path || init.items[0]?.src_path || "",
          current_file_size: prev.current_file_size || init.items[0]?.size || 0,
        }));
        setFiles((prev) => {
          if (prev.length === 0) return init.items;
          return init.items.map((newItem) => {
            const existing = prev.find((p) => p.id === newItem.id);
            if (existing && existing.status !== "pending") {
              return existing;
            }
            return newItem;
          });
        });
      }
    } catch (e) {
      setTransferStatus("failed");
      alert(`${t.statusFailed}: ${e}`);
    }
  };

  const handleCloseApp = async () => {
    try {
      await invoke("close_app");
    } catch (e) {
      console.error("close_app error:", e);
      try {
        const win = getCurrentWindow();
        await win.close();
      } catch (err) {
        console.error("Close window error:", err);
      }
    }
  };

  const handlePauseToggle = async () => {
    try {
      if (isPaused) {
        await invoke("resume_transfer");
        setTransferStatus("transferring");
      } else {
        await invoke("pause_transfer");
        setTransferStatus("paused");
      }
    } catch (e) {
      console.error(e);
    }
  };

  const handleCancel = async () => {
    isCanceledRef.current = true;
    setTransferStatus("canceled");
    setFiles((prev) =>
      prev.map((f) =>
        f.status === "copying" || f.status === "pending"
          ? { ...f, status: "canceled" }
          : f
      )
    );
    setStats((prev) => ({
      ...prev,
      current_speed_bps: 0,
      eta_seconds: 0,
    }));
    try {
      await invoke("cancel_transfer");
    } catch (e) {
      console.error(e);
    }
  };

  const handleResetToIdle = () => {
    isCanceledRef.current = false;
    setTransferStatus("idle");
    setFiles([]);
    setStagedSources([]);
    setStagedPreview(null);
    setStats({
      total_files: 0,
      completed_files: 0,
      skipped_files: 0,
      failed_files: 0,
      total_bytes: 0,
      transferred_bytes: 0,
      current_speed_bps: 0,
      average_speed_bps: 0,
      eta_seconds: 0,
      overall_percentage: 0,
      current_file_name: "",
      current_file_transferred: 0,
      current_file_size: 0,
    });
    setSpeedHistory([]);
  };

  const handleRetryFailed = async () => {
    try {
      await invoke("retry_failed_transfer");
      setTransferStatus("transferring");
    } catch (e) {
      console.error(e);
    }
  };

  const handleToggleCompact = async (compact: boolean) => {
    setIsCompact(compact);
    try {
      await invoke("toggle_compact_mode", { compact });
    } catch (e) {
      console.error(e);
    }
  };

  const handleToggleAlwaysOnTop = async () => {
    const nextVal = !alwaysOnTop;
    setAlwaysOnTop(nextVal);
    try {
      await invoke("set_always_on_top", { alwaysOnTop: nextVal });
    } catch (e) {
      console.error(e);
    }
  };

  const handleRegisterContextMenu = async () => {
    try {
      const res = await invoke<string>("register_shell_context_menu");
      alert(res);
    } catch (e) {
      alert("Hata: " + e);
    }
  };

  const handleResolveConflict = async (
    action: "overwrite" | "older" | "rename" | "skip",
    applyToAll: boolean
  ) => {
    setActiveConflict(null);
    try {
      await invoke("resolve_conflict", { action, applyToAll });
    } catch (e) {
      console.error("Resolve conflict error:", e);
    }
  };

  return (
    <main className="w-screen h-screen overflow-hidden bg-neutral-900 text-neutral-100 flex flex-col font-sans">
      {isCompact ? (
        <CompactView
          stats={stats}
          source={stagedSources[0] || ""}
          destination={destination || ""}
          status={transferStatus}
          speedHistory={speedHistory}
          alwaysOnTop={alwaysOnTop}
          onToggleAlwaysOnTop={handleToggleAlwaysOnTop}
          onPauseToggle={handlePauseToggle}
          onCancel={handleCancel}
          onExpand={() => handleToggleCompact(false)}
          onOpenVerify={() => setVerifyModalOpen(true)}
          onCloseApp={handleCloseApp}
          lang={lang}
        />
      ) : (
        <ExpandedView
          stats={stats}
          files={files}
          speedHistory={speedHistory}
          isPaused={isPaused}
          isTransferring={isTransferring}
          transferStatus={transferStatus}
          alwaysOnTop={alwaysOnTop}
          onPauseToggle={handlePauseToggle}
          onCancel={handleCancel}
          onRetryFailed={handleRetryFailed}
          onCloseApp={handleCloseApp}
          onResetToIdle={handleResetToIdle}
          onMinimize={() => handleToggleCompact(true)}
          onToggleAlwaysOnTop={handleToggleAlwaysOnTop}
          onOpenVerify={() => setVerifyModalOpen(true)}
          onRegisterContextMenu={handleRegisterContextMenu}
          onFinishAction={onFinishAction}
          onChangeOnFinish={setOnFinishAction}
          conflictPolicy={conflictPolicy}
          onChangeConflictPolicy={setConflictPolicy}
          verifyMode={verifyMode}
          onChangeVerifyMode={setVerifyMode}
          hashAlgorithm={hashAlgorithm}
          onChangeHashAlgorithm={setHashAlgorithm}
          directIo={directIo}
          onChangeDirectIo={setDirectIo}
          stagedSources={stagedSources}
          destination={destination}
          onChangeDestination={setDestination}
          stagedPreview={stagedPreview}
          onAddFiles={handleAddFiles}
          onAddFolder={handleAddFolder}
          onBrowseDestination={handleBrowseDestination}
          onRemoveStaged={handleRemoveStaged}
          onClearStaged={handleClearStaged}
          onStartTransfer={handleStartTransfer}
          lang={lang}
          onLangChange={setLang}
        />
      )}

      {/* Verify Modal */}
      <VerifyModal
        isOpen={verifyModalOpen}
        onClose={() => setVerifyModalOpen(false)}
        lang={lang}
      />

      {/* File Conflict Resolution Modal */}
      <ConflictModal
        conflict={activeConflict}
        onResolve={handleResolveConflict}
        lang={lang}
      />

      {/* Copy & Paste Chooser Modal */}
      <CopyPasteModal
        isOpen={copyPasteModalOpen}
        source={stagedSources[0] || ""}
        destination={destination || "C:\\Users\\mehme\\Desktop"}
        onClose={() => setCopyPasteModalOpen(false)}
        onSelectExplorer={() => {
          setCopyPasteModalOpen(false);
          alert(lang === "tr" ? "Windows Gezgini ile yapıştırma seçildi." : "Pasted with Windows Explorer.");
        }}
        onSelectDeepNodeCopy={(policy) => {
          setCopyPasteModalOpen(false);
          handleStartTransfer("copy", policy);
        }}
        onSelectDeepNodeMove={() => {
          setCopyPasteModalOpen(false);
          handleStartTransfer("move");
        }}
        lang={lang}
      />
    </main>
  );
};

export default App;
