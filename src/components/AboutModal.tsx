import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  X,
  User,
  Mail,
  RefreshCw,
  CheckCircle2,
  HardDriveDownload,
  Loader,
  AlertCircle,
} from "lucide-react";
import { translations, Language } from "../i18n";
import packageJson from "../../package.json";

interface AboutModalProps {
  isOpen: boolean;
  onClose: () => void;
  lang: Language;
}

interface UpdateData {
  tag: string;
  name: string;
  notes: string;
  htmlUrl: string;
  downloadUrl: string;
}

interface UpdateProgressPayload {
  status: "downloading" | "ready" | "error";
  progress: number;
  error?: string;
}

export const AboutModal: React.FC<AboutModalProps> = ({ isOpen, onClose, lang }) => {
  const [updateState, setUpdateState] = useState<"idle" | "checking" | "available" | "upToDate" | "error">("idle");
  const [updateData, setUpdateData] = useState<UpdateData | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  // Download & Install progress states (identical to DDM)
  const [dlStatus, setDlStatus] = useState<"idle" | "downloading" | "ready" | "error">("idle");
  const [dlProgress, setDlProgress] = useState(0);
  const [dlError, setDlError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const installTriggeredRef = useRef(false);

  const t = translations[lang] || translations.tr;
  const [currentVersion, setCurrentVersion] = useState(packageJson.version);
  const [stagedUpdate, setStagedUpdate] = useState<{ version: string; is_setup: boolean } | null>(null);

  useEffect(() => {
    invoke<string>("get_app_version")
      .then((ver) => {
        if (ver) setCurrentVersion(ver);
      })
      .catch(() => {});

    if (isOpen) {
      invoke<any>("get_staged_update")
        .then((res) => {
          if (res && res.version) {
            setStagedUpdate({ version: res.version, is_setup: !!res.is_setup });
          }
        })
        .catch(() => {});
    }
  }, [isOpen]);

  const compareVersions = (v1: string, v2: string): number => {
    const parse = (v: string) =>
      String(v || "")
        .replace(/^v/i, "")
        .split("-")[0]
        .split(".")
        .map((part) => {
          const n = parseInt(part, 10);
          return Number.isFinite(n) ? n : 0;
        });

    const p1 = parse(v1);
    const p2 = parse(v2);
    for (let i = 0; i < 3; i++) {
      const n1 = p1[i] || 0;
      const n2 = p2[i] || 0;
      if (n1 > n2) return 1;
      if (n1 < n2) return -1;
    }
    return 0;
  };

  const checkUpdates = async () => {
    setUpdateState("checking");
    setUpdateError(null);
    setUpdateData(null);
    setDlStatus("idle");
    setDlProgress(0);
    setDlError(null);
    setInstalling(false);
    installTriggeredRef.current = false;

    try {
      const liveVer = await invoke<string>("get_app_version").catch(() => currentVersion);
      if (liveVer) setCurrentVersion(liveVer);

      const isPortable = await invoke<boolean>("is_portable_app").catch(() => true);

      let release: any;
      try {
        release = await invoke("check_latest_release");
      } catch {
        const response = await fetch("https://api.github.com/repos/deepnodestudios/deepnode-copy/releases/latest", {
          headers: { Accept: "application/vnd.github.v3+json" },
        });
        if (!response.ok) {
          throw new Error(`HTTP ${response.status}`);
        }
        release = await response.json();
      }

      const latestTag = release.tag_name || "";
      const isNewer = compareVersions(latestTag, liveVer || currentVersion) > 0;

      let downloadAssetUrl = release.html_url;
      if (Array.isArray(release.assets)) {
        if (isPortable) {
          // Portable standalone executable priority (e.g. DeepNodeCopy.exe)
          const portableAsset = release.assets.find(
            (a: any) =>
              a.name?.toLowerCase() === "deepnodecopy.exe" ||
              (a.name?.toLowerCase().endsWith(".exe") && !a.name?.toLowerCase().includes("setup"))
          );
          if (portableAsset?.browser_download_url) {
            downloadAssetUrl = portableAsset.browser_download_url;
          }
        }

        // If not portable, or if portable asset not found, fallback to setup.exe
        if (!downloadAssetUrl || downloadAssetUrl === release.html_url) {
          const setupAsset =
            release.assets.find((a: any) => a.name?.toLowerCase() === "deepnodecopy_setup.exe") ||
            release.assets.find(
              (a: any) =>
                a.name?.toLowerCase().includes("setup") &&
                a.name?.toLowerCase().endsWith(".exe")
            ) ||
            release.assets.find((a: any) => a.name?.toLowerCase().endsWith(".exe"));

          if (setupAsset?.browser_download_url) {
            downloadAssetUrl = setupAsset.browser_download_url;
          }
        }
      }

      const info: UpdateData = {
        tag: latestTag,
        name: release.name || latestTag,
        notes: release.body || "",
        htmlUrl: release.html_url,
        downloadUrl: downloadAssetUrl,
      };

      setUpdateData(info);

      if (isNewer) {
        setUpdateState("available");
      } else {
        setUpdateState("upToDate");
      }
    } catch (err: any) {
      console.error("Update check error:", err);
      setUpdateError(err.message || String(err));
      setUpdateState("error");
    }
  };

  useEffect(() => {
    if (isOpen) {
      checkUpdates();
    } else {
      setUpdateState("idle");
      setUpdateData(null);
      setUpdateError(null);
      setDlStatus("idle");
      setDlProgress(0);
      setDlError(null);
      setInstalling(false);
      installTriggeredRef.current = false;
    }
  }, [isOpen]);

  // Listen to native Tauri Rust update progress events
  useEffect(() => {
    if (!isOpen) return;
    let isMounted = true;
    let unlisten: (() => void) | null = null;
    let unlistenStaged: (() => void) | null = null;

    listen<UpdateProgressPayload>("update_progress", (event) => {
      if (!isMounted) return;
      const p = event.payload;
      if (p.status === "downloading") {
        setDlStatus("downloading");
        setDlProgress(p.progress || 0);
      } else if (p.status === "ready") {
        setDlStatus("ready");
        setDlProgress(100);
        if (!installTriggeredRef.current) {
          installTriggeredRef.current = true;
          triggerInstall();
        }
      } else if (p.status === "error") {
        setDlStatus("error");
        setDlError(p.error || "Download error");
      }
    })
      .then((fn) => {
        if (!isMounted) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((err) => {
        console.error("Failed to setup update_progress listener:", err);
      });

    listen<any>("staged_update_ready", (event) => {
      if (!isMounted) return;
      if (event.payload && event.payload.version) {
        setStagedUpdate({ version: event.payload.version, is_setup: !!event.payload.is_setup });
      }
    })
      .then((fn) => {
        if (!isMounted) {
          fn();
        } else {
          unlistenStaged = fn;
        }
      })
      .catch((err) => {
        console.error("Failed to setup staged_update_ready listener:", err);
      });

    return () => {
      isMounted = false;
      if (unlisten) unlisten();
      if (unlistenStaged) unlistenStaged();
    };
  }, [isOpen]);

  const startDownload = async () => {
    if (!updateData?.downloadUrl) return;
    setDlStatus("downloading");
    setDlProgress(0);
    setDlError(null);

    try {
      await invoke("start_update_download", { url: updateData.downloadUrl });
    } catch (err: any) {
      setDlStatus("error");
      setDlError(err?.message || String(err));
    }
  };

  const triggerInstall = async () => {
    setInstalling(true);
    try {
      await invoke("install_app_update");
    } catch (err: any) {
      setInstalling(false);
      setDlStatus("error");
      setDlError(err?.message || String(err));
    }
  };

  const handleOpenUrl = (url: string) => {
    invoke("open_external_url", { url }).catch(() => {
      window.open(url, "_blank");
    });
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 select-none animate-in fade-in duration-150"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[420px] bg-neutral-900 border border-neutral-700/80 rounded-2xl shadow-2xl overflow-hidden flex flex-col animate-in zoom-in-95 duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 bg-neutral-950/80 border-b border-neutral-800">
          <span className="text-sm font-semibold text-neutral-100">{t.aboutTitle}</span>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-neutral-800 text-neutral-400 hover:text-neutral-200 transition cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="p-6 flex flex-col items-center text-center">
          {/* Brand Icon */}
          <div className="w-[72px] h-[72px] mb-3 flex items-center justify-center rounded-2xl bg-neutral-950 border border-neutral-700/70 shadow-lg p-1 overflow-hidden">
            <img src="/branding/app-icon.png" alt="DeepNode Copy" className="w-full h-full object-contain rounded-xl" />
          </div>

          <h3 className="text-lg font-bold text-neutral-100 tracking-wide">{t.appTitle}</h3>
          <p className="text-xs text-neutral-400 font-mono mt-0.5 mb-4">
            {t.versionLabel} {currentVersion}
          </p>

          {/* Developer details */}
          <div className="flex flex-col gap-2 items-center text-xs text-neutral-300">
            <div className="flex items-center gap-2">
              <User className="w-4 h-4 text-indigo-400" />
              <span>DeepNode Studios</span>
            </div>
            <div className="flex items-center gap-2">
              <Mail className="w-4 h-4 text-indigo-400" />
              <button
                type="button"
                onClick={() => handleOpenUrl("mailto:deepnodestudios@gmail.com")}
                className="text-indigo-400 hover:text-indigo-300 hover:underline cursor-pointer"
              >
                deepnodestudios@gmail.com
              </button>
            </div>
          </div>

          {/* Update section */}
          <div className="w-full mt-5 pt-4 border-t border-neutral-800/80 flex flex-col items-center">
            <button
              onClick={checkUpdates}
              disabled={updateState === "checking" || dlStatus === "downloading" || installing}
              className="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-neutral-800 hover:bg-neutral-700/90 border border-neutral-700/80 text-xs font-medium text-neutral-200 transition shadow-sm disabled:opacity-60 cursor-pointer"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${updateState === "checking" ? "animate-spin text-indigo-400" : ""}`} />
              <span>{updateState === "checking" ? t.checkingUpdates : t.checkUpdates}</span>
            </button>

            {/* Silent update ready / staged card */}
            {stagedUpdate && updateState !== "available" && (
              <div className="w-full mt-3 p-3 rounded-xl bg-indigo-950/40 border border-indigo-700/50 flex flex-col gap-1.5 text-left animate-in fade-in duration-200">
                <div className="flex items-center gap-1.5 text-xs text-indigo-300 font-semibold">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <span>{lang === "tr" ? "Güncelleme Hazır" : "Update Ready"} (v{stagedUpdate.version})</span>
                </div>
                <p className="text-[11px] text-neutral-400 leading-relaxed">
                  {lang === "tr"
                    ? "Yeni sürüm arka planda sessizce indirildi. Uygulama kapatıldığında veya bir sonraki kopyalama işleminde otomatik olarak uygulanacaktır."
                    : "The update was downloaded silently in the background. It will be applied automatically when the app closes or on the next copy task."}
                </p>
                <button
                  type="button"
                  onClick={triggerInstall}
                  disabled={installing}
                  className="mt-1 w-full py-1.5 px-3 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-medium text-xs transition flex items-center justify-center gap-1.5 cursor-pointer shadow-sm"
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${installing ? "animate-spin" : ""}`} />
                  <span>{installing ? (lang === "tr" ? "Uygulanıyor..." : "Applying...") : (lang === "tr" ? "Şimdi Yeniden Başlat ve Güncelle" : "Restart & Update Now")}</span>
                </button>
              </div>
            )}

            {/* Up to date */}
            {updateState === "upToDate" && !stagedUpdate && (
              <div className="mt-3 flex items-center gap-1.5 text-xs text-emerald-400 font-medium bg-emerald-950/30 border border-emerald-900/50 px-3 py-1.5 rounded-lg">
                <CheckCircle2 className="w-4 h-4" />
                <span>{t.upToDate}</span>
              </div>
            )}

            {/* Update available */}
            {updateState === "available" && updateData && (
              <div className="w-full mt-3 p-3.5 rounded-lg bg-neutral-950 border border-indigo-800/60 flex flex-col gap-2.5 text-left">
                <div className="flex items-center justify-between text-xs">
                  <span className="font-semibold text-neutral-200">{t.updateAvailable}</span>
                  <span className="text-indigo-400 font-mono font-bold">{updateData.tag}</span>
                </div>

                {updateData.notes && (
                  <div className="text-[11px] text-neutral-400 max-h-24 overflow-y-auto whitespace-pre-wrap bg-neutral-900/80 p-2 rounded border border-neutral-800 font-mono">
                    {updateData.notes}
                  </div>
                )}

                {/* Download / Install states identical to DDM */}
                {dlStatus === "idle" && (
                  <button
                    onClick={startDownload}
                    className="w-full py-2 px-3 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-medium text-xs transition flex items-center justify-center gap-2 cursor-pointer shadow-md shadow-indigo-950"
                  >
                    <HardDriveDownload className="w-4 h-4" />
                    <span>{t.downloadInstall}</span>
                  </button>
                )}

                {dlStatus === "downloading" && (
                  <div className="flex flex-col gap-1.5 mt-1">
                    <div className="flex items-center justify-between text-[11px] text-neutral-300">
                      <div className="flex items-center gap-1.5">
                        <Loader className="w-3.5 h-3.5 animate-spin text-indigo-400" />
                        <span>{t.downloadingPct.replace("{n}", dlProgress.toString())}</span>
                      </div>
                      <span className="font-mono text-indigo-400">{dlProgress}%</span>
                    </div>
                    <div className="w-full h-2 bg-neutral-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-gradient-to-r from-indigo-500 to-cyan-400 transition-all duration-300 rounded-full"
                        style={{ width: `${dlProgress}%` }}
                      />
                    </div>
                  </div>
                )}

                {dlStatus === "ready" && (
                  <div className="flex items-center justify-center gap-2 py-1 text-xs text-emerald-400 font-medium">
                    <Loader className="w-3.5 h-3.5 animate-spin text-emerald-400" />
                    <span>{t.willInstall}</span>
                  </div>
                )}

                {installing && (
                  <div className="text-center text-xs text-amber-400 py-1 font-medium">
                    {t.installStarted}
                  </div>
                )}

                {dlStatus === "error" && (
                  <div className="flex flex-col gap-1.5 mt-1">
                    <div className="text-xs text-rose-400 font-mono break-all">{dlError || t.updateCheckError}</div>
                    <button
                      onClick={startDownload}
                      className="w-full py-1.5 px-3 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 text-xs font-medium transition cursor-pointer"
                    >
                      {t.retryDownload}
                    </button>
                  </div>
                )}
              </div>
            )}

            {/* General Check Error */}
            {updateState === "error" && (
              <div className="mt-3 flex items-center gap-1.5 text-xs text-rose-400 bg-rose-950/30 border border-rose-900/50 px-3 py-1.5 rounded-lg">
                <AlertCircle className="w-4 h-4 flex-shrink-0" />
                <span>{updateError || t.updateCheckError}</span>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="px-5 py-3 bg-neutral-950/80 border-t border-neutral-800 flex justify-end">
          <button
            onClick={onClose}
            className="px-5 py-1.5 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg text-xs font-semibold shadow-md shadow-indigo-950 transition cursor-pointer"
          >
            {t.close}
          </button>
        </div>
      </div>
    </div>
  );
};
