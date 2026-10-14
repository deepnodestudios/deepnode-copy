export type FileItemStatus =
  | "pending"
  | "scanning"
  | "copying"
  | "verifying"
  | "completed"
  | "skipped"
  | "failed"
  | "canceled";

export interface FileItem {
  id: number;
  src_path: string;
  dst_path: string;
  relative_path: string;
  size: number;
  bytes_transferred: number;
  status: FileItemStatus;
  src_hash?: string;
  dst_hash?: string;
  error_message?: string;
  retry_attempts: number;
}

export interface TransferStats {
  total_files: number;
  completed_files: number;
  skipped_files: number;
  failed_files: number;
  total_bytes: number;
  transferred_bytes: number;
  current_speed_bps: number;
  average_speed_bps: number;
  eta_seconds: number;
  overall_percentage: number;
  current_file_name: string;
  current_file_transferred: number;
  current_file_size: number;
}

export type EngineEvent =
  | { type: "TransferStarted"; payload: { total_files: number; total_bytes: number; items: FileItem[] } }
  | { type: "ProgressUpdated"; payload: TransferStats }
  | { type: "FileStarted"; payload: FileItem }
  | { type: "FileProgress"; payload: { id: number; transferred: number; total: number } }
  | { type: "FileCompleted"; payload: FileItem }
  | { type: "FileSkipped"; payload: { item: FileItem; reason: string } }
  | { type: "FileFailed"; payload: { item: FileItem; error: string } }
  | { type: "VerificationStarted"; payload: FileItem }
  | { type: "VerificationCompleted"; payload: { item: FileItem; matched: boolean } }
  | { type: "TransferPaused" }
  | { type: "TransferResumed" }
  | { type: "TransferCompleted"; payload: TransferStats }
  | { type: "TransferCanceled" }
  | { type: "TransferFailed"; payload: string }
  | { type: "ConflictDetected"; payload: ConflictInfo };

export interface ConflictInfo {
  file_id: number;
  filename: string;
  src_path: string;
  dst_path: string;
  src_size: number;
  dst_size: number;
  src_modified: number;
  dst_modified: number;
}

export interface ScanPreviewResult {
  total_files: number;
  total_bytes: number;
  items: FileItem[];
}

export interface LaunchArgs {
  sources: string[];
  destination?: string;
  mode: string;
  is_popup: boolean;
  auto_start: boolean;
  conflict_policy?: string;
  on_finish?: string;
}

export type TransferStatus = "idle" | "transferring" | "paused" | "completed" | "canceled" | "failed";

export interface StartTransferResult {
  total_files: number;
  total_bytes: number;
  items: FileItem[];
}

export interface CurrentTransferState {
  status: string;
  stats: TransferStats;
  files: FileItem[];
  launch_args: LaunchArgs;
  active_conflict?: ConflictInfo | null;
}

