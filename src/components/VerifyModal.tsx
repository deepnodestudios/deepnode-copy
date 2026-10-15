import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CheckCircle2, XCircle, X, Clipboard, ShieldCheck } from "lucide-react";
import { translations, Language } from "../i18n";

interface VerifyModalProps {
  isOpen: boolean;
  onClose: () => void;
  lang: Language;
}

export const VerifyModal: React.FC<VerifyModalProps> = ({ isOpen, onClose, lang }) => {
  const [filePath, setFilePath] = useState("");
  const [expectedHash, setExpectedHash] = useState("");
  const [algorithm, setAlgorithm] = useState("xxhash64");
  const [result, setResult] = useState<{
    calculated?: string;
    matches?: boolean;
    error?: string;
  } | null>(null);
  const [loading, setLoading] = useState(false);

  if (!isOpen) return null;
  const t = translations[lang];

  const handleBrowseFile = async () => {
    try {
      const files = await invoke<string[]>("pick_source_files");
      if (files && files.length > 0) {
        setFilePath(files[0]);
      }
    } catch {
      // dialog cancelled
    }
  };

  const handlePasteClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setExpectedHash(text.trim());
    } catch {
      // clipboard access fallback
    }
  };

  const handleVerify = async () => {
    if (!filePath.trim()) return;
    setLoading(true);
    setResult(null);

    try {
      const data = await invoke<{
        calculated: string;
        algorithm: string;
        matches: boolean | null;
      }>("verify_file", {
        filePath: filePath.trim(),
        expected: expectedHash.trim() || null,
        algo: algorithm,
      });

      setResult({
        calculated: data.calculated,
        matches: data.matches ?? undefined,
      });
    } catch (err: any) {
      setResult({ error: String(err) });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-md bg-neutral-900 border border-neutral-800 rounded-xl shadow-2xl p-5 flex flex-col gap-4">
        <div className="flex items-center justify-between border-b border-neutral-800 pb-3">
          <div className="flex items-center gap-2 text-indigo-400">
            <ShieldCheck className="w-5 h-5" />
            <h3 className="text-sm font-semibold text-neutral-100">{t.clipboardTest}</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-neutral-800 text-neutral-400 hover:text-neutral-200"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="flex flex-col gap-3 text-xs">
          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-neutral-400">Target File Path</label>
              <button
                type="button"
                onClick={handleBrowseFile}
                className="flex items-center gap-1 text-[11px] text-indigo-400 hover:text-indigo-300 cursor-pointer"
              >
                Browse...
              </button>
            </div>
            <input
              type="text"
              value={filePath}
              onChange={(e) => setFilePath(e.target.value)}
              placeholder="C:\Folder\SampleFile.iso"
              className="w-full px-3 py-2 bg-neutral-950 border border-neutral-800 rounded text-neutral-200 font-mono text-xs focus:outline-none focus:border-indigo-500"
            />
          </div>

          <div>
            <div className="flex items-center justify-between mb-1">
              <label className="text-neutral-400">{t.expectedHash}</label>
              <button
                onClick={handlePasteClipboard}
                className="flex items-center gap-1 text-[11px] text-indigo-400 hover:text-indigo-300"
              >
                <Clipboard className="w-3 h-3" /> Paste Clipboard
              </button>
            </div>
            <input
              type="text"
              value={expectedHash}
              onChange={(e) => setExpectedHash(e.target.value)}
              placeholder="e.g. 7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1f..."
              className="w-full px-3 py-2 bg-neutral-950 border border-neutral-800 rounded text-neutral-200 font-mono text-xs focus:outline-none focus:border-indigo-500"
            />
          </div>

          <div className="flex items-center justify-between gap-3">
            <div className="flex-1">
              <label className="text-neutral-400 block mb-1">{t.hashAlgorithm}</label>
              <select
                value={algorithm}
                onChange={(e) => setAlgorithm(e.target.value)}
                className="w-full px-2.5 py-1.5 bg-neutral-950 border border-neutral-800 rounded text-neutral-300 text-xs focus:outline-none"
              >
                <option value="xxhash64">xxHash3 (64-bit SIMD)</option>
                <option value="xxhash128">xxHash3 (128-bit SIMD)</option>
                <option value="blake3">BLAKE3 (Multi-threaded)</option>
                <option value="sha256">SHA-256</option>
                <option value="sha1">SHA-1</option>
                <option value="md5">MD5</option>
              </select>
            </div>

            <div className="flex items-end">
              <button
                onClick={handleVerify}
                disabled={loading || !filePath.trim()}
                className="px-4 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium text-xs transition"
              >
                {loading ? "Testing..." : t.testFile}
              </button>
            </div>
          </div>

          {result && (
            <div
              className={`p-3 rounded border text-xs flex flex-col gap-1.5 ${
                result.matches === true
                  ? "bg-emerald-950/40 border-emerald-800 text-emerald-300"
                  : result.matches === false
                  ? "bg-rose-950/40 border-rose-800 text-rose-300"
                  : "bg-neutral-800/60 border-neutral-700 text-neutral-200"
              }`}
            >
              <div className="flex items-center gap-1.5 font-semibold">
                {result.matches === true ? (
                  <>
                    <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                    <span>{t.matchSuccess}</span>
                  </>
                ) : result.matches === false ? (
                  <>
                    <XCircle className="w-4 h-4 text-rose-400" />
                    <span>{t.matchFail}</span>
                  </>
                ) : (
                  <span>Hash Calculated:</span>
                )}
              </div>

              {result.calculated && (
                <div className="font-mono text-[11px] break-all bg-neutral-950/70 p-2 rounded border border-neutral-800/80">
                  {result.calculated}
                </div>
              )}

              {result.error && <p className="text-rose-400">{result.error}</p>}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
