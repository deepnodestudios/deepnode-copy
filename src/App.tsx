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
import { AboutModal } from "./components/AboutModal";
import { Language, translations } from "./i18n";

const POPUP_REMEMBER_KEY = "deepnode_copy_popup_remember";
const POPUP_LAST_CHOICE_KEY = "deepnode_copy_popup_last_choice";
const SETTING_ON_FINISH = "deepnode_copy_on_finish";
const SETTING_CONFLICT = "deepnode_copy_conflict_policy";
const SETTING_VERIFY = "deepnode_copy_verify_mode";
const SETTING_HASH = "deepnode_copy_hash_algorithm";
const SETTING_DIRECT_IO = "deepnode_copy_direct_io";

const readStoredSetting = (key: string, fallback: string, allowed: string[]): string => {
  try {
    const saved = localStorage.getItem(key);
    if (saved && allowed.includes(saved)) {
      return saved;
    }
  } catch {
    // ignore
  }
  return fallback;
};

const readOnFinishSetting = (): string => {
  const saved = readStoredSetting(SETTING_ON_FINISH, "do_nothing", [
    "do_nothing",
    "close",
    "close_app",
    "sleep",
    "shutdown",
  ]);
  return saved === "close" ? "close_app" : saved;
};

export const App: React.FC = () => {
  const [isCompact, setIsCompact] = useState(false);
  const [transferStatus, setTransferStatus] = useState<TransferStatus>("idle");
  const [alwaysOnTop, setAlwaysOnTop] = useState(false);
  const [verifyModalOpen, setVerifyModalOpen] = useState(false);
  const [aboutModalOpen, setAboutModalOpen] = useState(false);
  const [copyPasteModalOpen, setCopyPasteModalOpen] = useState(false);
  const [launchMode, setLaunchMode] = useState<"copy" | "move">("copy");
  const [isInitialized, setIsInitialized] = useState(false);
  const [activeConflict, setActiveConflict] = useState<ConflictInfo | null>(null);
  const [isContextMenuRegistered, setIsContextMenuRegistered] = useState<boolean>(false);
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

  const [rememberPopupChoice, setRememberPopupChoiceState] = useState<boolean>(() => {
    try {
      return localStorage.getItem(POPUP_REMEMBER_KEY) !== "false";
    } catch {
      return true;
    }
  });
  const [lastPopupChoice, setLastPopupChoice] = useState<string | null>(() => {
    try {
      return localStorage.getItem(POPUP_LAST_CHOICE_KEY);
    } catch {
      return null;
    }
  });

  const setRememberPopupChoice = (enabled: boolean) => {
    setRememberPopupChoiceState(enabled);
    try {
      localStorage.setItem(POPUP_REMEMBER_KEY, enabled ? "true" : "false");
    } catch {
      // ignore
    }
  };

  const rememberPopupSelection = (id: string) => {
    setLastPopupChoice(id);
    try {
      localStorage.setItem(POPUP_LAST_CHOICE_KEY, id);
    } catch {
      // ignore
    }
  };

  const langRef = React.useRef(lang);
  useEffect(() => {
    langRef.current = lang;
  }, [lang]);

  const getTranslation = () => translations[langRef.current] || translations.tr;

  const resolveDestinationFolder = async (pathStr: string): Promise<string> => {
    if (!pathStr || !pathStr.trim()) return "";
    try {
      const resolved = await invoke<string>("resolve_destination_path", { path: pathStr });
      return resolved || pathStr;
    } catch {
      const clean = pathStr.trim().replace(/\//g, "\\");
      const lastSlash = clean.lastIndexOf("\\");
      if (lastSlash > 0 && clean.slice(lastSlash + 1).includes(".")) {
        return clean.slice(0, lastSlash);
      }
      return clean;
    }
  };

  const isCanceledRef = React.useRef(false);
  const lastStatsUpdateRef = React.useRef<number>(0);
  const isStartingTransferRef = React.useRef(false);
  const previewRequestIdRef = React.useRef(0);
  const [isDraggingOver, setIsDraggingOver] = useState(false);

  // Transfer settings (persisted so the next popup/session uses the same choices)
  const [onFinishAction, setOnFinishAction] = useState(() => readOnFinishSetting());
  const [conflictPolicy, setConflictPolicy] = useState(() =>
    readStoredSetting(SETTING_CONFLICT, "ask", ["ask", "older", "rename", "skip", "overwrite"])
  );
  const [verifyMode, setVerifyMode] = useState(() =>
    readStoredSetting(SETTING_VERIFY, "always", ["always", "ondifference", "never"])
  );
  const [hashAlgorithm, setHashAlgorithm] = useState(() =>
    readStoredSetting(SETTING_HASH, "xxhash3", ["xxhash3", "xxhash128", "blake3", "sha256", "md5"])
  );
  const [directIo, setDirectIo] = useState(
    () => readStoredSetting(SETTING_DIRECT_IO, "false", ["true", "false"]) === "true"
  );

  useEffect(() => {
    try {
      localStorage.setItem(SETTING_ON_FINISH, onFinishAction);
    } catch {
      // ignore
    }
  }, [onFinishAction]);
  useEffect(() => {
    try {
      localStorage.setItem(SETTING_CONFLICT, conflictPolicy);
    } catch {
      // ignore
    }
  }, [conflictPolicy]);
  useEffect(() => {
    try {
      localStorage.setItem(SETTING_VERIFY, verifyMode);
    } catch {
      // ignore
    }
  }, [verifyMode]);
  useEffect(() => {
    try {
      localStorage.setItem(SETTING_HASH, hashAlgorithm);
    } catch {
      // ignore
    }
  }, [hashAlgorithm]);
  useEffect(() => {
    try {
      localStorage.setItem(SETTING_DIRECT_IO, directIo ? "true" : "false");
    } catch {
      // ignore
    }
  }, [directIo]);

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

  const isTransferring =
    transferStatus === "transferring" || transferStatus === "scanning" || transferStatus === "paused";
  const isPaused = transferStatus === "paused";

  const [excludedPaths, setExcludedPaths] = useState<string[]>([]);

  const isCompactRef = React.useRef(isCompact);
  const isTransferringRef = React.useRef(isTransferring);
  const transferSettingsRef = React.useRef({
    onFinishAction,
    conflictPolicy,
    verifyMode,
    hashAlgorithm,
    directIo,
  });
  useEffect(() => {
    isCompactRef.current = isCompact;
  }, [isCompact]);
  useEffect(() => {
    isTransferringRef.current = isTransferring;
  }, [isTransferring]);
  useEffect(() => {
    transferSettingsRef.current = {
      onFinishAction,
      conflictPolicy,
      verifyMode,
      hashAlgorithm,
      directIo,
    };
  }, [onFinishAction, conflictPolicy, verifyMode, hashAlgorithm, directIo]);

  useEffect(() => {
    document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";
    document.documentElement.lang = lang;
  }, [lang]);

  const updatePreview = async (sources: string[]) => {
    const reqId = ++previewRequestIdRef.current;

    if (isTransferringRef.current) {
      return;
    }
    if (sources.length === 0) {
      setStagedPreview(null);
      setFiles([]);
      setStats((prev) => ({
        ...prev,
        total_files: 0,
        total_bytes: 0,
        current_file_name: "",
        current_file_size: 0,
      }));
      return;
    }
    try {
      const res = await invoke<ScanPreviewResult>("scan_items_preview", { sources });
      if (reqId === previewRequestIdRef.current) {
        setStagedPreview(res);
        if (res && res.items) {
          setFiles(res.items);
          setStats((prev) => ({
            ...prev,
            total_files: res.total_files,
            total_bytes: res.total_bytes,
          }));
        }
      }
    } catch (e) {
      if (reqId === previewRequestIdRef.current) {
        console.error("Preview scan error:", e);
      }
    }
  };

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | null = null;
    let unlistenDragDrop: (() => void) | null = null;
    let unlistenPopup: (() => void) | null = null;
    let unlistenRestore: (() => void) | null = null;
    let unlistenCli: (() => void) | null = null;

    const setupApp = async () => {
      // 0. Listen for native OS drag & drop events from Windows Explorer
      try {
        const appWin = getCurrentWindow();
        const unlisten = await appWin.onDragDropEvent((event) => {
          if (!isMounted) return;
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setIsDraggingOver(true);
          } else if (event.payload.type === "leave") {
            setIsDraggingOver(false);
          } else if (event.payload.type === "drop") {
            setIsDraggingOver(false);
            const dropped = event.payload.paths;
            if (dropped && dropped.length > 0) {
              // Check if dropped onto destination input
              const pos = event.payload.position;
              let droppedOnDestination = false;
              if (pos) {
                const x = pos.x / (window.devicePixelRatio || 1);
                const y = pos.y / (window.devicePixelRatio || 1);
                const elem = document.elementFromPoint(x, y);
                if (elem && elem.closest('[data-drop-target="destination"]')) {
                  droppedOnDestination = true;
                  resolveDestinationFolder(dropped[0]).then((dest) => {
                    if (isMounted) {
                      setDestination(dest);
                    }
                  });
                }
              }

              if (!droppedOnDestination) {
                setTransferStatus((cur) => {
                  if (cur === "completed" || cur === "canceled") {
                    setFiles([]);
                    return "idle";
                  }
                  return cur;
                });

                setStagedSources((prev) => {
                  const updated = Array.from(new Set([...prev, ...dropped]));
                  updatePreview(updated);
                  return updated;
                });
              }
            }
          }
        });
        if (!isMounted) {
          unlisten();
        } else {
          unlistenDragDrop = unlisten;
        }
      } catch (dragErr) {
        console.error("Error setting onDragDropEvent:", dragErr);
      }

      // 1. Listen for real-time engine events from Tauri Rust backend
      try {
        const unlisten = await listen<EngineEvent>("engine_event", (event) => {
          if (!isMounted) return;
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

            case "ProgressUpdated": {
              const now = performance.now();
              if (now - lastStatsUpdateRef.current >= 80) {
                lastStatsUpdateRef.current = now;
                setStats(data.payload);
                setSpeedHistory((prev) => {
                  const speedMb = data.payload.current_speed_bps / 1_048_576;
                  const updated = [...prev, speedMb];
                  return updated.length > 50 ? updated.slice(updated.length - 50) : updated;
                });
              }
              break;
            }

            case "FileProgress":
              // Performance optimization: Live file progress is displayed using
              // stats.current_file_transferred and stats.overall_percentage.
              // Omitting per-chunk full table re-renders prevents UI freezing on right-click context menu.
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

            case "TransferCompleted": {
              if (isCanceledRef.current) {
                setTransferStatus("canceled");
                break;
              }
              const completedStats = data.payload;
              const hasFailures = completedStats.failed_files > 0;
              setStats(completedStats);
              setTransferStatus((prev) =>
                prev === "canceled" ? "canceled" : hasFailures ? "failed" : "completed"
              );
              Sentry.addBreadcrumb({
                category: "transfer",
                message: `Transfer completed: ${completedStats.completed_files}/${completedStats.total_files} files (${completedStats.transferred_bytes} bytes, ${completedStats.failed_files} failed)`,
                level: hasFailures ? "warning" : "info",
              });
              if (
                !hasFailures &&
                (isCompactRef.current || transferSettingsRef.current.onFinishAction === "close_app")
              ) {
                setTimeout(() => {
                  handleCloseApp();
                }, 1200);
              }
              break;
            }

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
              handleCloseApp();
              break;

            case "TransferFailed":
              setTransferStatus("failed");
              Sentry.captureMessage(`DeepNode Transfer Failed: ${data.payload}`, "error");
              alert(`${getTranslation().statusFailed}: ${data.payload}`);
              break;

            case "ConflictDetected":
              setActiveConflict(data.payload);
              break;

            default:
              break;
          }
        });
        if (!isMounted) {
          unlisten();
        } else {
          unlistenFn = unlisten;
        }
      } catch (err) {
        console.error("Error setting up event listener:", err);
      }

      // 1.1 Listen for Explorer Ctrl+V / Drag-Drop interceptor modal trigger
      try {
        const unlisten = await listen<{ sources: string[]; destination: string; mode?: string }>(
          "open_copy_paste_modal",
          (event) => {
            if (!isMounted) return;
            if (event.payload && event.payload.sources && event.payload.sources.length > 0) {
              setStagedSources(event.payload.sources);
              updatePreview(event.payload.sources);
              setDestination(event.payload.destination || "");
              setLaunchMode(event.payload.mode === "move" ? "move" : "copy");
              setCopyPasteModalOpen(true);
            }
          }
        );
        if (!isMounted) {
          unlisten();
        } else {
          unlistenPopup = unlisten;
        }
      } catch (err) {
        console.error("Error setting up open_copy_paste_modal listener:", err);
      }

      // 1.15 Listen for main window restore signal from primary/secondary instance activation
      try {
        const unlisten = await listen("restore_main_ui", () => {
          if (!isMounted) return;
          setCopyPasteModalOpen(false);
          setIsCompact(false);
        });
        if (!isMounted) {
          unlisten();
        } else {
          unlistenRestore = unlisten;
        }
      } catch (err) {
        console.error("Error setting up restore_main_ui listener:", err);
      }

      // 1.2 Listen for forwarded CLI arguments from secondary instances
      try {
        const unlisten = await listen<{
          sources: string[];
          destination?: string;
          auto_start: boolean;
          mode: string;
        }>("cli_args_forwarded", async (event) => {
          if (!isMounted) return;
          const p = event.payload;
          if (p.sources && p.sources.length > 0) {
            setStagedSources(p.sources);
            updatePreview(p.sources);
            setLaunchMode(p.mode === "move" ? "move" : "copy");
            if (p.destination) {
              setDestination(p.destination);
            }
            if (p.auto_start && p.destination) {
              const settings = transferSettingsRef.current;
              setExcludedPaths([]);
              setIsCompact(true);
              invoke("toggle_compact_mode", { compact: true }).catch(() => {});
              setTransferStatus("transferring");
              try {
                const init = await invoke<StartTransferResult>("start_transfer", {
                  sources: p.sources,
                  destination: p.destination,
                  mode: p.mode || "copy",
                  verifyMode: settings.verifyMode,
                  hashAlgorithm: settings.hashAlgorithm,
                  conflictPolicy: settings.conflictPolicy,
                  directIo: settings.directIo,
                  onFinish: settings.onFinishAction,
                });
                if (init && init.items) {
                  setStats((prev) => ({
                    ...prev,
                    total_files: init.total_files,
                    total_bytes: init.total_bytes,
                    current_file_name: init.items[0]?.relative_path || init.items[0]?.src_path || "",
                    current_file_size: init.items[0]?.size || 0,
                  }));
                  setFiles(init.items);
                }
              } catch (e) {
                console.error("Auto transfer start error:", e);
                setTransferStatus("failed");
              }
            }
          }
        });
        if (!isMounted) {
          unlisten();
        } else {
          unlistenCli = unlisten;
        }
      } catch (err) {
        console.error("Error setting up cli_args_forwarded listener:", err);
      }

      // 2. Check current transfer state (if auto-started by Rust) or launch args
      try {
        const curState = await invoke<CurrentTransferState>("get_current_transfer_state");
        if (
          curState &&
          (curState.status === "transferring" ||
            curState.status === "scanning" ||
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
          if (curState.active_conflict) {
            setActiveConflict(curState.active_conflict);
          }
          setIsInitialized(true);
          invoke("app_ready_to_show").catch(() => {});
          return;
        }

        const args = curState?.launch_args || (await invoke<LaunchArgs>("get_launch_args"));
        if (args.sources && args.sources.length > 0) {
          setStagedSources(args.sources);
          updatePreview(args.sources);
          setLaunchMode(args.mode === "move" ? "move" : "copy");
          if (args.destination) {
            setDestination(args.destination);
          }
          if (args.is_popup) {
            setCopyPasteModalOpen(true);
            setIsCompact(false);
          } else if (args.auto_start && args.destination) {
            if (isStartingTransferRef.current) return;
            isStartingTransferRef.current = true;
            const settings = transferSettingsRef.current;
            setExcludedPaths([]);
            // Automatically launch transfer
            setIsCompact(true);
            invoke("toggle_compact_mode", { compact: true }).catch(() => {});
            setTransferStatus("transferring");

            try {
              const init = await invoke<StartTransferResult>("start_transfer", {
                sources: args.sources,
                destination: args.destination,
                mode: args.mode || "copy",
                verifyMode: settings.verifyMode,
                hashAlgorithm: settings.hashAlgorithm,
                conflictPolicy: args.conflict_policy || settings.conflictPolicy,
                directIo: settings.directIo,
                onFinish: args.on_finish || settings.onFinishAction,
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
            } catch (err) {
              setTransferStatus("failed");
              console.error("Auto transfer error:", err);
            } finally {
              isStartingTransferRef.current = false;
            }
          }
        }
        // Check if context menu is registered
        invoke<boolean>("is_shell_context_menu_registered")
          .then((registered) => setIsContextMenuRegistered(registered))
          .catch(() => {});
      } catch (err) {
        console.error("Error reading launch args:", err);
      } finally {
        setIsInitialized(true);
        invoke("app_ready_to_show").catch(() => {});
      }
    };

    setupApp();

    return () => {
      isMounted = false;
      if (unlistenFn) unlistenFn();
      if (unlistenDragDrop) unlistenDragDrop();
      if (unlistenPopup) unlistenPopup();
      if (unlistenRestore) unlistenRestore();
      if (unlistenCli) unlistenCli();
    };
  }, []);

  useEffect(() => {
    if (activeConflict) {
      if (isCompact) {
        invoke("set_conflict_mode", { active: true, isCompact: true }).catch(() => {});
      }
    }
  }, [activeConflict, isCompact]);

  const handleAddFiles = async () => {
    try {
      const picked = await invoke<string[]>("pick_source_files");
      if (picked && picked.length > 0) {
        setExcludedPaths([]);
        setStagedSources((prev) => {
          const updated = Array.from(new Set([...prev, ...picked]));
          updatePreview(updated);
          return updated;
        });
      }
    } catch (e) {
      console.error("pick_source_files error:", e);
    }
  };

  const handleAddFolder = async () => {
    try {
      const folder = await invoke<string | null>("pick_source_folder");
      if (folder) {
        setExcludedPaths([]);
        setStagedSources((prev) => {
          const updated = Array.from(new Set([...prev, folder]));
          updatePreview(updated);
          return updated;
        });
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

  const handleRemoveStaged = (indices: number[] | number) => {
    const toRemove = new Set(Array.isArray(indices) ? indices : [indices]);
    if (toRemove.size === 0) return;

    setExcludedPaths([]);
    setStagedSources((prev) => {
      const updated = prev.filter((_, idx) => !toRemove.has(idx));
      if (updated.length === 0) {
        handleClearStaged();
      } else {
        updatePreview(updated);
      }
      return updated;
    });
  };

  const handleClearStaged = () => {
    if (isTransferringRef.current) return;
    setStagedSources([]);
    setStagedPreview(null);
    setFiles([]);
    setExcludedPaths([]);
  };

  const handleStartTransfer = async (mode: "copy" | "move", overridePolicy?: string) => {
    if (isStartingTransferRef.current) return;
    if (transferStatus === "transferring" || transferStatus === "scanning" || transferStatus === "paused") {
      return;
    }
    if (stagedSources.length === 0 && files.length === 0) {
      alert(getTranslation().noFilesFound);
      return;
    }

    let target = destination;
    if (!target) {
      const picked = await invoke<string | null>("pick_destination_folder");
      if (!picked) return;
      target = picked;
      setDestination(picked);
    } else {
      target = await resolveDestinationFolder(target);
      if (target !== destination) {
        setDestination(target);
      }
    }

    if (isStartingTransferRef.current) return;
    isStartingTransferRef.current = true;
    setTransferStatus("transferring");
    setSpeedHistory([]);

    try {
      const sourcesToTransfer = stagedSources.length > 0 ? stagedSources : files.map((f) => f.src_path);

      const init = await invoke<StartTransferResult>("start_transfer", {
        sources: sourcesToTransfer,
        destination: target,
        mode,
        verifyMode,
        hashAlgorithm,
        conflictPolicy: overridePolicy || conflictPolicy,
        directIo,
        onFinish: onFinishAction,
        excludePaths: excludedPaths,
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
      alert(`${getTranslation().statusFailed}: ${e}`);
    } finally {
      isStartingTransferRef.current = false;
    }
  };

  const handleCloseApp = async () => {
    try {
      await Sentry.flush(1500).catch(() => false); // send queued events before the process exits
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
      console.error("Pause/resume error:", e);
      alert(`${getTranslation().pause}: ${e}`);
    }
  };

  const handleCancel = async () => {
    isCanceledRef.current = true;
    setTransferStatus("canceled");
    setStats((prev) => ({
      ...prev,
      current_speed_bps: 0,
      eta_seconds: 0,
    }));
    try {
      await invoke("cancel_transfer");
    } catch (e) {
      console.error("cancel_transfer error:", e);
    }
    // As explicitly requested by user: clicking cancel aborts and closes the window
    await handleCloseApp();
  };

  const handleSkipCurrent = async () => {
    try {
      await invoke("skip_current_file");
    } catch (e) {
      console.error("skip_current_file error:", e);
      alert(`${getTranslation().skipFile}: ${e}`);
    }
  };

  const handleSkipFiles = async (ids: number[]) => {
    try {
      await invoke("skip_files_by_id", { ids });
      setFiles((prev) =>
        prev.map((f) => (ids.includes(f.id) ? { ...f, status: "skipped" } : f))
      );
    } catch (e) {
      console.error("skip_files_by_id error:", e);
      alert(`${getTranslation().skipFile}: ${e}`);
    }
  };

  const handleRemoveQueued = async (ids: number[]) => {
    if (!isTransferringRef.current) {
      const removed = files.filter((f) => ids.includes(f.id));
      if (removed.length === 0) return;
      const remaining = files.filter((f) => !ids.includes(f.id));
      if (remaining.length === 0) {
        handleClearStaged();
        return;
      }
      // The engine rescans the staged sources, so removed preview entries must be excluded explicitly.
      setExcludedPaths((prev) => Array.from(new Set([...prev, ...removed.map((f) => f.src_path)])));
      setFiles(remaining);
      setStats((s) => ({
        ...s,
        total_files: remaining.length,
        total_bytes: remaining.reduce((acc, f) => acc + f.size, 0),
      }));
      return;
    }
    try {
      await invoke("remove_queued_files", { ids });
      setFiles((prev) => prev.filter((f) => !ids.includes(f.id) || f.status === "completed"));
    } catch (e) {
      console.error("remove_queued_files error:", e);
      alert(`${getTranslation().statusFailed}: ${e}`);
    }
  };

  const handleShowInFolder = async (path: string) => {
    try {
      await invoke("show_in_folder", { path });
    } catch (e) {
      console.error("show_in_folder error:", e);
    }
  };

  const handleOpenFolder = async (path: string) => {
    try {
      await invoke("open_folder", { path });
    } catch (e) {
      console.error("open_folder error:", e);
    }
  };

  const handleResetToIdle = () => {
    if (isTransferringRef.current) {
      alert(
        lang === "tr"
          ? "Aktif aktarım sürerken kuyruk temizlenemez. Önce duraklatıp iptal edin."
          : "The queue cannot be cleared during an active transfer. Pause and cancel it first."
      );
      return;
    }
    isCanceledRef.current = false;
    setTransferStatus("idle");
    setFiles([]);
    setStagedSources([]);
    setStagedPreview(null);
    setExcludedPaths([]);
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
    if (isStartingTransferRef.current) return;
    if (transferStatus === "transferring" || transferStatus === "scanning" || transferStatus === "paused") {
      return;
    }
    isStartingTransferRef.current = true;
    isCanceledRef.current = false;
    try {
      await invoke("retry_failed_transfer");
      setTransferStatus("transferring");
    } catch (e) {
      console.error("retry_failed_transfer error:", e);
      const curT = getTranslation();
      alert(`${curT.retryFailed || curT.retryDownload}: ${e}`);
    } finally {
      isStartingTransferRef.current = false;
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
      const res = await invoke<string>("register_shell_context_menu", { lang });
      setIsContextMenuRegistered(true);
      alert(res);
    } catch (e) {
      alert("Hata: " + e);
    }
  };

  const handleUnregisterContextMenu = async () => {
    try {
      const res = await invoke<string>("unregister_shell_context_menu", { lang });
      setIsContextMenuRegistered(false);
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
    if (isCompact) {
      invoke("set_conflict_mode", { active: false, isCompact: true }).catch(() => {});
    }
    try {
      await invoke("resolve_conflict", { action, applyToAll });
    } catch (e) {
      console.error("Resolve conflict error:", e);
    }
  };

  const handleMinimizeWindow = async () => {
    try {
      await invoke("minimize_window");
    } catch (e) {
      console.error("minimize_window error:", e);
      try {
        const win = getCurrentWindow();
        await win.minimize();
      } catch (err) {
        console.error("win.minimize error:", err);
      }
    }
  };

  const handleRerunSession = (sources: string[], dest: string) => {
    if (isTransferring) {
      alert(lang === "tr" ? "Aktif bir aktarım devam ederken yeni bir aktarım yüklenemez." : "Cannot load transfer while another transfer is active.");
      return;
    }
    isCanceledRef.current = false;
    setExcludedPaths([]);
    if (sources.length > 0) {
      setStagedSources(sources);
      updatePreview(sources);
    }
    if (dest) {
      setDestination(dest);
    }
    if (transferStatus === "completed" || transferStatus === "canceled" || transferStatus === "failed") {
      setTransferStatus("idle");
    }
  };

  const handleToggleMaximizeWindow = async () => {
    try {
      await invoke("toggle_maximize_window");
    } catch (e) {
      console.error("toggle_maximize_window error:", e);
      try {
        const win = getCurrentWindow();
        await win.toggleMaximize();
      } catch (err) {
        console.error("win.toggleMaximize error:", err);
      }
    }
  };

  if (!isInitialized) {
    return <main className="w-screen h-screen bg-[#070a12] select-none" />;
  }

  return (
    <main data-tauri-drag-region={copyPasteModalOpen ? "false" : "deep"} className="w-screen h-screen overflow-hidden bg-[#070a12] text-neutral-100 flex flex-col font-sans select-none">
      {copyPasteModalOpen ? (
        <CopyPasteModal
          isOpen={true}
          mode={launchMode}
          source={
            stagedSources.length > 1
              ? `${stagedSources[0]} (+${stagedSources.length - 1} ${translations[lang]?.selectedItems || "öğe"})`
              : stagedSources[0] || ""
          }
          destination={destination || ""}
          onClose={async () => {
            setCopyPasteModalOpen(false);
            try {
              await Sentry.flush(1500).catch(() => false); // send queued events before the process exits
              await invoke("close_app");
            } catch (e) {
              console.error(e);
            }
          }}
          onSelectExplorer={async () => {
            setCopyPasteModalOpen(false);
            try {
              await invoke("explorer_copy", {
                sources: stagedSources,
                destination: destination || "",
                mode: launchMode,
              });
            } catch (e) {
              console.error("Explorer copy failed:", e);
              alert(`${getTranslation().statusFailed}: ${e}`);
              setCopyPasteModalOpen(true);
              return;
            }
            try {
              await Sentry.flush(1500).catch(() => false); // send queued events before the process exits
              await invoke("close_app");
            } catch (e) {
              console.error(e);
            }
          }}
          onSelectDeepNodeCopy={async () => {
            setCopyPasteModalOpen(false);
            setIsCompact(true);
            try {
              await invoke("toggle_compact_mode", { compact: true });
              // The chooser popup is topmost; the transfer view follows the user's pin setting.
              await invoke("set_always_on_top", { alwaysOnTop });
            } catch (e) {
              console.error(e);
            }
            handleStartTransfer(launchMode);
          }}
          lang={lang}
          rememberChoice={rememberPopupChoice}
          lastChoice={lastPopupChoice}
          onRememberChoice={rememberPopupSelection}
          onChangeRememberChoice={setRememberPopupChoice}
        />
      ) : isCompact ? (
        <CompactView
          stats={stats}
          source={stagedSources[0] || ""}
          destination={destination || ""}
          status={transferStatus}
          speedHistory={speedHistory}
          alwaysOnTop={alwaysOnTop}
          directIo={directIo}
          onToggleAlwaysOnTop={handleToggleAlwaysOnTop}
          onPauseToggle={handlePauseToggle}
          onCancel={handleCancel}
          onSkip={handleSkipCurrent}
          onRetryFailed={handleRetryFailed}
          onExpand={() => handleToggleCompact(false)}
          onOpenVerify={() => setVerifyModalOpen(true)}
          onOpenAbout={() => setAboutModalOpen(true)}
          onCloseApp={handleCloseApp}
          onMinimizeWindow={handleMinimizeWindow}
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
          onSkipCurrent={handleSkipCurrent}
          onSkipFiles={handleSkipFiles}
          onRemoveQueued={handleRemoveQueued}
          onShowInFolder={handleShowInFolder}
          onOpenFolder={handleOpenFolder}
          onRetryFailed={handleRetryFailed}
          onCloseApp={handleCloseApp}
          onMinimizeWindow={handleMinimizeWindow}
          onToggleMaximizeWindow={handleToggleMaximizeWindow}
          onResetToIdle={handleResetToIdle}
          onMinimize={() => handleToggleCompact(true)}
          onToggleAlwaysOnTop={handleToggleAlwaysOnTop}
          onOpenVerify={() => setVerifyModalOpen(true)}
          onOpenAbout={() => setAboutModalOpen(true)}
          onRegisterContextMenu={handleRegisterContextMenu}
          onUnregisterContextMenu={handleUnregisterContextMenu}
          isContextMenuRegistered={isContextMenuRegistered}
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
          isDraggingOver={isDraggingOver}
          onRerunSession={handleRerunSession}
          rememberPopupChoice={rememberPopupChoice}
          onChangeRememberPopupChoice={setRememberPopupChoice}
        />
      )}

      {/* Verify Modal */}
      <VerifyModal
        isOpen={verifyModalOpen}
        onClose={() => setVerifyModalOpen(false)}
        lang={lang}
      />

      {/* About Modal */}
      <AboutModal
        isOpen={aboutModalOpen}
        onClose={() => setAboutModalOpen(false)}
        lang={lang}
      />

      {/* File Conflict Resolution Modal */}
      <ConflictModal
        conflict={activeConflict}
        onResolve={handleResolveConflict}
        onCancelTransfer={handleCancel}
        lang={lang}
        isCompact={isCompact}
      />
    </main>
  );
};

export default App;
