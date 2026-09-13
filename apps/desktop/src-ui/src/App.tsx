import { createPortal } from "react-dom";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AnimatePresence, motion, useMotionValue, useTransform } from "motion/react";
import { open as openDialog, save as saveDialog, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { writeText as writeClipboardText } from "@tauri-apps/plugin-clipboard-manager";
import {
  Activity,
  Archive,
  Bell,
  CheckCircle2,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Circle,
  Clapperboard,
  Coffee,
  Contrast,
  Copy,
  Database,
  Eye,
  FileText,
  FileWarning,
  Flag,
  FolderOpen,
  Footprints,
  Gauge,
  HardDrive,
  HelpCircle,
  Import,
  KeyRound,
  Layers,
  Library,
  Link2,
  ListFilter,
  Mic,
  MicOff,
  Music,
  Music2,
  Palette,
  Pause,
  Pencil,
  Piano,
  Play,
  Plus,
  RefreshCw,
  Repeat,
  Save,
  Search,
  Settings,
  ShieldCheck,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Sparkles,
  Star,
  Trash2,
  Volume2,
  Waves,
  Wind,
  Workflow,
  X,
  Zap
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode
} from "react";

type LibraryRecord = {
  id: string;
  name: string;
  media_root: string;
  import_root: string | null;
  /** Named subfolders under media_root a dropped file can be filed into
   * (e.g. ["Soundtrack", "SFX"]). Empty means one destination — no
   * "which folder" prompt needed on drop. */
  import_subfolders: string[];
};

/** A `.darkwave` library file opened before, most recent first — see
 * `preferences::RecentLibraryFileEntry` on the Rust side. */
type RecentLibraryFileEntry = {
  path: string;
  name: string;
  last_opened_at_ms: number;
};

/** A folder registered against a library beyond its implicit media_root —
 * see `storage::FolderRecord`. */
type FolderRecord = {
  id: string;
  library_id: string;
  path: string;
  role: string | null;
  kind: string;
  created_at: string;
};

/** Role choices offered when adding an advanced (non-primary) folder during
 * New Setup — mirrors the media_type-ish strings import classification
 * already understands, plus "documents" for a licence/PDF-only folder.
 * Labels intentionally match mediaTypeOptions' own labels for the same
 * values (defined further below, alongside the browser's classification
 * buttons) — see that array's own comment on why the two must read as the
 * same taxonomy, not a second, differently-worded one; "music" is labeled
 * "Soundtrack" there, not "Music". */
const FOLDER_ROLE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: "", label: "No specific type" },
  { value: "music", label: "Soundtrack" },
  { value: "sound_effect", label: "Sound Effect" },
  { value: "voiceover", label: "Voiceover" },
  { value: "foley", label: "Foley" },
  { value: "ambience", label: "Ambience" },
  { value: "documents", label: "Licence / Documents" }
];

/** Role choices for a project's categorized export folders. Unlike
 * FOLDER_ROLE_OPTIONS (import-side watched folders), "documents" and "no
 * specific type" don't belong here: an asset's media_type is never
 * "documents" (that's only a folder-kind for PDF/licence inventory, never
 * assigned to an actual asset), and an export folder with no role could
 * never match anything on export — every entry needs a real media_type an
 * asset can actually carry, since export_asset_to_project routes by exact
 * match against it. */
const PROJECT_EXPORT_FOLDER_ROLE_OPTIONS = FOLDER_ROLE_OPTIONS.filter(
  (option) => option.value && option.value !== "documents"
);

type AssetPath = { Managed: string } | { Referenced: string };

type AssetRecord = {
  id: string;
  library_id: string;
  original_filename: string;
  display_name: string;
  path: AssetPath;
  storage_mode: "Managed" | "Referenced" | "Hybrid";
  content_hash: string | null;
  media_type: string;
  file_size: number;
  availability_state: "Unknown" | "Local" | "Cached" | "Missing";
  review_state: "Unreviewed" | "Reviewed";
  favorite: boolean;
  embedded_title: string | null;
  embedded_genre: string | null;
  embedded_comment: string | null;
  duration_ms: number | null;
  sample_rate: number | null;
  bit_depth: number | null;
  channels: number | null;
  loudness_lufs: number | null;
  peak_db: number | null;
  bpm: number | null;
  bpm_confidence: number | null;
  /** Best-effort monophonic pitch note (e.g. "A4") — not a musical key. */
  musical_key: string | null;
  key_confidence: number | null;
  /** Fraction of the clip classified as speech (Silero VAD). */
  vocal_ratio: number | null;
  /** Krumhansl-Schmuckler musical key, e.g. "C major" / "A minor". */
  detected_key: string | null;
  key_strength: number | null;
  /** Shared by every member of a detected stem group (the full mix plus
   * its separated-instrument siblings). `null` if not part of one. */
  stem_group_id: string | null;
  /** This member's part within its stem group, e.g. "Full Mix", "Drums". */
  stem_label: string | null;
  /** The one member per stem_group_id shown as the browsable row — the
   * rest are only reachable through its Stems panel. */
  stem_is_primary: boolean;
};

type ImportFailure = {
  filename: string;
  reason: string;
};

type ImportFolderResult = {
  imported: AssetRecord[];
  failed: ImportFailure[];
};

type DroppedImportResult = ImportFolderResult & {
  stem_groups_detected: number;
};

type DeleteLibraryResult = {
  cache_files_removed: number;
  trash_items_cleared: number;
};

type TagRecord = {
  id: string;
  name: string;
  normalized_name: string;
  facet: string | null;
  is_system: boolean;
};

type ReconnectValidationReport = {
  library_id: string;
  manifest_revision: number;
  checked_paths: number;
  missing_paths: string[];
};

type VisibleFilter = {
  field: string;
  operator: string;
  value: string;
};

type SelectionMode = "Replace" | "Toggle" | "Range";

type BrowserState = {
  visible_asset_ids: string[];
  focused_index: number;
  anchor_index: number;
  selected_indices: number[];
};

type CollectionRecord = {
  id: string;
  library_id: string;
  name: string;
  collection_type: "Manual" | "Smart" | "Project";
  query_definition: string | null;
  export_path: string | null;
  sfx_export_path: string | null;
};

/** A categorized export destination on a project beyond its two built-in
 * slots — see `storage::ProjectExportFolderRecord`. `role` shares the same
 * vocabulary as a library `FolderRecord`'s role. */
type ProjectExportFolderRecord = {
  id: string;
  collection_id: string;
  path: string;
  role: string;
  created_at: string;
};

/** Resolve Live Bridge structure-sync state for one project — see
 * `storage::ResolveSyncStatus`. `approved_at` gates one-click
 * send-to-timeline. */
type ResolveSyncStatus = {
  collection_id: string;
  status: "not_synced" | "syncing" | "synced" | "failed";
  error: string | null;
  approved_at: string | null;
  last_synced_at: string | null;
};

type AssetProjectMembership = {
  asset_id: string;
  project_id: string;
  project_name: string;
  export_path: string | null;
  sfx_export_path: string | null;
  exported: boolean;
};

// A project's "DR button" quick-export is enabled once either folder is
// configured — music routes to export_path (the sound folder), everything
// else routes to sfx_export_path (the sound effects folder); which one a
// given send actually needs is resolved per-asset on the backend.
function projectHasExportFolder(project: { export_path: string | null; sfx_export_path: string | null }): boolean {
  return Boolean(project.export_path || project.sfx_export_path);
}

function projectExportFolderSummary(project: {
  name: string;
  export_path: string | null;
  sfx_export_path: string | null;
}): string {
  if (project.export_path && project.sfx_export_path) {
    return `Send selected to ${project.name}'s sound folder (music) or sound effects folder (everything else)`;
  }
  if (project.export_path) {
    return `Send selected to ${project.name}'s sound folder (${project.export_path})`;
  }
  if (project.sfx_export_path) {
    return `Send selected to ${project.name}'s sound effects folder (${project.sfx_export_path})`;
  }
  return `${project.name} has no sound or sound effects folder configured yet`;
}

type PaletteCommandId =
  | "Import"
  | "ApplyTag"
  | "AddToCollection"
  | "Export"
  | "Reveal"
  | "Convert"
  | "Rescan"
  | "OpenSettings"
  | "RunMaintenance";

type PaletteCommand = {
  id: PaletteCommandId;
  title: string;
  category: string;
  keywords: string[];
};

type SourceRecordDraft = {
  asset_id: string;
  provider: string | null;
  source_url: string | null;
  license_type: string | null;
  license_status: string | null;
  attribution: string | null;
  restrictions: string | null;
  receipt_path: string | null;
  license_document_path: string | null;
  license_valid_from: string | null;
  license_valid_until: string | null;
  license_expiry_source: string | null;
};

type LicenseDateCandidate = { date: string; keyword: string; context: string };

type LicenseDocumentAttachOutcome = {
  source: SourceRecordDraft;
  candidates: LicenseDateCandidate[];
};

function emptySourceDraft(assetId: string): SourceRecordDraft {
  return {
    asset_id: assetId,
    provider: null,
    source_url: null,
    license_type: null,
    license_status: null,
    attribution: null,
    restrictions: null,
    receipt_path: null,
    license_document_path: null,
    license_valid_from: null,
    license_valid_until: null,
    license_expiry_source: null
  };
}

type LicenseValidityTone = "none" | "valid" | "soon" | "expired";

// String comparison is intentional and correct for ISO YYYY-MM-DD dates
// (lexicographic order matches chronological order) — mirrors the same
// comparison the backend's assets_with_expired_license query does, so the
// badge here and the Maintenance "License expired" finding never disagree.
function licenseValidityStatus(validUntil: string | null): { label: string; tone: LicenseValidityTone } {
  if (!validUntil) return { label: "No expiry set", tone: "none" };

  const today = new Date().toISOString().slice(0, 10);
  if (validUntil < today) return { label: `Expired since ${validUntil}`, tone: "expired" };

  const daysLeft = Math.round(
    (Date.parse(`${validUntil}T00:00:00Z`) - Date.parse(`${today}T00:00:00Z`)) / 86_400_000
  );
  if (daysLeft <= 30) {
    return { label: `Expires in ${daysLeft} day${daysLeft === 1 ? "" : "s"} (${validUntil})`, tone: "soon" };
  }
  return { label: `Valid until ${validUntil}`, tone: "valid" };
}

type MaintenanceReport = {
  total_findings: number;
  severity: "Ok" | "Warning";
  counts_by_kind: Record<string, number>;
  findings: {
    kind: "MissingMedia" | "LicenseReviewRequired" | "StaleWaveformCache" | "DuplicateContent";
    asset_ids: string[];
    detail: string;
    recommended_action: "Relink" | "Review" | "Regenerate";
  }[];
};

type ShortcutBinding = { command: string; accelerator: string };

type AppPreferences = {
  browser_density: "Compact" | "Comfortable" | "Expanded";
  preview_cache_limit_mb: number;
  output_device: "SystemDefault" | { DeviceId: string };
  shortcuts: { bindings: ShortcutBinding[] };
  reduced_motion: boolean;
  reduced_transparency: boolean;
  theme: "Dark" | "Light" | "System";
  prevent_sleep_during_analysis: boolean;
  recent_library_files: RecentLibraryFileEntry[];
  last_active_library_file_path: string | null;
};

type SoundCategory = "music" | "voice" | "instrumental" | "sound_effect";

type ActiveFilter =
  | "all"
  | "favorites"
  | "unreviewed"
  | "missing"
  | "needs_review"
  | "music"
  | "sound_effect"
  | "ambience"
  | "voiceover"
  | "foley"
  | "has_vocals"
  | "instrumental"
  | "has_tempo"
  | "has_key"
  | "has_pitch"
  | { favoritesCategory: SoundCategory }
  | { unreviewedCategory: SoundCategory }
  | { project: string; smart?: boolean }
  | { tag: string }
  // Instrument Detection. `page: true` shows the pill grid (with
  // `instruments` as the running multi-selection); `page: false` shows the
  // track list filtered to any of `instruments` (OR).
  | { instrumentPage: boolean; instruments: string[] };

function matchesSoundCategory(asset: AssetRecord, category: SoundCategory): boolean {
  switch (category) {
    case "music":
      return asset.media_type === "music";
    case "sound_effect":
      return asset.media_type === "sound_effect";
    case "voice":
      return (asset.vocal_ratio ?? 0) >= VOCAL_RATIO_THRESHOLD;
    case "instrumental":
      return asset.vocal_ratio != null && asset.vocal_ratio < VOCAL_RATIO_THRESHOLD;
    default:
      return true;
  }
}

/** Every audio extension the browser's Format filter recognizes — broader
 * than what the analysis pipeline can actually decode (AAC/M4A are
 * importable and filterable here but not decoder-supported, see
 * docs/adr/0028-defer-aac-decode-pending-patent-question.md). A filter
 * that only covered WAV/MP3 would silently miss whatever fraction of the
 * library came in as FLAC, AAC, M4A, OGG, or AIFF. */
const AUDIO_FORMATS = ["wav", "mp3", "flac", "aac", "m4a", "ogg", "aiff"] as const;
type AudioFormat = (typeof AUDIO_FORMATS)[number];

function detectAudioFormat(asset: AssetRecord): AudioFormat | null {
  const name = asset.original_filename.toLowerCase();
  const dot = name.lastIndexOf(".");
  if (dot === -1) return null;
  const ext = name.slice(dot + 1);
  if (ext === "aif") return "aiff";
  return (AUDIO_FORMATS as readonly string[]).includes(ext) ? (ext as AudioFormat) : null;
}

/** Duration in seconds (converted to ms at the API boundary) since that's
 * what a person actually types; BPM stays as-is. */
type RangeFilters = {
  durationMinSec?: number;
  durationMaxSec?: number;
  bpmMin?: number;
  bpmMax?: number;
};

function hasActiveRangeFilters(filters: RangeFilters): boolean {
  return (
    filters.durationMinSec != null ||
    filters.durationMaxSec != null ||
    filters.bpmMin != null ||
    filters.bpmMax != null
  );
}

type TrashItem = {
  asset_id: string;
  original_path: string;
  trashed_at_ms: number;
  reason: string;
  state: "InTrash" | "Restored" | "Purged";
  file_deleted: boolean;
};

type OfflineControlState = {
  media_root: string;
  catalog_only: boolean;
  validation_paused: boolean;
  reconnect_requested: boolean;
};

type OfflineControlCommand =
  | "UseCatalogOnly"
  | "RetryReconnect"
  | "PauseValidation"
  | "ResumeValidation"
  | { RelinkMediaRoot: { media_root: string } };

type BackupPackage = {
  library_id: string;
  media_root: string;
  backup_file_path: string;
  created_at_ms: number;
};

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const whole = Math.floor(seconds);
  const minutes = Math.floor(whole / 60);
  const remaining = whole % 60;
  return `${minutes}:${remaining.toString().padStart(2, "0")}`;
}

// "not_set" (no media root yet — a library created without a folder, before
// its first import) is distinct from "offline" (a root that was reachable
// before and isn't right now) at the API boundary; this is just the label.
function formatMediaRootStatus(status: string | undefined): string {
  switch (status) {
    case "not_set":
      return "Not set yet";
    case "online":
      return "Online";
    case "offline":
      return "Offline";
    default:
      return "Unknown";
  }
}

// Kind-specific copy for a finished batch — what actually happened, in
// plain language, rather than a generic "N processed" that doesn't say
// what the work even was or what to do about failures.
function describeJobCompletion(summary: JobCompletionSummary): { headline: string; detail: string | null } {
  const noun = summary.kind === "metadata_extraction" ? "file" : "sound";
  const verb =
    summary.kind === "audio_analysis"
      ? "analyzing"
      : summary.kind === "waveform_generation"
        ? "drawing waveforms for"
        : summary.kind === "instrument_detection"
          ? "detecting instruments in"
          : "reading";
  const failedVerb =
    summary.kind === "audio_analysis"
      ? "analyze"
      : summary.kind === "waveform_generation"
        ? "draw waveforms for"
        : summary.kind === "instrument_detection"
          ? "detect instruments in"
          : "read";
  const headline =
    summary.completed > 0
      ? `Finished ${verb} ${summary.completed} ${noun}${summary.completed === 1 ? "" : "s"}`
      : `Couldn't ${failedVerb} ${summary.failed} ${noun}${summary.failed === 1 ? "" : "s"}`;
  if (summary.failed === 0) {
    return {
      headline,
      detail:
        summary.kind === "audio_analysis"
          ? "Tempo, key, vocal detection, and an automatic Soundtrack/Sound Effect/Ambience classification are ready — filter for them anytime under Sonic Radar."
          : null
    };
  }
  const isAre = summary.failed === 1 ? "was" : "were";
  const what =
    summary.failedExtensions.length === 1
      ? `${summary.failed} ${summary.failedExtensions[0]} file${summary.failed === 1 ? "" : "s"} ${isAre}`
      : summary.failedExtensions.length > 1
        ? `${summary.failed} file${summary.failed === 1 ? "" : "s"} (${summary.failedExtensions.join(", ")}) ${isAre}`
        : `${summary.failed} ${isAre}`;
  return {
    headline,
    detail: `${what} skipped, usually an unusual file encoding. They'll retry automatically for a while, or you can retry them now.`
  };
}

const activeBarTrailLength = 10;
const WAVEFORM_STRIP_BUCKETS = 200;

// Session cache of transport-strip peaks keyed by asset id. The backend
// (get_waveform) is the durable cache — this just avoids re-invoking it,
// or re-running the client fallback, while browsing the same list. Bounded
// so a long session can't grow it without limit.
const waveformStripCache = new Map<string, number[]>();
function rememberWaveformStrip(assetId: string, peaks: number[]): void {
  waveformStripCache.delete(assetId);
  waveformStripCache.set(assetId, peaks);
  if (waveformStripCache.size > 512) {
    const oldest = waveformStripCache.keys().next().value;
    if (oldest !== undefined) waveformStripCache.delete(oldest);
  }
}

// Fallback used only when the backend has no cached waveform yet (a freshly
// imported asset whose WaveformGeneration job hasn't run, or a library
// pre-dating waveform caching). Decodes the whole file once in the WebView;
// the caller persists the result via store_waveform_peaks so this never
// runs twice for the same asset.
async function computePeaks(
  path: string,
  bucketCount = WAVEFORM_STRIP_BUCKETS
): Promise<{ peaks: number[]; sampleRate: number } | null> {
  try {
    const response = await fetch(convertFileSrc(path));
    const arrayBuffer = await response.arrayBuffer();
    const AudioContextCtor = window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    const audioContext = new AudioContextCtor();
    const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);
    const channelData = audioBuffer.getChannelData(0);
    const samplesPerBucket = Math.max(1, Math.floor(channelData.length / bucketCount));
    const peaks: number[] = [];
    for (let i = 0; i < bucketCount; i++) {
      let max = 0;
      const start = i * samplesPerBucket;
      const end = Math.min(start + samplesPerBucket, channelData.length);
      for (let j = start; j < end; j++) {
        const value = Math.abs(channelData[j]);
        if (value > max) max = value;
      }
      peaks.push(max);
    }
    const sampleRate = audioBuffer.sampleRate;
    audioContext.close();
    return { peaks, sampleRate };
  } catch {
    return null;
  }
}

type ReleaseReadinessItem = {
  label: string;
  blocker: string;
  state: "Passed" | "Planned";
};

// Mirrors src-tauri/src/license.rs's LicenseStatus. In the Mac App Store
// build (no direct-dist feature) get_license_status always resolves
// { active: true, ... }, so every field below is effectively unused there —
// this type/state exists once for both build variants rather than branching
// the frontend per-build.
type LicenseStatus = {
  active: boolean;
  key: string | null;
  hwid: string;
  message: string | null;
  is_trial: boolean;
  trial_days_remaining: number | null;
  trial_expired: boolean;
};

type LicenseMode = "loading" | "trial" | "trial_expired" | "inactive" | "full";

function licenseModeFor(status: LicenseStatus | null): LicenseMode {
  if (!status) return "loading";
  if (status.active) return "full";
  if (status.is_trial) return "trial";
  if (status.trial_expired) return "trial_expired";
  return "inactive";
}

const fallbackReleaseItems: ReleaseReadinessItem[] = [
  { label: "macOS audit", blocker: "macos_audit", state: "Passed" },
  { label: "Windows audit", blocker: "windows_audit", state: "Passed" },
  { label: "Accessibility", blocker: "accessibility_audit", state: "Passed" },
  { label: "Performance", blocker: "performance_profile", state: "Passed" },
  { label: "Codec packaging", blocker: "codec_packaging", state: "Planned" },
  { label: "Codec license", blocker: "codec_license_review", state: "Planned" },
  { label: "Updates", blocker: "update_system", state: "Planned" },
  { label: "Signing", blocker: "signing_notarization", state: "Planned" }
];

const smartFilters: { id: ActiveFilter; label: string }[] = [
  { id: "all", label: "All Sounds" },
  { id: "favorites", label: "Favorites" },
  { id: "unreviewed", label: "Unreviewed" },
  { id: "missing", label: "Missing Files" },
  { id: "needs_review", label: "Needs Review" },
  { id: "music", label: "Soundtracks" },
  { id: "voiceover", label: "Voiceover" },
  { id: "sound_effect", label: "Sound Effects" },
  { id: "foley", label: "Foley" },
  { id: "ambience", label: "Ambience" }
];

// Same icon/color per category as rowIconMeta uses for browser rows, so the
// classification buttons in the inspector read as "the same categories,"
// not a second, differently-styled taxonomy. Ordered to mirror the export
// folder numbering (01_MUSIC/02_VO/03_SFX/04_FOLEY) for the four foldered
// categories, with the two unfoldered ones (Ambience, Other) trailing.
const mediaTypeOptions: { value: string; label: string; Icon: typeof Music2; color: string; shortcutDigit: string }[] = [
  { value: "music", label: "Soundtrack", Icon: Music2, color: "#4ade9c", shortcutDigit: "1" },
  { value: "voiceover", label: "Voiceover", Icon: Mic, color: "#f0a6d8", shortcutDigit: "2" },
  { value: "sound_effect", label: "Sound Effect", Icon: Waves, color: "var(--sfx-ink)", shortcutDigit: "3" },
  { value: "foley", label: "Foley", Icon: Footprints, color: "#e0a458", shortcutDigit: "4" },
  { value: "ambience", label: "Ambience", Icon: Wind, color: "#5ec8d8", shortcutDigit: "5" },
  { value: "other", label: "Other", Icon: HelpCircle, color: "var(--text-muted)", shortcutDigit: "0" }
];

// value -> CommandId, for the classify keyboard shortcuts registered in
// crates/preferences (ClassifySoundtrack..ClassifyOther) — keeps the
// keydown switch below from having to hardcode media-type strings twice.
const MEDIA_TYPE_CLASSIFY_COMMAND: Record<string, string> = {
  music: "ClassifySoundtrack",
  voiceover: "ClassifyVoiceover",
  sound_effect: "ClassifySoundEffect",
  foley: "ClassifyFoley",
  ambience: "ClassifyAmbience",
  other: "ClassifyOther"
};

type JobProgress = {
  kind: string;
  label: string;
  pending: number;
  total: number;
  failed: number;
  currentFile?: string;
};
type JobCompletionSummary = {
  kind: string;
  label: string;
  completed: number;
  failed: number;
  failedExtensions: string[];
};

const JOB_KINDS: {
  command:
    | "process_pending_jobs"
    | "process_waveform_jobs"
    | "process_instrument_jobs"
    | "process_audio_analysis_jobs";
  kind: string;
  label: string;
}[] = [
  { command: "process_pending_jobs", kind: "metadata_extraction", label: "Reading metadata" },
  { command: "process_waveform_jobs", kind: "waveform_generation", label: "Building waveforms" },
  { command: "process_instrument_jobs", kind: "instrument_detection", label: "Detecting instruments" },
  { command: "process_audio_analysis_jobs", kind: "audio_analysis", label: "Analyzing audio" }
];

const maintenanceLabels: Record<string, string> = {
  MissingMedia: "Missing media",
  LicenseReviewRequired: "License review",
  LicenseExpired: "License expired",
  StaleWaveformCache: "Waveform cache",
  DuplicateContent: "Duplicates"
};

type PlayerMood = "soundtrack" | "soundtrack-voice" | "voice-over" | "sfx";

// sfx used to be a coral (#ff8a73 -> #f2543a) that read as nearly the same
// hue as the app's own default orange accent (the fallback player color
// when no mood is classified, and every primary button) — the two oranges
// were only distinguishable side-by-side. Moved to amber/gold so all four
// moods are genuinely distinct hues (green / purple / blue / amber), with
// none of them competing with the brand accent.
const playerMoodTheme: Record<PlayerMood, { from: string; to: string; glow: string }> = {
  soundtrack: { from: "#4ade9c", to: "#0ea968", glow: "rgba(14, 169, 104, 0.45)" },
  "soundtrack-voice": { from: "#c4a6fa", to: "#8b5cf6", glow: "rgba(139, 92, 246, 0.45)" },
  "voice-over": { from: "#7c90f5", to: "#4c5fe0", glow: "rgba(76, 95, 224, 0.45)" },
  sfx: { from: "var(--sfx-from)", to: "var(--sfx-to)", glow: "var(--sfx-glow)" }
};

// Mirrors classifyPlayerMood's decision order, for the Settings legend —
// keep in sync if that function's logic changes.
const PLAYER_ACCENT_LEGEND: { mood: PlayerMood | "default"; label: string; description: string }[] = [
  { mood: "soundtrack", label: "Soundtrack", description: "Music with no significant vocals detected" },
  { mood: "soundtrack-voice", label: "Soundtrack (vocal)", description: "Music with vocals present" },
  { mood: "voice-over", label: "Voice-over", description: "Non-music audio with vocals detected" },
  { mood: "sfx", label: "Sound effect", description: "Tagged or classified as a sound effect" },
  { mood: "default", label: "Default", description: "No mood could be classified — falls back to the app's brand orange" }
];

// Derives a playback "mood" from the sound's applied tags (falling back to
// media_type) — there's no dedicated speech/music classifier yet, so this
// reuses the app's existing tagging system as the classification signal.
// Below this fraction of the clip detected as speech, treat it as noise in
// the Silero VAD signal rather than a real vocal presence (a few misfired
// frames on a transient shouldn't flip a whole SFX into "has voice").
const VOCAL_RATIO_THRESHOLD = 0.15;

// How far the pointer has to move past an asset row's pointerdown before
// it counts as a drag rather than a click — small enough to feel
// immediate, large enough that an ordinary click's few pixels of jitter
// never gets misread as one.
const DRAG_ACTIVATE_DISTANCE_PX = 6;

// How many recently-used projects the in-flight drag dock offers as
// one-hop drop targets, and where that short list is persisted so it
// survives a restart instead of resetting every session.
const DRAG_DOCK_PROJECT_COUNT = 4;
const RECENT_PROJECT_IDS_STORAGE_KEY = "darkwave.recentProjectIds";

// Cosmetic only (labels, shortcut glyphs). Every actual keyboard/modifier
// check in this file uses `event.metaKey || event.ctrlKey` so behavior is
// correct on both platforms regardless of what this detects.
const isMacPlatform = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
const modKeyLabel = isMacPlatform ? "⌘" : "Ctrl";

// Matches the min-height + margin-bottom each density actually renders at
// in styles.css (.asset-row / .browser[data-density="..."] .asset-row).
// Virtualization assumes a uniform row height, so this has to stay in
// sync with those rules by hand.
const ROW_HEIGHT_PX_BY_DENSITY: Record<string, number> = {
  Compact: 41,
  Comfortable: 60,
  Expanded: 72
};

const BROWSER_OVERSCAN_ROWS = 6;

type SettingsCategory =
  | "general"
  | "playback"
  | "storage"
  | "appearance"
  | "accessibility"
  | "release"
  | "maintenance";

const SETTINGS_CATEGORIES: { id: SettingsCategory; label: string; icon: typeof Database }[] = [
  { id: "general", label: "General", icon: Database },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "playback", label: "Playback", icon: Volume2 },
  { id: "storage", label: "Storage", icon: HardDrive },
  { id: "accessibility", label: "Accessibility", icon: Contrast },
  { id: "release", label: "Release Readiness", icon: ShieldCheck },
  { id: "maintenance", label: "Maintenance", icon: FileWarning }
];

type VisibleRowRange = {
  start: number;
  endExclusive: number;
  offsetTopPx: number;
  spacerBottomPx: number;
};

// Direct port of crates/viewport::VirtualViewport::visible_range (kept
// client-side, not round-tripped through Tauri, since it has to recompute
// on every scroll frame). Keep the two in sync if the algorithm changes.
function computeVisibleRowRange(
  totalRows: number,
  rowHeightPx: number,
  viewportHeightPx: number,
  scrollTopPx: number,
  overscanRows: number
): VisibleRowRange {
  if (totalRows === 0 || rowHeightPx === 0 || viewportHeightPx === 0) {
    return { start: 0, endExclusive: 0, offsetTopPx: 0, spacerBottomPx: 0 };
  }

  const firstVisibleRow = Math.floor(scrollTopPx / rowHeightPx);
  const visibleRowCount = Math.ceil(viewportHeightPx / rowHeightPx);
  const start = Math.max(0, firstVisibleRow - overscanRows);
  const endExclusive = Math.min(totalRows, firstVisibleRow + visibleRowCount + overscanRows);
  const offsetTopPx = start * rowHeightPx;
  const spacerBottomPx = Math.max(0, totalRows - endExclusive) * rowHeightPx;

  return { start, endExclusive, offsetTopPx, spacerBottomPx };
}

function classifyPlayerMood(asset: AssetRecord | null, tags: TagRecord[], vocalRatio: number | null): PlayerMood | null {
  if (!asset) return null;
  const names = tags.map((tag) => tag.name.toLowerCase());
  const hasMusic = names.some((name) => name.includes("music")) || asset.media_type === "music";
  const hasSfx = names.some((name) => name.includes("sound effect")) || asset.media_type === "sound_effect";
  // Prefer the real Silero VAD measurement over the tag-based guess once
  // the background analysis job has actually run on this asset.
  const hasVoice =
    vocalRatio != null
      ? vocalRatio >= VOCAL_RATIO_THRESHOLD
      : names.some((name) => name.includes("voice") || name.includes("dialogue"));

  if (hasMusic && hasVoice) return "soundtrack-voice";
  if (hasMusic) return "soundtrack";
  if (hasVoice) return "voice-over";
  if (hasSfx) return "sfx";
  return null;
}

// Same color language as playerMoodTheme (so a row's icon and the player's
// accent agree once that row is loaded), extended with a color for
// ambience, which has no player mood of its own. Picking the icon and
// color from real per-asset data (media_type, vocal_ratio) rather than a
// single flat "Music" glyph is what actually lets someone tell a
// soundtrack apart from a sound effect at a glance while scanning a long
// list, per row, without opening it.
function rowIconMeta(asset: AssetRecord): { Icon: typeof Music; color: string } {
  const hasVoice = (asset.vocal_ratio ?? 0) >= VOCAL_RATIO_THRESHOLD;
  if (asset.media_type === "music") {
    return hasVoice ? { Icon: Mic, color: "#c4a6fa" } : { Icon: Music2, color: "#4ade9c" };
  }
  if (asset.media_type === "sound_effect") {
    return { Icon: Waves, color: "var(--sfx-ink)" };
  }
  if (asset.media_type === "voiceover") {
    return { Icon: Mic, color: "#f0a6d8" };
  }
  if (asset.media_type === "foley") {
    return { Icon: Footprints, color: "#e0a458" };
  }
  if (asset.media_type === "ambience") {
    return { Icon: Wind, color: "#5ec8d8" };
  }
  if (hasVoice) {
    return { Icon: Mic, color: "#7c90f5" };
  }
  return { Icon: Music, color: "#8a7d6d" };
}

function CollapsibleSection({
  id,
  title,
  icon,
  accent,
  headerExtra,
  collapsed,
  onToggle,
  children,
  ...rest
}: {
  id: string;
  title: string;
  icon?: ReactNode;
  accent?: boolean;
  headerExtra?: ReactNode;
  collapsed: boolean;
  onToggle: (id: string) => void;
  children: ReactNode;
} & Record<string, unknown>) {
  return (
    <section {...rest}>
      <div className={accent ? "section-header accent" : "section-header"} onClick={() => onToggle(id)}>
        <h2>
          {icon}
          {title}
        </h2>
        <div className="section-header-actions">
          {headerExtra}
          <button
            type="button"
            className="section-toggle"
            aria-label={collapsed ? `Expand ${title}` : `Collapse ${title}`}
            onClick={(event) => {
              event.stopPropagation();
              onToggle(id);
            }}
          >
            {collapsed ? <ChevronRight size={13} /> : <ChevronDown size={13} />}
          </button>
        </div>
      </div>
      {collapsed ? null : <div className="section-body">{children}</div>}
    </section>
  );
}

type InstrumentFacet = { name: string; count: number };

// Full-area Instrument Detection view: every detected instrument in the
// library as a pill button with its track count. Plain click jumps
// straight to that instrument's tracks; shift-click toggles it into a
// running multi-selection (OR) that "Show tracks" then applies.
function InstrumentDetectionPage({
  libraryId,
  selected,
  onPick,
  onToggle,
  onApply
}: {
  libraryId: string | null;
  selected: string[];
  onPick: (name: string) => void;
  onToggle: (name: string) => void;
  onApply: () => void;
}) {
  const [facets, setFacets] = useState<InstrumentFacet[] | null>(null);

  useEffect(() => {
    if (!libraryId) {
      setFacets([]);
      return;
    }
    let live = true;
    const load = () =>
      invoke<InstrumentFacet[]>("instruments_for_library", { libraryId })
        .then((rows) => {
          if (live) setFacets(rows);
        })
        .catch(() => {
          if (live) setFacets([]);
        });
    load();
    // Counts grow as InstrumentDetection jobs finish — refresh on the
    // background tick that also drives job draining.
    const unlisten = listen("background-tick", load);
    return () => {
      live = false;
      unlisten.then((off) => off());
    };
  }, [libraryId]);

  const selectedCount = selected.length;

  return (
    <section className="instrument-page" aria-label="Instrument detection">
      <div className="instrument-page-head">
        <div>
          <h2>Instrument Detection</h2>
          <p>
            {facets === null
              ? "Scanning the library…"
              : facets.length === 0
                ? "No instruments detected yet — they appear here as analysis runs. If nothing ever shows, the detection model isn't installed (see docs)."
                : "Click an instrument to see its tracks. Shift-click to combine several (matches any)."}
          </p>
        </div>
        {selectedCount > 0 ? (
          <button type="button" className="primary-action" onClick={onApply}>
            Show tracks · {selectedCount} instrument{selectedCount === 1 ? "" : "s"}
          </button>
        ) : null}
      </div>
      {facets && facets.length > 0 ? (
        <div className="instrument-pill-grid">
          {facets.map((facet) => {
            const isSelected = selected.includes(facet.name);
            return (
              <button
                key={facet.name}
                type="button"
                className={isSelected ? "instrument-pill selected" : "instrument-pill"}
                aria-pressed={isSelected}
                onClick={(event) => (event.shiftKey ? onToggle(facet.name) : onPick(facet.name))}
              >
                <span className="instrument-pill-name">{facet.name}</span>
                <span className="instrument-pill-count">{facet.count}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </section>
  );
}

export function App() {
  const [releaseItems, setReleaseItems] = useState(fallbackReleaseItems);
  const updateChannelState = releaseItems.find((item) => item.blocker === "update_system")?.state ?? "Planned";

  const [licenseStatus, setLicenseStatus] = useState<LicenseStatus | null>(null);
  const [licenseFormOpen, setLicenseFormOpen] = useState(false);
  const [licenseKeyInput, setLicenseKeyInput] = useState("");
  const [licenseEmailInput, setLicenseEmailInput] = useState("");
  const [licenseFormBusy, setLicenseFormBusy] = useState(false);
  const [licenseFormError, setLicenseFormError] = useState<string | null>(null);
  const licenseMode = licenseModeFor(licenseStatus);

  const handleActivateLicense = useCallback(async () => {
    setLicenseFormBusy(true);
    setLicenseFormError(null);
    try {
      const status = await invoke<LicenseStatus>("activate_license", {
        key: licenseKeyInput,
        email: licenseEmailInput
      });
      setLicenseStatus(status);
      setLicenseFormOpen(false);
      setLicenseKeyInput("");
      setLicenseEmailInput("");
    } catch (error) {
      setLicenseFormError(String(error));
    } finally {
      setLicenseFormBusy(false);
    }
  }, [licenseKeyInput, licenseEmailInput]);

  const handleRecoverLicenseKey = useCallback(async () => {
    setLicenseFormBusy(true);
    setLicenseFormError(null);
    try {
      const result = await invoke<{ message: string }>("recover_license_key", {
        email: licenseEmailInput
      });
      setLicenseFormError(result.message);
    } catch (error) {
      setLicenseFormError(String(error));
    } finally {
      setLicenseFormBusy(false);
    }
  }, [licenseEmailInput]);

  const [librariesLoaded, setLibrariesLoaded] = useState(false);
  const [libraries, setLibraries] = useState<LibraryRecord[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState<string | null>(null);
  // Mirrors activeLibraryId for runJobDrain's async loop to read without
  // re-running the effect/closure on every library swap — see its use below.
  const activeLibraryIdRef = useRef<string | null>(null);
  useEffect(() => {
    activeLibraryIdRef.current = activeLibraryId;
  }, [activeLibraryId]);
  const [assets, setAssets] = useState<AssetRecord[]>([]);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [browserState, setBrowserState] = useState<BrowserState | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [rangeFilters, setRangeFilters] = useState<RangeFilters>({});
  const [smartCollectionModalOpen, setSmartCollectionModalOpen] = useState(false);
  const [smartCollectionName, setSmartCollectionName] = useState("");
  const [queryFilters, setQueryFilters] = useState<VisibleFilter[]>([]);
  const [libraryName, setLibraryName] = useState("");
  // First-run wizard state (see the `librariesLoaded && (libraries.length
  // === 0 || onboardingLibrary)` branch below). Keeping the just-created
  // library here — not just its id — is what keeps the wizard on screen
  // through the later steps: `libraries.length` stops being 0 the moment
  // step 1 creates it, so `onboardingLibrary` is what the branch condition
  // falls back on until the wizard explicitly finishes.
  //
  // Step 0 is the New Setup / Open Library choice (plus a recent-libraries
  // list); step 1 names the new library file and picks its save location;
  // step 2 is the media-root + advanced-folders setup, which finishes the
  // wizard directly on confirm — only reached via New Setup, since Open
  // Library (or a recent-file click) skips straight into the app since
  // that library file is already configured.
  const [onboardingStep, setOnboardingStep] = useState<0 | 1 | 2>(0);
  const [onboardingLibrary, setOnboardingLibrary] = useState<LibraryRecord | null>(null);
  const [onboardingLibraryFilePath, setOnboardingLibraryFilePath] = useState("");
  const [onboardingMediaRoot, setOnboardingMediaRoot] = useState("");
  // Step 2 asks up front which shape the user's files are in, then shows
  // only the matching UI — a single field for "one main folder", or the
  // per-type folder list for "already organized". Showing both at once
  // (the old layout) read as "fill in this AND optionally that", which
  // wasn't the actual choice being offered.
  const [onboardingFolderMode, setOnboardingFolderMode] = useState<"single" | "advanced">("single");
  // Advanced setup: additional folders added on the media-root step, each
  // optionally tagged with the media type it's expected to hold — turned
  // into `folders` rows (kind "watched", or "documents" for the licence/PDF
  // case) alongside the implicit media_root folder once step 2 confirms.
  // The basic single-folder user never touches this — it stays empty and
  // only the media_root folder itself gets registered. A client-side `id`
  // (not a real folder id yet — nothing's been created server-side until
  // step 2 confirms) keeps each row addressable while its path is still
  // empty: pick a type first, then Browse fills in that same row's path,
  // rather than a dialog opening the moment you add a row.
  const [onboardingAdvancedFolders, setOnboardingAdvancedFolders] = useState<
    Array<{ id: string; path: string; role: string }>
  >([]);
  const [onboardingBusy, setOnboardingBusy] = useState(false);
  const [onboardingError, setOnboardingError] = useState<string | null>(null);
  const [onboardingNetworkPathWarning, setOnboardingNetworkPathWarning] = useState<string | null>(null);
  const [recentLibraryFiles, setRecentLibraryFiles] = useState<RecentLibraryFileEntry[]>([]);
  // `null` while unknown (fetched once on launch, alongside list_libraries).
  // A real project file open (New Setup, Open Library, reopening the last
  // one, a double-clicked file) sets this to its path. Staying `null` while
  // `libraries` is non-empty means the app booted straight into the legacy
  // shared catalog.sqlite that predates the library-file model — that's
  // exactly the migration-screen condition below, not the New Setup/Open
  // Library first-run screen (which only applies when `libraries` is also
  // empty).
  const [activeLibraryFilePath, setActiveLibraryFilePath] = useState<string | null | undefined>(undefined);
  // "Remind me later" on the migration screen — a session-only dismissal
  // (not persisted), so the app remains fully usable against the legacy
  // catalog for the rest of this run, exactly as it already was before this
  // feature existed. The prompt reappears next launch since nothing was
  // actually migrated.
  const [migrationDismissed, setMigrationDismissed] = useState(false);
  type MigrationRowStatus = "pending" | "migrating" | "done" | "failed";
  const [migrationStatus, setMigrationStatus] = useState<Record<string, MigrationRowStatus>>({});
  const [migrationErrors, setMigrationErrors] = useState<Record<string, string>>({});
  const [migrationBusy, setMigrationBusy] = useState(false);
  const [importStatus, setImportStatus] = useState<string | null>(null);
  // Files/folders dropped onto the window from outside the app (Finder,
  // Downloads...). Set only when the active library has 2+ configured
  // import_subfolders and no remembered choice yet this session — the
  // modal that reads this asks "which folder", then either imports
  // directly (0 or 1 subfolders) or defers to the remembered choice.
  const [externalDropPrompt, setExternalDropPrompt] = useState<{
    libraryId: string;
    paths: string[];
    subfolders: string[];
  } | null>(null);
  const [externalDropRememberChoice, setExternalDropRememberChoice] = useState(false);
  // Session-only by design (a ref, never persisted) — "remember my
  // choice" means "for the rest of this run", not forever; the prompt
  // comes back on the next launch even if this was checked last time.
  const rememberedDropSubfolderRef = useRef<{ libraryId: string; subfolder: string | null } | null>(null);
  // True for the whole time a file is hovering over the window during an
  // OS-level drag (Tauri's 'enter'/'over'), false again on 'drop' or
  // 'leave' — purely the canvas overlay's visibility, no import logic of
  // its own (that's handleExternalFileDrop, on 'drop').
  const [isExternalDragActive, setIsExternalDragActive] = useState(false);
  // A brief, auto-dismissing confirmation once a drop finishes importing —
  // separate from importStatus (which only ever shows up inside the
  // Background Activity panel a click away). `id` keys the AnimatePresence
  // node so two imports finishing with the same message back-to-back still
  // each get their own enter animation rather than looking like one that
  // never left.
  const [importToast, setImportToast] = useState<{ id: number; message: string } | null>(null);
  const importToastTimeoutRef = useRef<number | null>(null);
  const showImportToast = useCallback((message: string) => {
    if (importToastTimeoutRef.current !== null) window.clearTimeout(importToastTimeoutRef.current);
    setImportToast({ id: Date.now(), message });
    importToastTimeoutRef.current = window.setTimeout(() => setImportToast(null), 3600);
  }, []);
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>("all");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>("general");
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);
  const [filterMenuPosition, setFilterMenuPosition] = useState<{ top: number; left: number } | null>(null);
  const filterButtonRef = useRef<HTMLButtonElement | null>(null);
  // Sonic Radar's re-analyse options — one button, one menu, instead of two
  // unlabeled icons that looked like the same action twice ("Re-analyse
  // everything" vs "Backfill Tempo/Key/Pitch/Vocals only" were previously
  // both bare icon buttons with nothing but a hover tooltip telling them
  // apart).
  const [radarSyncMenuOpen, setRadarSyncMenuOpen] = useState(false);
  const [radarSyncMenuPosition, setRadarSyncMenuPosition] = useState<{ top: number; left: number } | null>(null);
  const radarSyncButtonRef = useRef<HTMLButtonElement | null>(null);
  const [sfxSubcategoriesOpen, setSfxSubcategoriesOpen] = useState(false);
  const [favoritesCategoriesOpen, setFavoritesCategoriesOpen] = useState(false);
  const [unreviewedCategoriesOpen, setUnreviewedCategoriesOpen] = useState(false);
  // "projects" is deliberately absent here — the inspector's Projects
  // section (id="projects") is a drag-and-drop target, so it needs to be
  // visible by default or a fresh session hides it and dropping a track
  // there looks broken until the user notices and expands it manually.
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(
    () => new Set(["embedded", "detected", "source", "maintenance", "nas", "backup"])
  );
  const [refreshStatus, setRefreshStatus] = useState<string | null>(null);
  const [newProjectModalOpen, setNewProjectModalOpen] = useState(false);
  // Drag-and-drop a track onto a project name (sidebar or inspector) to add
  // it there — tracks which project row is currently a valid drop target,
  // for the hover highlight. dragPreview (+ the motion values tracking the
  // cursor) drives the floating, morphing drag-preview card; null means no
  // drag is in progress. See the pointermove/pointerup effect further down.
  const [dragOverProjectId, setDragOverProjectId] = useState<string | null>(null);
  const [dragPreview, setDragPreview] = useState<{ label: string; count: number } | null>(null);
  // The Projects grid's per-card count badge, clicked open — fetched fresh
  // via assets_in_collection each time rather than kept in sync locally,
  // since it's opened rarely enough that an extra round trip is cheaper
  // than reasoning about staleness.
  const [projectTracksPanel, setProjectTracksPanel] = useState<{
    project: CollectionRecord;
    position: { top: number; left: number };
    assets: AssetRecord[] | null;
  } | null>(null);
  // The last few projects a track was actually dropped/added into, most
  // recent first — persisted so the drag dock below has something useful
  // to show even on a fresh launch's first drag. Purely a convenience
  // ranking; add_to_collection is the source of truth, this never is.
  const [recentProjectIds, setRecentProjectIds] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem(RECENT_PROJECT_IDS_STORAGE_KEY);
      const parsed = raw ? JSON.parse(raw) : [];
      return Array.isArray(parsed) ? parsed.filter((entry): entry is string => typeof entry === "string") : [];
    } catch {
      return [];
    }
  });
  const dragPreviewX = useMotionValue(0);
  const dragPreviewY = useMotionValue(0);
  // Offset a little past the cursor rather than centered under it, so the
  // card doesn't itself hide the drop target it's hovering over. Derived
  // values (not a static CSS transform) so they keep composing with
  // whileDrag/animate's own transform instead of fighting it.
  const dragPreviewLeft = useTransform(dragPreviewX, (value) => value + 14);
  const dragPreviewTop = useTransform(dragPreviewY, (value) => value + 18);
  const dragStartRef = useRef<{ x: number; y: number; asset: AssetRecord } | null>(null);
  const dragActiveRef = useRef(false);
  // The ids captured for the in-progress drag. Must be a ref, not a plain
  // local variable inside the pointermove/pointerup effect below: that
  // effect depends on bulkAssetIds (among others), which gets a new array
  // reference on essentially any background refresh — a completed analysis
  // batch, a filter change, anything — even when the actual selection is
  // unchanged. Each such change tears down and re-installs the effect's
  // listeners from a fresh closure, and a plain local variable resets to
  // [] on that remount; since it's only ever populated on the
  // inactive->active transition (dragActiveRef, a ref, stays true across
  // the remount), the new closure's copy never gets filled in again for
  // the rest of the gesture. Dropping the track then silently no-ops on
  // pointerup (ids.length === 0) with no error at all. A ref sidesteps
  // this entirely by surviving the remount.
  const draggedIdsRef = useRef<string[]>([]);
  const suppressNextRowClickRef = useRef(false);
  const [createLibraryModalOpen, setCreateLibraryModalOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const toggleSection = useCallback((id: string) => {
    setCollapsedSections((previous) => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const [tags, setTags] = useState<TagRecord[]>([]);
  const [appliedTags, setAppliedTags] = useState<TagRecord[]>([]);
  const [suggestedTags, setSuggestedTags] = useState<TagRecord[]>([]);
  const [assetInstruments, setAssetInstruments] = useState<{ name: string; confidence: number }[]>([]);
  // Every member of the selected asset's stem group (primary/full-mix
  // first) — the inspector's Stems panel. Empty whenever the selection
  // isn't part of a group at all, not just while a fetch is pending, so
  // the panel can tell "no stems" from "still loading" if it ever needs
  // to (it currently doesn't bother — the section just doesn't render).
  const [stemGroupMembers, setStemGroupMembers] = useState<AssetRecord[]>([]);
  const [newTagName, setNewTagName] = useState("");
  const [newTagFacet, setNewTagFacet] = useState("action");

  const [collections, setCollections] = useState<CollectionRecord[]>([]);
  // Keyed by project id — only ever has entries for projects that have
  // actually attempted a Resolve sync (see get_resolve_sync_statuses_for_library's
  // doc comment); a project absent from this map has simply never synced.
  const [resolveSyncStatuses, setResolveSyncStatuses] = useState<Record<string, ResolveSyncStatus>>({});
  const [libraryFolders, setLibraryFolders] = useState<FolderRecord[]>([]);
  const [projectMemberships, setProjectMemberships] = useState<AssetProjectMembership[]>([]);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectExportPath, setNewProjectExportPath] = useState("");
  const [newProjectSfxExportPath, setNewProjectSfxExportPath] = useState("");
  // Additional categorized export folders added while creating a project —
  // beyond the two built-in export_path/sfx_export_path slots — each
  // becomes a project_export_folders row once the project is created. A
  // client-side `id` (nothing's created server-side until then) keeps each
  // row addressable while its path is still empty, same as onboarding's
  // advanced-folder rows.
  const [newProjectExportFolders, setNewProjectExportFolders] = useState<
    Array<{ id: string; path: string; role: string }>
  >([]);
  const [newImportSubfolderName, setNewImportSubfolderName] = useState("");
  // The sidebar's hover-to-reveal edit icon on a project row opens this —
  // editProjectId doubles as "is the modal open".
  const [editProjectId, setEditProjectId] = useState<string | null>(null);
  const [editProjectName, setEditProjectName] = useState("");
  const [editProjectExportPath, setEditProjectExportPath] = useState("");
  const [editProjectSfxExportPath, setEditProjectSfxExportPath] = useState("");
  // The categorized export folders already saved on the project being
  // edited — fetched fresh each time the edit modal opens. Unlike the
  // create-modal list above, add/remove/role-change here apply immediately
  // (the project already exists), the same "no separate Save step" pattern
  // Settings' Watched Folders panel uses.
  const [editProjectExportFolders, setEditProjectExportFolders] = useState<ProjectExportFolderRecord[]>([]);
  const [lastExportProjectId, setLastExportProjectId] = useState<string | null>(null);
  const [trashModalOpen, setTrashModalOpen] = useState(false);
  const [editorWorkflowOpen, setEditorWorkflowOpen] = useState(false);
  const [editorActionStatus, setEditorActionStatus] = useState<string | null>(null);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [commandPaletteQuery, setCommandPaletteQuery] = useState("");
  const [commandPaletteResults, setCommandPaletteResults] = useState<PaletteCommand[]>([]);
  const [commandPaletteActiveIndex, setCommandPaletteActiveIndex] = useState(0);
  const commandPaletteInputRef = useRef<HTMLInputElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const [undoStack, setUndoStack] = useState<{ id: string; label: string }[]>([]);
  const [redoStack, setRedoStack] = useState<{ id: string; label: string }[]>([]);

  const [sourceDraft, setSourceDraft] = useState<SourceRecordDraft | null>(null);
  const [licenseDateCandidates, setLicenseDateCandidates] = useState<LicenseDateCandidate[]>([]);
  const [licenseDocumentStatus, setLicenseDocumentStatus] = useState<string | null>(null);
  const [maintenanceReport, setMaintenanceReport] = useState<MaintenanceReport | null>(null);
  const [mediaRootStatus, setMediaRootStatus] = useState<{ status: string; reconnectRequired: boolean } | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [formatFilter, setFormatFilter] = useState<AudioFormat | null>(null);
  const [similarStatus, setSimilarStatus] = useState<string | null>(null);
  const [jobProgress, setJobProgress] = useState<JobProgress[]>([]);
  const [jobCompletionSummaries, setJobCompletionSummaries] = useState<Record<string, JobCompletionSummary>>({});
  const [audioAnalysisPaused, setAudioAnalysisPaused] = useState(false);
  const [waveformGenerationPaused, setWaveformGenerationPaused] = useState(false);
  // null = not yet checked; false = no model installed, so skip the
  // instrument-detection drain (its jobs stay pending harmlessly).
  const [instrumentDetectionAvailable, setInstrumentDetectionAvailable] = useState<boolean | null>(null);
  const [resyncing, setResyncing] = useState(false);
  // Sonic Radar inspector panel: which per-asset analysis kind is currently
  // queued/running (drives the row's progress bar/disabled state) and the
  // last status line shown under the row list. Both reset when selection
  // changes. sonicRadarBusyKind only clears once handleAnalyzeAssetKind's
  // own poll of asset_job_state confirms the job reached a terminal state
  // (completed or failed) — never optimistically, and never silently.
  const [sonicRadarBusyKind, setSonicRadarBusyKind] = useState<"audio_analysis" | "instrument_detection" | null>(
    null
  );
  const [sonicRadarStatus, setSonicRadarStatus] = useState<string | null>(null);
  // Bumped on every new Analyze click and every selection change; a poll
  // loop checks its own captured id against this before acting, so an
  // in-flight poll for a superseded click (user switched tracks, or
  // clicked Analyze again) quietly stops touching state instead of racing
  // a newer one.
  const sonicRadarRequestIdRef = useRef(0);
  // Same pattern: refreshAssets can be triggered concurrently (a job drain
  // finishing, a background-tick, a manual filter change) and its
  // invoke() calls aren't guaranteed to resolve in the order they were
  // sent — a slower earlier call landing after a faster later one would
  // otherwise clobber `assets` with stale data, which the "selection no
  // longer in the list" effect below would then act on. Bumped at the
  // start of every refreshAssets call; a resolving request checks its own
  // captured id against this before calling setAssets.
  const refreshAssetsRequestIdRef = useRef(0);
  const [backgroundActivityOpen, setBackgroundActivityOpen] = useState(false);
  const drainingJobKinds = useRef<Set<string>>(new Set());
  const [offlineControl, setOfflineControl] = useState<OfflineControlState | null>(null);
  const [reconnectStatus, setReconnectStatus] = useState<string | null>(null);
  const [trashItems, setTrashItems] = useState<TrashItem[]>([]);
  const [backupStatus, setBackupStatus] = useState<string | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);
  const [libraryAdminStatus, setLibraryAdminStatus] = useState<string | null>(null);

  const [preferences, setPreferences] = useState<AppPreferences | null>(null);
  const [trashRetentionDays, setTrashRetentionDays] = useState(30);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playingAssetId, setPlayingAssetId] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [looping, setLooping] = useState(false);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const peakRequestId = useRef(0);

  const selectedAsset = assets.find((asset) => asset.id === selectedAssetId) ?? null;
  const selectedAssetIds = useMemo(() => {
    if (!browserState) return [];
    return browserState.selected_indices
      .map((index) => browserState.visible_asset_ids[index])
      .filter((id): id is string => Boolean(id));
  }, [browserState]);
  const selectedCount = selectedAssetIds.length;
  const bulkAssetIds = selectedCount > 1 ? selectedAssetIds : selectedAssetId ? [selectedAssetId] : [];
  const activeLibrary = libraries.find((library) => library.id === activeLibraryId) ?? null;
  const importFolderScannedRef = useRef<Set<string>>(new Set());

  const membershipsByAsset = useMemo(() => {
    const map = new Map<string, AssetProjectMembership[]>();
    for (const membership of projectMemberships) {
      const existing = map.get(membership.asset_id);
      if (existing) existing.push(membership);
      else map.set(membership.asset_id, [membership]);
    }
    return map;
  }, [projectMemberships]);

  // How many tracks have already been sent to each project — the Projects
  // grid's count badge. Derived from the same library-wide membership list
  // already kept fresh for the per-row "in a project" badges, so this is
  // free: no extra fetch just to show a number.
  const trackCountByProject = useMemo(() => {
    const counts = new Map<string, number>();
    for (const membership of projectMemberships) {
      counts.set(membership.project_id, (counts.get(membership.project_id) ?? 0) + 1);
    }
    return counts;
  }, [projectMemberships]);

  const visibleAssets = useMemo(() => {
    let base: AssetRecord[];
    if (activeFilter === "favorites") {
      base = assets.filter((asset) => asset.favorite);
    } else if (activeFilter === "unreviewed") {
      base = assets.filter((asset) => asset.review_state === "Unreviewed");
    } else if (activeFilter === "missing") {
      base = assets.filter((asset) => asset.availability_state === "Missing");
    } else if (
      activeFilter === "needs_review" ||
      activeFilter === "music" ||
      activeFilter === "sound_effect" ||
      activeFilter === "ambience" ||
      activeFilter === "voiceover" ||
      activeFilter === "foley"
    ) {
      base = assets.filter((asset) => asset.media_type === activeFilter);
    } else if (activeFilter === "has_vocals") {
      base = assets.filter((asset) => (asset.vocal_ratio ?? 0) >= VOCAL_RATIO_THRESHOLD);
    } else if (activeFilter === "instrumental") {
      base = assets.filter((asset) => asset.vocal_ratio != null && asset.vocal_ratio < VOCAL_RATIO_THRESHOLD);
    } else if (activeFilter === "has_tempo") {
      base = assets.filter((asset) => asset.bpm != null);
    } else if (activeFilter === "has_key") {
      base = assets.filter((asset) => asset.detected_key != null);
    } else if (activeFilter === "has_pitch") {
      base = assets.filter((asset) => asset.musical_key != null);
    } else if (typeof activeFilter === "object" && "favoritesCategory" in activeFilter) {
      base = assets.filter((asset) => asset.favorite && matchesSoundCategory(asset, activeFilter.favoritesCategory));
    } else if (typeof activeFilter === "object" && "unreviewedCategory" in activeFilter) {
      base = assets.filter(
        (asset) => asset.review_state === "Unreviewed" && matchesSoundCategory(asset, activeFilter.unreviewedCategory)
      );
    } else {
      base = assets;
    }
    if (formatFilter) {
      base = base.filter((asset) => detectAudioFormat(asset) === formatFilter);
    }
    // A stem group's non-primary members (the individual instrument
    // parts) never show up as their own row in the main browser — only
    // the primary (the full mix, or a stand-in when no full mix exists)
    // does, with the rest reachable through its own Stems panel. Applied
    // last, after every other filter, so this holds no matter how the
    // list is currently filtered.
    return base.filter((asset) => !asset.stem_group_id || asset.stem_is_primary);
  }, [assets, activeFilter, formatFilter]);

  const instrumentFilter =
    typeof activeFilter === "object" && "instrumentPage" in activeFilter ? activeFilter : null;

  const pickInstrument = useCallback((name: string) => {
    setActiveFilter({ instrumentPage: false, instruments: [name] });
  }, []);

  const toggleInstrument = useCallback((name: string) => {
    setActiveFilter((current) => {
      const base =
        typeof current === "object" && "instrumentPage" in current ? current.instruments : [];
      const next = base.includes(name) ? base.filter((entry) => entry !== name) : [...base, name];
      // Removing the last one in results mode drops back to the pill page.
      const page =
        (typeof current === "object" && "instrumentPage" in current ? current.instrumentPage : true) ||
        next.length === 0;
      return { instrumentPage: page, instruments: next };
    });
  }, []);

  const browserScrollRef = useRef<HTMLElement | null>(null);
  const [browserScrollTop, setBrowserScrollTop] = useState(0);
  const [browserViewportHeight, setBrowserViewportHeight] = useState(600);

  useEffect(() => {
    const node = browserScrollRef.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) setBrowserViewportHeight(entry.contentRect.height);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const browserRowHeightPx = ROW_HEIGHT_PX_BY_DENSITY[preferences?.browser_density ?? "Comfortable"] ?? 60;

  const browserVisibleRange = useMemo(
    () =>
      computeVisibleRowRange(
        visibleAssets.length,
        browserRowHeightPx,
        browserViewportHeight,
        browserScrollTop,
        BROWSER_OVERSCAN_ROWS
      ),
    [visibleAssets.length, browserRowHeightPx, browserViewportHeight, browserScrollTop]
  );

  useEffect(() => {
    let cancelled = false;
    const ids = visibleAssets.map((asset) => asset.id);
    // Captured from the *current* (about-to-be-replaced) browserState via
    // closure — this is the real multi-selection (Toggle/Range can select
    // many rows independently of `selectedAssetId`, which is only ever
    // "whichever row was last clicked"). Previously this effect restored
    // just `selectedAssetId` after a rebuild, silently collapsing a real
    // multi-selection down to one row (or zero) on every `visibleAssets`
    // change — which is any refreshAssets call: background analysis
    // progress, a filter change, anything that gives `assets` a new array
    // reference. That's what made "select several tracks, then Add to
    // Project" only add a fraction of them, or none, whenever a refresh
    // landed between selecting and acting on the selection.
    const previouslySelectedIds = selectedAssetIds;

    (async () => {
      let next = await invoke<BrowserState>("create_browser_state", { visibleAssetIds: ids }).catch(() => null);
      if (!next) return;

      const survivingIndices = previouslySelectedIds
        .map((id) => ids.indexOf(id))
        .filter((index) => index >= 0);

      if (survivingIndices.length > 0) {
        next = await invoke<BrowserState>("apply_browser_command", {
          browserState: next,
          command: { SelectIndices: { indices: survivingIndices } }
        }).catch(() => next);
      } else if (selectedAssetId) {
        const index = ids.indexOf(selectedAssetId);
        if (index >= 0) {
          next = await invoke<BrowserState>("apply_browser_command", {
            browserState: next,
            command: { FocusRow: { index } }
          }).catch(() => next);
          next = await invoke<BrowserState>("apply_browser_command", {
            browserState: next,
            command: { SelectFocused: { mode: "Replace" } }
          }).catch(() => next);
        }
      }

      if (!cancelled) setBrowserState(next);
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visibleAssets]);

  const refreshAssets = useCallback(
    (libraryId: string, query: string, filter: ActiveFilter) => {
      const requestId = ++refreshAssetsRequestIdRef.current;
      const isCurrent = () => refreshAssetsRequestIdRef.current === requestId;
      const applyResult = (result: AssetRecord[]) => {
        if (isCurrent()) setAssets(result);
      };
      const applyEmpty = () => {
        if (isCurrent()) setAssets([]);
      };

      if (typeof filter === "object" && "project" in filter) {
        const command = filter.smart ? "assets_in_smart_collection" : "assets_in_collection";
        invoke<AssetRecord[]>(command, { collectionId: filter.project }).then(applyResult).catch(applyEmpty);
        return;
      }
      if (typeof filter === "object" && "tag" in filter) {
        invoke<AssetRecord[]>("assets_for_tag", { libraryId, tagId: filter.tag })
          .then(applyResult)
          .catch(applyEmpty);
        return;
      }
      if (typeof filter === "object" && "instrumentPage" in filter) {
        // On the pill page the list isn't shown; in results mode fetch the
        // OR-match set. Empty selection => nothing.
        if (filter.instrumentPage || filter.instruments.length === 0) {
          applyEmpty();
          return;
        }
        invoke<AssetRecord[]>("assets_by_instruments", { libraryId, instruments: filter.instruments })
          .then(applyResult)
          .catch(applyEmpty);
        return;
      }

      const request = hasActiveRangeFilters(rangeFilters)
        ? invoke<AssetRecord[]>("search_assets_advanced", {
            libraryId,
            filters: {
              text: query,
              duration_min_ms: rangeFilters.durationMinSec != null ? rangeFilters.durationMinSec * 1000 : null,
              duration_max_ms: rangeFilters.durationMaxSec != null ? rangeFilters.durationMaxSec * 1000 : null,
              bpm_min: rangeFilters.bpmMin ?? null,
              bpm_max: rangeFilters.bpmMax ?? null
            }
          })
        : query.trim().length > 0
          ? invoke<AssetRecord[]>("search_assets", { libraryId, query })
          : invoke<AssetRecord[]>("list_assets", { libraryId });

      request.then(applyResult).catch(applyEmpty);
    },
    [rangeFilters]
  );

  // Repeatedly drives process_pending_jobs/process_audio_analysis_jobs to
  // completion (each call only processes a capped batch) and tracks live
  // progress per job kind for the status bars. `only` restricts which kinds
  // to drain — used by the cache-warm effect, which should only retry
  // audio analysis, not metadata extraction.
  const runJobDrain = useCallback(
    (libraryId: string, only?: string[]) => {
      const configs = (only ? JOB_KINDS.filter((config) => only.includes(config.kind)) : JOB_KINDS)
        // No instrument model installed: its jobs stay pending harmlessly,
        // so don't spin process_instrument_jobs (and flash a progress bar)
        // on every drain trigger.
        .filter((config) => config.kind !== "instrument_detection" || instrumentDetectionAvailable !== false);

      type JobStatus = { kind: string; pending: number; failed: number; completed: number };

      invoke<JobStatus[]>("job_status", { libraryId })
        .then((statuses) => {
          const statusByKind = new Map(statuses.map((entry) => [entry.kind, entry]));

          configs.forEach((config) => {
            const status = statusByKind.get(config.kind);
            const startPending = status?.pending ?? 0;
            const failedBefore = status?.failed ?? 0;
            const completedBefore = status?.completed ?? 0;
            // A prior drain for this same kind (e.g. from the last
            // background-tick) may still be mid-flight — importing a large
            // batch of files can easily take longer than the ~20s tick
            // interval. Starting a second overlapping loop on top of it was
            // stacking concurrent analysis work with nothing bounding it,
            // which is exactly what was driving CPU/memory usage far past
            // what a single drain needs.
            if (config.kind === "audio_analysis" && audioAnalysisPaused) return;
            if (config.kind === "waveform_generation" && waveformGenerationPaused) return;
            if (startPending === 0 || drainingJobKinds.current.has(config.kind)) return;
            drainingJobKinds.current.add(config.kind);

            // total is the *whole* library's job count for this kind
            // (pending + failed + completed already done, possibly across
            // earlier sessions) — not just what's left to do right now.
            // Using startPending alone here used to make every fresh drain
            // (in particular the one right after launch) render as "0 of a
            // small number", indistinguishable from a job that had never
            // made any progress at all: the bar always looked like it was
            // starting over, even though the already-completed work was
            // never touched again and every session really does pick up
            // exactly where the last one left off (claim_pending_jobs only
            // ever selects 'pending' rows — nothing re-does 'completed'
            // ones). done = total - pending below now reflects that.
            const total = startPending + failedBefore + completedBefore;
            setJobProgress((previous) => [
              ...previous.filter((entry) => entry.kind !== config.kind),
              { kind: config.kind, label: config.label, pending: startPending, total, failed: failedBefore }
            ]);
            setJobCompletionSummaries((previous) => {
              if (!(config.kind in previous)) return previous;
              const next = { ...previous };
              delete next[config.kind];
              return next;
            });

            (async () => {
              let remaining = startPending;
              for (let iterations = 0; iterations < 200 && remaining > 0; iterations += 1) {
                // The active library can be swapped mid-drain (File > Open
                // Library / Last Open) while this loop is still awaiting a
                // multi-minute analysis batch for `libraryId`. The job
                // commands below operate on whatever catalog is *currently*
                // active, not necessarily `libraryId` — so once the two
                // diverge, stop rather than keep attributing another
                // library's job counts to this drain, and never let the
                // stale `libraryId` below overwrite the canvas that's now
                // showing a different, unrelated library.
                if (activeLibraryIdRef.current !== libraryId) break;
                const processed = await invoke<number>(config.command).catch(() => 0);
                if (processed === 0) break;
                remaining = Math.max(0, remaining - processed);
                setJobProgress((previous) =>
                  previous.map((entry) => (entry.kind === config.kind ? { ...entry, pending: remaining } : entry))
                );
              }
              setJobProgress((previous) => previous.filter((entry) => entry.kind !== config.kind));
              drainingJobKinds.current.delete(config.kind);
              if (activeLibraryIdRef.current !== libraryId) return;
              // waveform_generation touches no field AssetRecord carries —
              // the strip itself is fetched separately, on demand, via
              // get_waveform (see loadPeaksForAsset) — so refreshing the
              // whole list here bought nothing but churn: every batch (and
              // with a big backlog, this kind drains near-continuously)
              // replaced `assets` wholesale, which the "selection no longer
              // in the list" effect below then reacted to, quietly
              // deselecting/reordering the track a user was in the middle
              // of picking. Skipping it here removes that churn at the
              // source instead of trying to out-guard it downstream.
              if (config.kind !== "waveform_generation") {
                refreshAssets(libraryId, searchQuery, activeFilter);
              }

              // Ground-truth diff against the DB (not the locally-tracked
              // counters above) for the "what actually happened" summary —
              // simpler and more trustworthy than reconciling React state
              // timing with the per-job progress events.
              const finalStatuses = await invoke<JobStatus[]>("job_status", { libraryId }).catch(() => []);
              const finalStatus = finalStatuses.find((entry) => entry.kind === config.kind);
              if (!finalStatus) return;
              const completedDelta = Math.max(0, finalStatus.completed - completedBefore);
              const failedDelta = Math.max(0, finalStatus.failed - failedBefore);
              if (completedDelta === 0 && failedDelta === 0) return;
              const failedExtensions =
                failedDelta > 0
                  ? await invoke<string[]>("failed_job_extensions", { libraryId, kind: config.kind }).catch(() => [])
                  : [];
              setJobCompletionSummaries((previous) => ({
                ...previous,
                [config.kind]: {
                  kind: config.kind,
                  label: config.label,
                  completed: completedDelta,
                  failed: failedDelta,
                  failedExtensions
                }
              }));
            })();
          });
        })
        .catch(() => {});
    },
    [refreshAssets, searchQuery, activeFilter, audioAnalysisPaused, waveformGenerationPaused, instrumentDetectionAvailable]
  );

  const refreshMaintenance = useCallback((libraryId: string) => {
    invoke<MaintenanceReport>("maintenance_report", { libraryId })
      .then(setMaintenanceReport)
      .catch(() => setMaintenanceReport(null));
  }, []);

  const refreshCollections = useCallback((libraryId: string) => {
    invoke<CollectionRecord[]>("list_collections", { libraryId })
      .then(setCollections)
      .catch(() => setCollections([]));
  }, []);

  const refreshLibraryFolders = useCallback((libraryId: string) => {
    invoke<FolderRecord[]>("list_folders", { libraryId })
      .then(setLibraryFolders)
      .catch(() => setLibraryFolders([]));
  }, []);

  const refreshProjectMemberships = useCallback((libraryId: string) => {
    invoke<AssetProjectMembership[]>("project_memberships_for_library", { libraryId })
      .then(setProjectMemberships)
      .catch(() => setProjectMemberships([]));
  }, []);

  const refreshTrashItems = useCallback((libraryId: string) => {
    invoke<TrashItem[]>("list_trash_items", { libraryId })
      .then(setTrashItems)
      .catch(() => setTrashItems([]));
  }, []);

  const refreshAssetTags = useCallback((assetId: string) => {
    invoke<TagRecord[]>("tags_for_asset", { assetId }).then(setAppliedTags).catch(() => setAppliedTags([]));
    invoke<TagRecord[]>("suggested_tags_for_asset", { assetId })
      .then(setSuggestedTags)
      .catch(() => setSuggestedTags([]));
    invoke<SourceRecordDraft | null>("get_source_record", { assetId })
      .then((record) => setSourceDraft(record ?? emptySourceDraft(assetId)))
      .catch(() => setSourceDraft(null));
    setLicenseDateCandidates([]);
    setLicenseDocumentStatus(null);
  }, []);

  useEffect(() => {
    invoke<ReleaseReadinessItem[]>("release_readiness_items")
      .then(setReleaseItems)
      .catch(() => setReleaseItems(fallbackReleaseItems));
  }, []);

  // Mac App Store build: get_license_status always resolves { active: true }
  // (crates/preferences has no equivalent — this is the src-tauri
  // license.rs stub compiled in place of the real direct-dist licensing
  // code), so licenseMode becomes "full" immediately and none of the
  // trial/paywall UI below ever renders. Same JS for both build variants.
  useEffect(() => {
    invoke<LicenseStatus>("get_license_status")
      .then((status) => {
        // Fresh install, direct-dist build, no trial started yet and no
        // license on file — init_trial is idempotent, so this is safe to
        // call unconditionally whenever the initial check comes back with
        // nothing on record.
        if (!status.active && !status.is_trial && !status.trial_expired) {
          return invoke<LicenseStatus>("init_trial");
        }
        return status;
      })
      .then(setLicenseStatus)
      .catch(() =>
        setLicenseStatus({
          active: true,
          key: null,
          hwid: "",
          message: null,
          is_trial: false,
          trial_days_remaining: null,
          trial_expired: false
        })
      );
  }, []);

  useEffect(() => {
    invoke<boolean>("audio_analysis_paused").then(setAudioAnalysisPaused).catch(() => {});
    invoke<boolean>("waveform_generation_paused").then(setWaveformGenerationPaused).catch(() => {});
    invoke<boolean>("instrument_detection_available")
      .then(setInstrumentDetectionAvailable)
      .catch(() => setInstrumentDetectionAvailable(false));
  }, []);

  const handleToggleAudioAnalysisPaused = useCallback(() => {
    const next = !audioAnalysisPaused;
    setAudioAnalysisPaused(next);
    invoke("set_audio_analysis_paused", { paused: next }).catch(() => {});
    if (next) {
      // The in-flight drain loop's current backend call can take a while
      // to come back (it's mid-flight, possibly queued behind other work)
      // and only notices the pause on its *next* iteration — without this,
      // its live "Analyzing audio 22/3421 · 1%" row sat there next to the
      // new "Audio analysis paused" row until that call finally returned,
      // looking like two contradictory rows for the same thing. Hiding it
      // immediately on click is purely cosmetic: the loop still winds
      // itself down and cleans up its own bookkeeping in the background.
      setJobProgress((previous) => previous.filter((entry) => entry.kind !== "audio_analysis"));
    } else if (activeLibraryId) {
      runJobDrain(activeLibraryId, ["audio_analysis"]);
    }
  }, [audioAnalysisPaused, activeLibraryId, runJobDrain]);

  // A library catalogued before waveform caching existed (or one with a lot
  // of Referenced/NAS assets) can carry a backlog thousands deep — decoding
  // through that at WAVEFORM_CONCURRENCY in a debug build pins CPU for a
  // long time and makes everything else (including just selecting a track)
  // feel slow. This is the pause valve for that specifically.
  const handleToggleWaveformGenerationPaused = useCallback(() => {
    const next = !waveformGenerationPaused;
    setWaveformGenerationPaused(next);
    invoke("set_waveform_generation_paused", { paused: next }).catch(() => {});
    if (next) {
      // See the matching comment in handleToggleAudioAnalysisPaused — same
      // "live row lingers next to the new paused row" fix.
      setJobProgress((previous) => previous.filter((entry) => entry.kind !== "waveform_generation"));
    } else if (activeLibraryId) {
      runJobDrain(activeLibraryId, ["waveform_generation"]);
    }
  }, [waveformGenerationPaused, activeLibraryId, runJobDrain]);

  const handleRetryFailedJobs = useCallback(
    (kind: string) => {
      if (!activeLibraryId) return;
      setJobCompletionSummaries((previous) => {
        const next = { ...previous };
        delete next[kind];
        return next;
      });
      invoke("retry_failed_jobs", { libraryId: activeLibraryId, kind })
        .then(() => runJobDrain(activeLibraryId, [kind]))
        .catch(() => {});
    },
    [activeLibraryId, runJobDrain]
  );

  useEffect(() => {
    invoke<AppPreferences>("load_app_preferences")
      .then(setPreferences)
      .catch(() => invoke<AppPreferences>("default_preferences").then(setPreferences));
  }, []);

  useEffect(() => {
    invoke<number>("trash_retention_policy_days").then(setTrashRetentionDays).catch(() => {});
  }, []);

  useEffect(() => {
    invoke<LibraryRecord[]>("list_libraries")
      .then((loaded) => {
        setLibraries(loaded);
        if (loaded.length > 0) setActiveLibraryId(loaded[0].id);
      })
      .catch(() => setLibraries([]))
      .finally(() => setLibrariesLoaded(true));
    // Only actually shown on the first-run screen (libraries.length === 0),
    // but harmless — and one less round trip — to fetch it unconditionally
    // up front rather than only once the wizard is about to render.
    invoke<RecentLibraryFileEntry[]>("list_recent_library_files")
      .then(setRecentLibraryFiles)
      .catch(() => setRecentLibraryFiles([]));
    invoke<string | null>("active_library_file_path")
      .then(setActiveLibraryFilePath)
      .catch(() => setActiveLibraryFilePath(null));
  }, []);

  useEffect(() => {
    invoke<TagRecord[]>("list_tags").then(setTags).catch(() => setTags([]));
  }, [activeLibraryId]);

  useEffect(() => {
    if (!activeLibraryId) return;
    refreshCollections(activeLibraryId);
    refreshLibraryFolders(activeLibraryId);
    refreshProjectMemberships(activeLibraryId);
    refreshMaintenance(activeLibraryId);
    refreshTrashItems(activeLibraryId);
    invoke<[string, boolean]>("media_root_status", { libraryId: activeLibraryId })
      .then(([status, reconnectRequired]) => setMediaRootStatus({ status, reconnectRequired }))
      .catch(() => setMediaRootStatus(null));

    // Fire-and-forget: warms the local playback cache up to the configured budget so
    // browsing feels fast. Bounded by preview_cache_limit_mb (unlike the mutex-holding
    // full-library rescan mistake this app shipped once already — see ADR 0023's
    // follow-up fix — this only ever copies as much as the user's cache budget allows).
    // Once warmed, retry audio analysis: a referenced/NAS asset's job is left
    // pending (not failed) if its file wasn't locally cached yet when analysis
    // first ran, so this is what actually gets it processed instead of it
    // staying stuck pending forever.
    invoke<number>("warm_library_cache", { libraryId: activeLibraryId })
      .then(() => runJobDrain(activeLibraryId, ["audio_analysis"]))
      .catch(() => {});
  }, [
    activeLibraryId,
    refreshCollections,
    refreshLibraryFolders,
    refreshProjectMemberships,
    refreshMaintenance,
    refreshTrashItems,
    runJobDrain
  ]);

  // Auto-import, scan-on-launch half: once per library per session, checks
  // its configured import folder (if any) for new sounds and copies them
  // in — see scan_import_folder. The manual half of the same check lives in
  // handleRefreshLibrary. Guarded by a ref rather than component state so a
  // library switch back to an already-scanned library doesn't re-trigger it.
  useEffect(() => {
    if (!activeLibraryId || !activeLibrary?.import_root) return;
    if (importFolderScannedRef.current.has(activeLibraryId)) return;
    importFolderScannedRef.current.add(activeLibraryId);

    invoke<ImportFolderResult>("scan_import_folder", { libraryId: activeLibraryId })
      .then((result) => {
        if (result.imported.length === 0) return;
        runJobDrain(activeLibraryId);
        refreshAssets(activeLibraryId, searchQuery, activeFilter);
        refreshMaintenance(activeLibraryId);
        setImportStatus(
          `Imported ${result.imported.length} new sound${result.imported.length === 1 ? "" : "s"} from the import folder`
        );
      })
      .catch(() => {});
  }, [activeLibraryId, activeLibrary?.import_root, runJobDrain, refreshAssets, refreshMaintenance, searchQuery, activeFilter]);

  useEffect(() => {
    if (activeLibrary) {
      setOfflineControl({
        media_root: activeLibrary.media_root,
        catalog_only: false,
        validation_paused: false,
        reconnect_requested: false
      });
    } else {
      setOfflineControl(null);
    }
  }, [activeLibrary]);

  // Deleting a library only reloads whichever library becomes active next
  // (or nothing) — without this, a deleted library's assets, trash, and
  // maintenance findings would keep showing until something else happened
  // to trigger a refetch. This is what actually makes "Delete" clean the
  // app's own view of things, not just the backend catalog.
  useEffect(() => {
    if (activeLibraryId) return;
    setAssets([]);
    setSelectedAssetId(null);
    setBrowserState(null);
    setCollections([]);
    setTrashItems([]);
    setMaintenanceReport(null);
  }, [activeLibraryId]);

  useEffect(() => {
    if (!activeLibraryId) return;
    if (typeof activeFilter === "object") {
      refreshAssets(activeLibraryId, searchQuery, activeFilter);
      return;
    }
    const timeout = setTimeout(() => refreshAssets(activeLibraryId, searchQuery, activeFilter), 200);
    return () => clearTimeout(timeout);
  }, [activeLibraryId, searchQuery, activeFilter, refreshAssets]);

  useEffect(() => {
    if (!searchQuery.trim()) {
      setQueryFilters([]);
      return;
    }
    const timeout = setTimeout(() => {
      invoke<VisibleFilter[]>("explain_search_query", { query: searchQuery })
        .then(setQueryFilters)
        .catch(() => setQueryFilters([]));
    }, 200);
    return () => clearTimeout(timeout);
  }, [searchQuery]);

  useEffect(() => {
    if (selectedAssetId && !assets.some((asset) => asset.id === selectedAssetId)) {
      setSelectedAssetId(null);
    }
  }, [assets, selectedAssetId]);

  useEffect(() => {
    if (!selectedAssetId) return;
    const container = browserScrollRef.current;
    if (!container) return;
    const index = visibleAssets.findIndex((asset) => asset.id === selectedAssetId);
    if (index === -1) return;

    // Computed directly from the row's index rather than found-and-scrolled
    // via the DOM, since a virtualized row outside the current window
    // doesn't exist as an element yet to scroll to — this is what actually
    // brings it into the rendered range in the first place.
    const rowTop = index * browserRowHeightPx;
    const rowBottom = rowTop + browserRowHeightPx;
    if (rowTop < container.scrollTop) {
      container.scrollTop = rowTop;
    } else if (rowBottom > container.scrollTop + container.clientHeight) {
      container.scrollTop = rowBottom - container.clientHeight;
    }
  }, [selectedAssetId, visibleAssets, browserRowHeightPx]);

  useEffect(() => {
    setSonicRadarStatus(null);
    setSonicRadarBusyKind(null);
    sonicRadarRequestIdRef.current += 1; // cancel any in-flight poll for the previous asset
    if (!selectedAssetId) {
      setAppliedTags([]);
      setSuggestedTags([]);
      setSourceDraft(null);
      setAssetInstruments([]);
      setStemGroupMembers([]);
      return;
    }
    refreshAssetTags(selectedAssetId);
    invoke<{ name: string; confidence: number }[]>("asset_instruments", { assetId: selectedAssetId })
      .then(setAssetInstruments)
      .catch(() => setAssetInstruments([]));
  }, [selectedAssetId, refreshAssetTags]);

  // Separate from the effect above: keyed on the group id (a plain string,
  // stable across re-renders) rather than the freshly-`.find()`'d
  // selectedAsset object, which would otherwise re-trigger this on every
  // render rather than only when the selection actually changes.
  const selectedAssetStemGroupId = selectedAsset?.stem_group_id ?? null;
  useEffect(() => {
    if (!selectedAssetStemGroupId) {
      setStemGroupMembers([]);
      return;
    }
    let live = true;
    invoke<AssetRecord[]>("stem_group_members", { groupId: selectedAssetStemGroupId })
      .then((members) => {
        if (live) setStemGroupMembers(members);
      })
      .catch(() => {
        if (live) setStemGroupMembers([]);
      });
    return () => {
      live = false;
    };
  }, [selectedAssetStemGroupId]);

  const loadAssetForPlayback = useCallback(async (asset: AssetRecord, autoplay: boolean) => {
    const path = await invoke<string>("asset_playback_path", { assetId: asset.id });
    const audio = audioRef.current;
    if (!audio) return;

    audio.src = convertFileSrc(path);
    setPlayingAssetId(asset.id);
    if (autoplay) {
      audio.play().catch(() => setIsPlaying(false));
    }

    const requestId = ++peakRequestId.current;
    const isCurrent = () => peakRequestId.current === requestId;

    // 1. Session memo — no round-trip at all while browsing the same list.
    const memoized = waveformStripCache.get(asset.id);
    if (memoized) {
      setPeaks(memoized);
      return;
    }
    setPeaks(null);

    // 2. Durable backend cache — a hit means the source file is never read.
    try {
      const cached = await invoke<{ peaks: number[]; sample_rate: number } | null>("get_waveform", {
        assetId: asset.id
      });
      if (cached && cached.peaks.length > 0) {
        rememberWaveformStrip(asset.id, cached.peaks);
        if (isCurrent()) setPeaks(cached.peaks);
        return;
      }
    } catch {
      // fall through to the client-side compute
    }
    if (!isCurrent()) return;

    // 3. Miss — decode once in the WebView, render it, and hand it back to
    //    the backend so this asset is a cache hit from here on.
    const computed = await computePeaks(path);
    if (!isCurrent()) return;
    if (computed) {
      rememberWaveformStrip(asset.id, computed.peaks);
      setPeaks(computed.peaks);
      invoke("store_waveform_peaks", {
        assetId: asset.id,
        peaks: computed.peaks,
        sampleRate: Math.round(computed.sampleRate) || 44_100
      }).catch(() => {});
    } else {
      setPeaks(null);
    }
  }, []);

  const togglePlayback = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;

    // Keyed on playingAssetId alone (matching the per-row play button) so
    // this stays a "pause/resume what's actually playing" control even
    // after the user selects a different row without playing it — merely
    // browsing the list is the normal case, not an edge case, and
    // selectedAssetId routinely diverges from playingAssetId once that
    // happens (bug-log-v2 OPEN-1).
    if (playingAssetId) {
      if (audio.paused) audio.play().catch(() => {});
      else audio.pause();
      return;
    }

    if (selectedAsset) {
      loadAssetForPlayback(selectedAsset, true);
    }
  }, [playingAssetId, selectedAsset, loadAssetForPlayback]);

  const playRelative = useCallback(
    (direction: 1 | -1) => {
      if (visibleAssets.length === 0) return;
      const currentId = playingAssetId ?? selectedAssetId;
      const currentIndex = visibleAssets.findIndex((asset) => asset.id === currentId);
      const nextIndex = currentIndex === -1 ? 0 : (currentIndex + direction + visibleAssets.length) % visibleAssets.length;
      const nextAsset = visibleAssets[nextIndex];
      setSelectedAssetId(nextAsset.id);
      loadAssetForPlayback(nextAsset, true);
      if (browserState) {
        invoke<BrowserState>("apply_browser_command", {
          browserState,
          command: { FocusRow: { index: nextIndex } }
        })
          .then((focused) =>
            invoke<BrowserState>("apply_browser_command", {
              browserState: focused,
              command: { SelectFocused: { mode: "Replace" } }
            })
          )
          .then(setBrowserState)
          .catch(() => {});
      }
    },
    [visibleAssets, playingAssetId, selectedAssetId, loadAssetForPlayback, browserState]
  );

  const handleToggleFavorite = useCallback((asset: AssetRecord) => {
    const nextFavorite = !asset.favorite;
    invoke("set_favorite", { assetId: asset.id, favorite: nextFavorite })
      .then(() => {
        setAssets((previous) =>
          previous.map((entry) => (entry.id === asset.id ? { ...entry, favorite: nextFavorite } : entry))
        );
      })
      .catch(() => {});
  }, []);

  const handleRelinkAsset = useCallback(
    async (asset: AssetRecord) => {
      const newPath = await openDialog({
        directory: false,
        multiple: false,
        title: `Locate "${asset.display_name}"`
      });
      if (typeof newPath !== "string") return;

      invoke("relink_asset", { assetId: asset.id, newPath })
        .then(() => {
          // The relinked file is a different file — drop the remembered
          // strip so the next play loads the new file's shape (the backend
          // cache was cleared server-side and a fresh job enqueued).
          waveformStripCache.delete(asset.id);
          if (activeLibraryId) refreshAssets(activeLibraryId, searchQuery, activeFilter);
        })
        .catch(() => {});
    },
    [activeLibraryId, searchQuery, activeFilter, refreshAssets]
  );

  const handleToggleReviewed = useCallback((asset: AssetRecord) => {
    const reviewed = asset.review_state !== "Reviewed";
    invoke("set_reviewed", { assetId: asset.id, reviewed })
      .then(() => {
        setAssets((previous) =>
          previous.map((entry) =>
            entry.id === asset.id ? { ...entry, review_state: reviewed ? "Reviewed" : "Unreviewed" } : entry
          )
        );
      })
      .catch(() => {});
  }, []);

  const handleSetMediaType = useCallback((asset: AssetRecord, mediaType: string) => {
    if (asset.media_type === mediaType) return;
    invoke("set_media_type", { assetId: asset.id, mediaType })
      .then(() => {
        setAssets((previous) =>
          previous.map((entry) => (entry.id === asset.id ? { ...entry, media_type: mediaType } : entry))
        );
      })
      .catch(() => {});
  }, []);

  const handleFindSimilar = useCallback(() => {
    if (!selectedAssetId || !activeLibraryId) return;
    setSimilarStatus("Finding similar sounds…");
    invoke<AssetRecord[]>("similar_assets", {
      libraryId: activeLibraryId,
      assetId: selectedAssetId,
      limit: 24
    })
      .then((results) => {
        setAssets(results);
        setSimilarStatus(
          results.length > 0
            ? `Showing ${results.length} similar sound${results.length === 1 ? "" : "s"}`
            : "No other analyzed sounds to compare yet"
        );
      })
      .catch(() =>
        setSimilarStatus("This sound hasn't been analyzed yet — try again once import finishes processing")
      );
  }, [selectedAssetId, activeLibraryId]);

  const handleApplyTag = useCallback(
    (tag: TagRecord) => {
      if (bulkAssetIds.length === 0) return;
      invoke<string>("apply_tag", { assetIds: bulkAssetIds, tagId: tag.id })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Apply "${tag.name}"` }]);
          setRedoStack([]);
          if (selectedAssetId) refreshAssetTags(selectedAssetId);
        })
        .catch(() => {});
    },
    [bulkAssetIds, selectedAssetId, refreshAssetTags]
  );

  const handleRemoveTag = useCallback(
    (tag: TagRecord) => {
      if (!selectedAssetId) return;
      invoke<string>("remove_tag", { assetId: selectedAssetId, tagId: tag.id })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Remove "${tag.name}"` }]);
          setRedoStack([]);
          refreshAssetTags(selectedAssetId);
        })
        .catch(() => {});
    },
    [selectedAssetId, refreshAssetTags]
  );

  const handleCreateAndApplyTag = useCallback(() => {
    if (!newTagName.trim() || !selectedAssetId) return;
    invoke<TagRecord>("create_tag", { name: newTagName.trim(), facet: newTagFacet })
      .then((tag) => {
        setTags((previous) => (previous.some((existing) => existing.id === tag.id) ? previous : [...previous, tag]));
        setNewTagName("");
        return handleApplyTag(tag);
      })
      .catch(() => {});
  }, [newTagName, newTagFacet, selectedAssetId, handleApplyTag]);

  // One-click tagging straight from what Sonic Radar already detected
  // (instrument names, vocal presence) — reuses the existing tag if the
  // library already has one with this name, otherwise creates it under a
  // sensible facet first. Same apply path as the manual "Apply Tag" grid.
  const handleQuickTag = useCallback(
    (name: string, facet: string) => {
      if (bulkAssetIds.length === 0) return;
      const normalized = name.trim().toLowerCase();
      const existing = tags.find(
        (tag) => tag.normalized_name.toLowerCase() === normalized || tag.name.toLowerCase() === normalized
      );
      if (existing) {
        handleApplyTag(existing);
        return;
      }
      invoke<TagRecord>("create_tag", { name: name.trim(), facet })
        .then((tag) => {
          setTags((previous) => (previous.some((existing2) => existing2.id === tag.id) ? previous : [...previous, tag]));
          return handleApplyTag(tag);
        })
        .catch(() => {});
    },
    [bulkAssetIds, tags, handleApplyTag]
  );

  const handleAcceptSuggestion = useCallback(
    (tag: TagRecord) => {
      if (!selectedAssetId) return;
      invoke("accept_suggested_tag", { assetId: selectedAssetId, tagId: tag.id })
        .then(() => refreshAssetTags(selectedAssetId))
        .catch(() => {});
    },
    [selectedAssetId, refreshAssetTags]
  );

  const handleRejectSuggestion = useCallback(
    (tag: TagRecord) => {
      if (!selectedAssetId) return;
      invoke("reject_suggested_tag", { assetId: selectedAssetId, tagId: tag.id })
        .then(() => refreshAssetTags(selectedAssetId))
        .catch(() => {});
    },
    [selectedAssetId, refreshAssetTags]
  );

  const handleUndo = useCallback(() => {
    const entry = undoStack[undoStack.length - 1];
    if (!entry) return;
    invoke("undo_action", { undoId: entry.id }).then(() => {
      setUndoStack((previous) => previous.slice(0, -1));
      setRedoStack((previous) => [...previous, entry]);
      if (activeLibraryId) refreshAssets(activeLibraryId, searchQuery, activeFilter);
      if (selectedAssetId) refreshAssetTags(selectedAssetId);
    });
  }, [undoStack, activeLibraryId, searchQuery, activeFilter, refreshAssets, selectedAssetId, refreshAssetTags]);

  const handleRedo = useCallback(() => {
    const entry = redoStack[redoStack.length - 1];
    if (!entry) return;
    invoke("redo_action", { undoId: entry.id }).then(() => {
      setRedoStack((previous) => previous.slice(0, -1));
      setUndoStack((previous) => [...previous, entry]);
      if (activeLibraryId) refreshAssets(activeLibraryId, searchQuery, activeFilter);
      if (selectedAssetId) refreshAssetTags(selectedAssetId);
    });
  }, [redoStack, activeLibraryId, searchQuery, activeFilter, refreshAssets, selectedAssetId, refreshAssetTags]);

  useEffect(() => {
    const unlistenUndo = listen("menu-undo", () => handleUndo());
    const unlistenRedo = listen("menu-redo", () => handleRedo());
    return () => {
      unlistenUndo.then((dispose) => dispose());
      unlistenRedo.then((dispose) => dispose());
    };
  }, [handleUndo, handleRedo]);

  const handleCreateProject = useCallback(async () => {
    if (!activeLibraryId || !newProjectName.trim()) return;
    try {
      const project = await invoke<CollectionRecord>("create_project", {
        libraryId: activeLibraryId,
        name: newProjectName.trim(),
        exportPath: newProjectExportPath.trim() || null,
        sfxExportPath: newProjectSfxExportPath.trim() || null
      });
      for (const folder of newProjectExportFolders.filter((entry) => entry.path.trim())) {
        await invoke("add_project_export_folder", {
          projectId: project.id,
          path: folder.path,
          role: folder.role
        }).catch(() => {});
      }
      setCollections((previous) => [...previous, project]);
      setNewProjectName("");
      setNewProjectExportPath("");
      setNewProjectSfxExportPath("");
      setNewProjectExportFolders([]);
    } catch {
      // Surfaced nowhere specific today — matches this handler's existing
      // silent-failure behavior for create_project itself.
    }
  }, [activeLibraryId, newProjectName, newProjectExportPath, newProjectSfxExportPath, newProjectExportFolders]);

  // "+" on the New Project modal adds a blank row (category first, path
  // empty) — pick the category, then use that row's own Browse button to
  // fill in its path. Same pattern as onboarding's advanced-folder step.
  const handleAddNewProjectExportFolderRow = useCallback(() => {
    setNewProjectExportFolders((previous) => [
      ...previous,
      { id: crypto.randomUUID(), path: "", role: PROJECT_EXPORT_FOLDER_ROLE_OPTIONS[0].value }
    ]);
  }, []);

  const handleBrowseNewProjectExportFolderRow = useCallback(async (id: string) => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose an export folder for this project"
    });
    if (typeof selected !== "string") return;
    setNewProjectExportFolders((previous) =>
      previous.map((folder) => (folder.id === id ? { ...folder, path: selected } : folder))
    );
  }, []);

  const handleSetNewProjectExportFolderRole = useCallback((id: string, role: string) => {
    setNewProjectExportFolders((previous) =>
      previous.map((folder) => (folder.id === id ? { ...folder, role } : folder))
    );
  }, []);

  const handleRemoveNewProjectExportFolderRow = useCallback((id: string) => {
    setNewProjectExportFolders((previous) => previous.filter((folder) => folder.id !== id));
  }, []);

  const handleChooseProjectExportPath = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a sound folder"
    });
    if (typeof selected === "string") setNewProjectExportPath(selected);
  }, []);

  const handleChooseProjectSfxExportPath = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a sound effects folder"
    });
    if (typeof selected === "string") setNewProjectSfxExportPath(selected);
  }, []);

  const handleOpenEditProject = useCallback((project: CollectionRecord) => {
    setEditProjectId(project.id);
    setEditProjectName(project.name);
    setEditProjectExportPath(project.export_path ?? "");
    setEditProjectSfxExportPath(project.sfx_export_path ?? "");
    invoke<ProjectExportFolderRecord[]>("list_project_export_folders", { projectId: project.id })
      .then(setEditProjectExportFolders)
      .catch(() => setEditProjectExportFolders([]));
  }, []);

  const refreshEditProjectExportFolders = useCallback((projectId: string) => {
    invoke<ProjectExportFolderRecord[]>("list_project_export_folders", { projectId })
      .then(setEditProjectExportFolders)
      .catch(() => setEditProjectExportFolders([]));
  }, []);

  // Add/remove/re-tag a categorized export folder on an already-existing
  // project — applied immediately (no separate Save step), same pattern
  // Settings' Watched Folders panel uses for library folders.
  const handleAddEditProjectExportFolder = useCallback(async () => {
    if (!editProjectId) return;
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose an export folder for this project"
    });
    if (typeof selected !== "string") return;
    await invoke("add_project_export_folder", {
      projectId: editProjectId,
      path: selected,
      role: "music"
    }).catch(() => {});
    refreshEditProjectExportFolders(editProjectId);
  }, [editProjectId, refreshEditProjectExportFolders]);

  const handleSetEditProjectExportFolderRole = useCallback(
    async (folderId: string, role: string) => {
      if (!editProjectId) return;
      await invoke("set_project_export_folder_role", { folderId, role }).catch(() => {});
      refreshEditProjectExportFolders(editProjectId);
    },
    [editProjectId, refreshEditProjectExportFolders]
  );

  const handleRemoveEditProjectExportFolder = useCallback(
    async (folderId: string) => {
      if (!editProjectId) return;
      await invoke("remove_project_export_folder", { folderId }).catch(() => {});
      refreshEditProjectExportFolders(editProjectId);
    },
    [editProjectId, refreshEditProjectExportFolders]
  );

  const handleChooseEditProjectExportPath = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: "Choose a sound folder" });
    if (typeof selected === "string") setEditProjectExportPath(selected);
  }, []);

  const handleChooseEditProjectSfxExportPath = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: "Choose a sound effects folder" });
    if (typeof selected === "string") setEditProjectSfxExportPath(selected);
  }, []);

  // Saves whichever of name/export paths actually changed — each field has
  // its own backend command (rename_project, set_project_export_path,
  // set_project_sfx_export_path), so this only calls the ones it needs to
  // rather than always writing all three.
  const handleSaveProjectEdits = useCallback(() => {
    if (!editProjectId) return;
    const project = collections.find((entry) => entry.id === editProjectId);
    if (!project) return;
    const projectId = editProjectId;
    const trimmedName = editProjectName.trim();
    const trimmedExportPath = editProjectExportPath.trim();
    const trimmedSfxExportPath = editProjectSfxExportPath.trim();

    const updates: Promise<CollectionRecord>[] = [];
    if (trimmedName && trimmedName !== project.name) {
      updates.push(invoke<CollectionRecord>("rename_project", { projectId, name: trimmedName }));
    }
    if (trimmedExportPath !== (project.export_path ?? "")) {
      updates.push(
        invoke<CollectionRecord>("set_project_export_path", {
          projectId,
          exportPath: trimmedExportPath || null
        })
      );
    }
    if (trimmedSfxExportPath !== (project.sfx_export_path ?? "")) {
      updates.push(
        invoke<CollectionRecord>("set_project_sfx_export_path", {
          projectId,
          sfxExportPath: trimmedSfxExportPath || null
        })
      );
    }
    if (updates.length === 0) {
      setEditProjectId(null);
      return;
    }
    // Each call returns the full row post-update; the last one to resolve
    // reflects every field that changed, so just take the last result.
    Promise.all(updates)
      .then((results) => {
        const updated = results[results.length - 1];
        setCollections((previous) => previous.map((entry) => (entry.id === updated.id ? updated : entry)));
        setEditProjectId(null);
      })
      .catch(() => {});
  }, [editProjectId, editProjectName, editProjectExportPath, editProjectSfxExportPath, collections]);

  const handleExportToProject = useCallback(
    (project: CollectionRecord, assetIds: string[]) => {
      if (assetIds.length === 0 || (!project.export_path && !project.sfx_export_path)) return;
      Promise.all(assetIds.map((assetId) => invoke<string>("export_asset_to_project", { assetId, projectId: project.id })))
        .then(() => {
          setLastExportProjectId(project.id);
          if (activeLibraryId) refreshProjectMemberships(activeLibraryId);
        })
        .catch(() => {});
    },
    [activeLibraryId, refreshProjectMemberships]
  );

  // Structure sync runs invisibly in the backend (see spawn_resolve_structure_sync's
  // doc comment) — this just polls the result. Only ever populates entries
  // for projects that have actually attempted a sync.
  const refreshResolveSyncStatuses = useCallback((libraryId: string) => {
    invoke<ResolveSyncStatus[]>("get_resolve_sync_statuses_for_library", { libraryId })
      .then((statuses) => {
        setResolveSyncStatuses(Object.fromEntries(statuses.map((status) => [status.collection_id, status])));
      })
      .catch(() => {});
  }, []);

  const handleApproveResolveSync = useCallback((project: CollectionRecord) => {
    invoke("approve_project_resolve_sync", { projectId: project.id })
      .then(() => {
        setResolveSyncStatuses((previous) => ({
          ...previous,
          [project.id]: { ...previous[project.id], approved_at: new Date().toISOString() }
        }));
      })
      .catch((error) => setEditorActionStatus(`Could not approve: ${String(error)}`));
  }, []);

  const handleSendAssetsToResolveTimeline = useCallback((assetIds: string[], project: CollectionRecord) => {
    if (assetIds.length === 0) return;
    setEditorActionStatus(
      assetIds.length === 1 ? "Sending to Resolve…" : `Sending ${assetIds.length} sounds to Resolve…`
    );
    // Sequential, not Promise.all — each one lands on the timeline at
    // "the current playhead," so firing them concurrently would race for
    // the same position instead of landing back-to-back.
    (async () => {
      let sent = 0;
      let lastError: string | null = null;
      for (const assetId of assetIds) {
        try {
          await invoke<string>("send_asset_to_resolve_timeline", { assetId, projectId: project.id });
          sent += 1;
        } catch (error) {
          lastError = String(error);
        }
      }
      if (lastError && sent === 0) {
        setEditorActionStatus(`Send to Resolve failed: ${lastError}`);
      } else if (lastError) {
        setEditorActionStatus(`Sent ${sent}/${assetIds.length} — last error: ${lastError}`);
      } else {
        setEditorActionStatus(sent === 1 ? "Sent to Resolve" : `Sent ${sent} sounds to Resolve`);
      }
    })();
  }, []);

  // Per-row "send to folder" — independent of the sidebar/bulk-selection
  // export buttons above: any track already associated with a project can
  // be sent straight to that project's folder at any time, right from its
  // own row. If the user is currently browsing a specific project, that's
  // the target (matches "the button in the tracks after status" request);
  // otherwise it sends to every project the track belongs to that has
  // either a sound or a sound effects folder configured — the backend
  // picks the right one per asset based on its media type.
  const handleSendAssetToItsProjects = useCallback(
    (asset: AssetRecord, memberships: AssetProjectMembership[]) => {
      const hasAnyFolder = (membership: AssetProjectMembership) =>
        Boolean(membership.export_path || membership.sfx_export_path);
      const currentProjectId =
        typeof activeFilter === "object" && "project" in activeFilter ? activeFilter.project : null;
      const inCurrentProject = currentProjectId
        ? memberships.find((membership) => membership.project_id === currentProjectId && hasAnyFolder(membership))
        : undefined;
      const targets = inCurrentProject ? [inCurrentProject] : memberships.filter(hasAnyFolder);
      if (targets.length === 0) return;

      Promise.all(
        targets.map((target) =>
          invoke<string>("export_asset_to_project", { assetId: asset.id, projectId: target.project_id })
        )
      )
        .then(() => {
          if (activeLibraryId) refreshProjectMemberships(activeLibraryId);
        })
        .catch(() => {});
    },
    [activeFilter, activeLibraryId, refreshProjectMemberships]
  );

  const handleRevealAssets = useCallback((assetIds: string[]) => {
    if (assetIds.length === 0) return;
    setEditorActionStatus(assetIds.length === 1 ? "Revealing…" : `Revealing ${assetIds.length} sounds…`);
    Promise.all(assetIds.map((assetId) => invoke<string>("asset_playback_path", { assetId })))
      .then((paths) => revealItemInDir(paths.length === 1 ? paths[0] : paths))
      .then(() => setEditorActionStatus(`Revealed in ${isMacPlatform ? "Finder" : "Explorer"}`))
      .catch((error) => setEditorActionStatus(`Reveal failed: ${String(error)}`));
  }, []);

  const handleCopyAssetPaths = useCallback((assetIds: string[]) => {
    if (assetIds.length === 0) return;
    Promise.all(assetIds.map((assetId) => invoke<string>("asset_playback_path", { assetId })))
      .then((paths) => writeClipboardText(paths.join("\n")))
      .then(() =>
        setEditorActionStatus(assetIds.length === 1 ? "Copied file path" : `Copied ${assetIds.length} file paths`)
      )
      .catch((error) => setEditorActionStatus(`Copy failed: ${String(error)}`));
  }, []);

  const handleCreateSmartCollection = useCallback(() => {
    if (!activeLibraryId || !smartCollectionName.trim()) return;
    invoke<CollectionRecord>("create_smart_collection", {
      libraryId: activeLibraryId,
      name: smartCollectionName.trim(),
      filters: {
        text: searchQuery,
        duration_min_ms: rangeFilters.durationMinSec != null ? rangeFilters.durationMinSec * 1000 : null,
        duration_max_ms: rangeFilters.durationMaxSec != null ? rangeFilters.durationMaxSec * 1000 : null,
        bpm_min: rangeFilters.bpmMin ?? null,
        bpm_max: rangeFilters.bpmMax ?? null
      }
    })
      .then((collection) => {
        setCollections((previous) => [...previous, collection]);
        setSmartCollectionName("");
      })
      .catch(() => {});
  }, [activeLibraryId, smartCollectionName, searchQuery, rangeFilters]);

  const handleRowClick = useCallback(
    (asset: AssetRecord, index: number, event: MouseEvent) => {
      // A drag-to-project gesture that just released can still trigger a
      // trailing click on whatever the pointer started on — ignore it once,
      // so dropping a track onto a project doesn't also reselect/replay it.
      if (suppressNextRowClickRef.current) {
        suppressNextRowClickRef.current = false;
        return;
      }
      setSelectedAssetId(asset.id);
      const mode: SelectionMode = event.shiftKey ? "Range" : event.metaKey || event.ctrlKey ? "Toggle" : "Replace";
      if (mode === "Replace") {
        if (playingAssetId !== asset.id) {
          loadAssetForPlayback(asset, true);
        } else if (audioRef.current?.paused) {
          // Same track, but paused or finished (ended also leaves the
          // element paused) — playingAssetId alone doesn't distinguish
          // "already playing" from "already loaded but not moving", so a
          // click here used to silently no-op. A user's own repro was
          // exactly this: click plays once, pause it, click the same row
          // again expecting it to resume, nothing happens — reads as
          // "sometimes I have to click twice." Resuming directly instead
          // of routing back through loadAssetForPlayback also skips a
          // redundant asset_playback_path round trip and waveform-peaks
          // lookup for a track that's already loaded.
          audioRef.current.play().catch(() => setIsPlaying(false));
        }
      }
      if (!browserState) return;

      invoke<BrowserState>("apply_browser_command", {
        browserState,
        command: { FocusRow: { index } }
      })
        .then((focused) =>
          invoke<BrowserState>("apply_browser_command", {
            browserState: focused,
            command: { SelectFocused: { mode } }
          })
        )
        .then(setBrowserState)
        .catch(() => {});
    },
    [browserState, playingAssetId, loadAssetForPlayback]
  );

  // Bumps a project to the front of recentProjectIds (used to rank the
  // drag dock and, incidentally, nothing else — the DB membership itself
  // is never derived from this).
  const markProjectUsed = useCallback((projectId: string) => {
    setRecentProjectIds((previous) => {
      const next = [projectId, ...previous.filter((id) => id !== projectId)].slice(0, DRAG_DOCK_PROJECT_COUNT);
      try {
        localStorage.setItem(RECENT_PROJECT_IDS_STORAGE_KEY, JSON.stringify(next));
      } catch {
        // Best-effort — a private window or full storage just means the
        // drag dock falls back to the plain project list next launch.
      }
      return next;
    });
  }, []);

  const handleAddSelectedToProject = useCallback(
    (project: CollectionRecord) => {
      if (bulkAssetIds.length === 0) return;
      invoke<string>("add_to_collection", { collectionId: project.id, assetIds: bulkAssetIds })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Add to "${project.name}"` }]);
          setRedoStack([]);
          if (activeLibraryId) refreshProjectMemberships(activeLibraryId);
          markProjectUsed(project.id);
        })
        .catch(() => {});
    },
    [bulkAssetIds, activeLibraryId, refreshProjectMemberships, markProjectUsed]
  );

  // Opens (or closes, toggled from the same badge) the popover listing
  // exactly which tracks are in a project — the count badge on the
  // Projects grid card is otherwise just a number with no way to see what
  // it's counting. Fetched fresh via assets_in_collection rather than
  // reused from `assets` (which may be filtered/paginated and miss members
  // outside the current view) or from projectMemberships (ids only, no
  // display names).
  const handleToggleProjectTracks = useCallback(
    (project: CollectionRecord, anchor: HTMLElement) => {
      setProjectTracksPanel((current) => {
        if (current?.project.id === project.id) return null;
        const rect = anchor.getBoundingClientRect();
        return {
          project,
          position: { top: rect.bottom + 6, left: Math.max(8, rect.right - 260) },
          assets: null
        };
      });
    },
    []
  );

  useEffect(() => {
    if (!projectTracksPanel || projectTracksPanel.assets !== null) return;
    const projectId = projectTracksPanel.project.id;
    invoke<AssetRecord[]>("assets_in_collection", { collectionId: projectId })
      .then((result) => {
        setProjectTracksPanel((current) =>
          current && current.project.id === projectId ? { ...current, assets: result } : current
        );
      })
      .catch(() => {
        setProjectTracksPanel((current) => (current && current.project.id === projectId ? { ...current, assets: [] } : current));
      });
  }, [projectTracksPanel]);

  // Drag a track (or, if the dragged row is part of the current selection,
  // every selected track) and drop it on a project name — in the left
  // sidebar's Projects list or the inspector's Projects chips — to add it
  // there. Picking up an unselected row only moves that one row, matching
  // ordinary file-manager drag conventions.
  //
  // This is a hand-rolled pointer drag, not native HTML5 DnD: native drag's
  // only customizable visual is `setDragImage`, a single frozen snapshot
  // taken at drag start — it can't animate. Tracking the pointer ourselves
  // lets dragPreview (rendered below with `motion`) morph and follow the
  // cursor smoothly for the whole gesture instead.
  const handleAssetPointerDown = useCallback((event: ReactPointerEvent<HTMLElement>, asset: AssetRecord) => {
    if (event.button !== 0) return; // left button / primary touch only
    // Without this, the browser reads the mousedown+move as a text-selection
    // drag (Chromium/WebKit's default for a mousedown-then-move over
    // non-form content) and highlights everything the cursor crosses —
    // rows, sidebar labels, project names — for the whole gesture. This is
    // the actual custom drag, so it owns the gesture from the first pixel.
    event.preventDefault();
    dragStartRef.current = { x: event.clientX, y: event.clientY, asset };
  }, []);

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const start = dragStartRef.current;
      if (!start) return;
      if (!dragActiveRef.current) {
        if (Math.hypot(event.clientX - start.x, event.clientY - start.y) < DRAG_ACTIVATE_DISTANCE_PX) return;
        dragActiveRef.current = true;
        draggedIdsRef.current = bulkAssetIds.includes(start.asset.id) ? bulkAssetIds : [start.asset.id];
        setDragPreview({ label: start.asset.display_name, count: draggedIdsRef.current.length });
      }
      dragPreviewX.set(event.clientX);
      dragPreviewY.set(event.clientY);
      const hovered = (event.target as HTMLElement | null)
        ?.ownerDocument?.elementFromPoint(event.clientX, event.clientY)
        ?.closest<HTMLElement>("[data-drop-project-id]");
      setDragOverProjectId(hovered?.dataset.dropProjectId ?? null);
    };

    const handlePointerUp = (event: PointerEvent) => {
      const wasDragging = dragActiveRef.current;
      dragStartRef.current = null;
      dragActiveRef.current = false;
      setDragPreview(null);
      setDragOverProjectId(null);
      if (!wasDragging) return;
      suppressNextRowClickRef.current = true;
      const hovered = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>("[data-drop-project-id]");
      const projectId = hovered?.dataset.dropProjectId;
      const ids = draggedIdsRef.current;
      draggedIdsRef.current = [];
      if (!projectId || ids.length === 0) return;
      const project = collections.find((entry) => entry.id === projectId);
      if (!project || project.collection_type !== "Project") return;
      invoke<string>("add_to_collection", { collectionId: project.id, assetIds: ids })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Add to "${project.name}"` }]);
          setRedoStack([]);
          if (activeLibraryId) refreshProjectMemberships(activeLibraryId);
          markProjectUsed(project.id);
        })
        .catch(() => {});
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
  }, [bulkAssetIds, collections, activeLibraryId, refreshProjectMemberships, dragPreviewX, dragPreviewY, markProjectUsed]);

  // The sidebar's per-project "+" — a one-click way to add whatever's
  // currently playing to a project without first selecting it in the
  // browser, independent of the bulk-selection Projects chips above.
  const handleAddPlayingToProject = useCallback(
    (project: CollectionRecord) => {
      if (!playingAssetId) return;
      invoke<string>("add_to_collection", { collectionId: project.id, assetIds: [playingAssetId] })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Add to "${project.name}"` }]);
          setRedoStack([]);
          if (activeLibraryId) refreshProjectMemberships(activeLibraryId);
          markProjectUsed(project.id);
        })
        .catch(() => {});
    },
    [playingAssetId, activeLibraryId, refreshProjectMemberships, markProjectUsed]
  );

  // Recent projects (most-recently-used first, backfilled from the plain
  // project list so there's always something to show) for the drag dock —
  // a fast-access shortcut that pops up during a drag so dropping a track
  // into a project never depends on either sidebar's Projects section
  // happening to be expanded/scrolled into view.
  const dragDockProjects = useMemo(() => {
    const projectCollections = collections.filter((entry) => entry.collection_type === "Project");
    const byId = new Map(projectCollections.map((entry) => [entry.id, entry] as const));
    const ranked: CollectionRecord[] = [];
    for (const id of recentProjectIds) {
      const match = byId.get(id);
      if (match) ranked.push(match);
    }
    for (const entry of projectCollections) {
      if (ranked.length >= DRAG_DOCK_PROJECT_COUNT) break;
      if (!ranked.includes(entry)) ranked.push(entry);
    }
    return ranked.slice(0, DRAG_DOCK_PROJECT_COUNT);
  }, [collections, recentProjectIds]);

  const handleSaveSource = useCallback(() => {
    if (!sourceDraft) return;
    invoke("set_source_record", { draft: sourceDraft })
      .then(() => activeLibraryId && refreshMaintenance(activeLibraryId))
      .catch(() => {});
  }, [sourceDraft, activeLibraryId, refreshMaintenance]);

  const handleAttachLicenseDocument = useCallback(async () => {
    if (!sourceDraft) return;
    const selected = await openDialog({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
      title: "Choose a license PDF"
    });
    if (typeof selected !== "string") return;

    setLicenseDocumentStatus("Attaching…");
    try {
      const outcome = await invoke<LicenseDocumentAttachOutcome>("attach_license_document", {
        assetId: sourceDraft.asset_id,
        sourcePath: selected
      });
      setLicenseDateCandidates(outcome.candidates);
      setSourceDraft((previous) => {
        if (!previous) return previous;
        // A single high-confidence candidate is worth pre-filling so
        // confirming is one click — but never overwrite a date the user
        // already entered or confirmed.
        const singleCandidate = outcome.candidates.length === 1 ? outcome.candidates[0] : null;
        return {
          ...previous,
          license_document_path: outcome.source.license_document_path,
          ...(singleCandidate && !previous.license_valid_until
            ? { license_valid_until: singleCandidate.date, license_expiry_source: "extracted" }
            : {})
        };
      });
      setLicenseDocumentStatus(
        outcome.candidates.length > 0
          ? `Attached — found ${outcome.candidates.length} possible expiry date${outcome.candidates.length === 1 ? "" : "s"}, please confirm below`
          : "Attached — no expiry date detected, enter one manually if known"
      );
      if (activeLibraryId) refreshMaintenance(activeLibraryId);
    } catch (error) {
      setLicenseDocumentStatus(`Attach failed: ${String(error)}`);
    }
  }, [sourceDraft, activeLibraryId, refreshMaintenance]);

  const handleRemoveLicenseDocument = useCallback(() => {
    if (!sourceDraft) return;
    const assetId = sourceDraft.asset_id;
    invoke("remove_license_document", { assetId })
      .then(() => invoke<SourceRecordDraft | null>("get_source_record", { assetId }))
      .then((record) => {
        setSourceDraft(record ?? emptySourceDraft(assetId));
        setLicenseDateCandidates([]);
        setLicenseDocumentStatus("License document removed");
        if (activeLibraryId) refreshMaintenance(activeLibraryId);
      })
      .catch((error) => setLicenseDocumentStatus(`Remove failed: ${String(error)}`));
  }, [sourceDraft, activeLibraryId, refreshMaintenance]);

  const handleRevealLicenseDocument = useCallback(() => {
    if (!sourceDraft) return;
    invoke<string | null>("resolve_license_document_path", { assetId: sourceDraft.asset_id })
      .then((path) => {
        if (path) return revealItemInDir(path);
      })
      .catch((error) => setLicenseDocumentStatus(`Reveal failed: ${String(error)}`));
  }, [sourceDraft]);

  const handleUseLicenseDateCandidate = useCallback((date: string) => {
    setSourceDraft((previous) =>
      previous ? { ...previous, license_valid_until: date, license_expiry_source: "extracted" } : previous
    );
  }, []);

  const handleMoveToTrash = useCallback(() => {
    if (!selectedAssetId || !activeLibraryId) return;
    invoke("move_to_trash", { assetId: selectedAssetId, reason: "manual" })
      .then(() => {
        setSelectedAssetId(null);
        refreshAssets(activeLibraryId, searchQuery, activeFilter);
        refreshTrashItems(activeLibraryId);
        refreshMaintenance(activeLibraryId);
      })
      .catch(() => {});
  }, [selectedAssetId, activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshTrashItems, refreshMaintenance]);

  const handleTrashDuplicateGroup = useCallback(
    (assetIds: string[]) => {
      if (!activeLibraryId) return;
      invoke("trash_duplicate_group", { assetIds })
        .then(() => {
          refreshAssets(activeLibraryId, searchQuery, activeFilter);
          refreshTrashItems(activeLibraryId);
          refreshMaintenance(activeLibraryId);
        })
        .catch(() => {});
    },
    [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshTrashItems, refreshMaintenance]
  );

  const handleRestoreFromTrash = useCallback(
    (item: TrashItem) => {
      if (!activeLibraryId) return;
      invoke("restore_from_trash", { assetId: item.asset_id })
        .then(() => {
          refreshAssets(activeLibraryId, searchQuery, activeFilter);
          refreshTrashItems(activeLibraryId);
        })
        .catch(() => {});
    },
    [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshTrashItems]
  );

  // The real thing: deletes the file itself from wherever it lives on disk
  // (not just the catalog row) — no retention wait, since the user just
  // confirmed it explicitly for this one item.
  const handleDeleteTrashItemPermanently = useCallback(
    (item: TrashItem) => {
      if (!activeLibraryId) return;
      const filename = item.original_path.split(/[/\\]/).pop() ?? item.original_path;
      confirmDialog(`Permanently delete "${filename}"? This removes the actual file from disk and cannot be undone.`, {
        title: "Delete Permanently",
        kind: "warning"
      })
        .then((confirmed) => {
          if (!confirmed) return;
          return invoke("delete_trash_item_permanently", { assetId: item.asset_id }).then(() => {
            if (activeLibraryId) refreshTrashItems(activeLibraryId);
          });
        })
        .catch(() => {});
    },
    [activeLibraryId, refreshTrashItems]
  );

  const handleOfflineCommand = useCallback(
    (command: OfflineControlCommand) => {
      if (!offlineControl) return;
      invoke<OfflineControlState>("apply_offline_control", { offlineState: offlineControl, command })
        .then(setOfflineControl)
        .catch(() => {});
    },
    [offlineControl]
  );

  const handleRetryReconnect = useCallback(() => {
    if (!offlineControl || !activeLibraryId) return;
    invoke<OfflineControlState>("apply_offline_control", { offlineState: offlineControl, command: "RetryReconnect" })
      .then(setOfflineControl)
      .catch(() => {});

    setReconnectStatus("Validating…");
    invoke<[number, ReconnectValidationReport | null]>("validate_reconnect", { libraryId: activeLibraryId })
      .then(([changed, report]) => {
        refreshAssets(activeLibraryId, searchQuery, activeFilter);
        refreshMaintenance(activeLibraryId);
        invoke<[string, boolean]>("media_root_status", { libraryId: activeLibraryId })
          .then(([status, reconnectRequired]) => setMediaRootStatus({ status, reconnectRequired }))
          .catch(() => {});

        if (report) {
          setReconnectStatus(
            `${changed} asset${changed === 1 ? "" : "s"} updated — ${report.missing_paths.length} of ${report.checked_paths} managed paths still missing`
          );
        } else {
          setReconnectStatus(`${changed} asset${changed === 1 ? "" : "s"} updated`);
        }
      })
      .catch((error) => setReconnectStatus(`Reconnect validation failed: ${String(error)}`));
  }, [offlineControl, activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance]);

  const handleBackupLibrary = useCallback(async () => {
    if (!activeLibraryId) return;
    const destination = await saveDialog({
      title: "Save a backup of this library",
      defaultPath: `${activeLibrary?.name ?? "Library"}.darkwavebak`,
      filters: [{ name: "Darkwave Library Backup", extensions: ["darkwavebak"] }]
    });
    if (typeof destination !== "string") return;

    // Same corruption-risk check New Setup's save dialog already runs —
    // a backup file is an equally critical single-file write, so a
    // dropped connection mid-save deserves the same warning here.
    const tolerant = await invoke<boolean>("is_path_network_tolerant", { path: destination }).catch(() => true);
    const networkWarning = tolerant
      ? ""
      : " — this looks like a network drive; a dropped connection mid-save risks corrupting it";

    setBackupStatus("Backing up…");
    try {
      const backupPackage = await invoke<BackupPackage>("backup_library", {
        backupFilePath: destination
      });
      setBackupStatus(`Backed up to ${backupPackage.backup_file_path}${networkWarning}`);
    } catch (error) {
      setBackupStatus(`Backup failed: ${String(error)}`);
    }
  }, [activeLibraryId, activeLibrary?.name]);

  const handleRestoreLibrary = useCallback(async () => {
    const source = await openDialog({
      multiple: false,
      title: "Choose a backup to restore",
      filters: [{ name: "Darkwave Library Backup", extensions: ["darkwavebak"] }]
    });
    if (typeof source !== "string") return;

    const confirmed = await confirmDialog(
      "This replaces the current library with the selected backup. A safety copy of the current file is kept, but any changes made since the backup was taken will no longer be visible.",
      { title: "Restore library from backup", kind: "warning" }
    );
    if (!confirmed) return;

    setBackupStatus("Restoring…");
    try {
      const library = await invoke<LibraryRecord>("restore_library", { backupFilePath: source });
      setBackupStatus(`Restored "${library.name}" from backup`);
      setSelectedAssetId(null);
      setLibraries([library]);
      setActiveLibraryId(library.id);
    } catch (error) {
      setBackupStatus(`Restore failed: ${String(error)}`);
    }
  }, []);

  const handlePurgeCache = useCallback(() => {
    invoke("purge_preview_cache")
      .then(() => setCacheStatus("Cache cleared"))
      .catch((error) => setCacheStatus(`Purge failed: ${String(error)}`));
  }, []);

  const handleCleanLibraryCache = useCallback((library: LibraryRecord) => {
    setLibraryAdminStatus(`Cleaning ${library.name}'s cache…`);
    invoke<number>("purge_library_cache", { libraryId: library.id })
      .then((removed) => setLibraryAdminStatus(`Cleared ${removed} cached file${removed === 1 ? "" : "s"} for ${library.name}`))
      .catch((error) => setLibraryAdminStatus(`Cache clean failed: ${String(error)}`));
  }, []);

  const handleEmptyLibraryTrash = useCallback(
    (library: LibraryRecord) => {
      setLibraryAdminStatus(`Emptying ${library.name}'s trash…`);
      invoke<number>("empty_library_trash", { libraryId: library.id })
        .then((purged) => {
          setLibraryAdminStatus(`Permanently removed ${purged} item${purged === 1 ? "" : "s"} from ${library.name}'s trash`);
          if (library.id === activeLibraryId) refreshTrashItems(library.id);
        })
        .catch((error) => setLibraryAdminStatus(`Empty trash failed: ${String(error)}`));
    },
    [activeLibraryId, refreshTrashItems]
  );

  const handleDeleteLibrary = useCallback(
    async (library: LibraryRecord) => {
      const confirmed = await confirmDialog(
        `This permanently deletes "${library.name}" from Darkwave — its catalog, tags applied to its sounds, collections, and trash records. The audio files themselves, at ${library.media_root}, are never touched or deleted.`,
        { title: `Delete "${library.name}"?`, kind: "warning" }
      );
      if (!confirmed) return;

      setLibraryAdminStatus(`Deleting ${library.name}…`);
      try {
        const result = await invoke<DeleteLibraryResult>("delete_library", { libraryId: library.id });
        setLibraryAdminStatus(
          `Deleted ${library.name} — cleared ${result.cache_files_removed} cached file${result.cache_files_removed === 1 ? "" : "s"} and its trash is clean (${result.trash_items_cleared} item${result.trash_items_cleared === 1 ? "" : "s"} removed)`
        );
        if (library.id === activeLibraryId) setSelectedAssetId(null);
        const loaded = await invoke<LibraryRecord[]>("list_libraries");
        setLibraries(loaded);
        if (library.id === activeLibraryId) {
          setActiveLibraryId(loaded.length > 0 ? loaded[0].id : null);
        }
      } catch (error) {
        setLibraryAdminStatus(`Delete failed: ${String(error)}`);
      }
    },
    [activeLibraryId]
  );

  const handleCreateLibrary = async () => {
    if (!libraryName.trim()) return;

    const library = await invoke<LibraryRecord>("create_library", {
      name: libraryName.trim(),
      mediaRoot: ""
    });
    setLibraries((previous) => [...previous, library]);
    setActiveLibraryId(library.id);
    setLibraryName("");
    setCreateLibraryModalOpen(false);
  };

  // Resets all onboarding state and lands directly in the app with the
  // given (already fully-configured) library — the path both Open Library
  // and a recent-libraries click take, since neither needs the New Setup
  // folder-configuration steps that follow library creation.
  const finishOnboardingWithLibrary = useCallback((library: LibraryRecord, libraryFilePath: string) => {
    setLibraries([library]);
    setActiveLibraryId(library.id);
    setActiveLibraryFilePath(libraryFilePath);
    setOnboardingLibrary(null);
    setOnboardingLibraryFilePath("");
    setOnboardingMediaRoot("");
    setOnboardingFolderMode("single");
    setOnboardingAdvancedFolders([]);
    setOnboardingNetworkPathWarning(null);
    setOnboardingError(null);
    setOnboardingStep(0);
  }, []);

  const handleBeginNewSetup = useCallback(() => {
    setOnboardingError(null);
    setOnboardingNetworkPathWarning(null);
    setOnboardingStep(1);
  }, []);

  // First-run wizard step 1: name the new library file, pick where to save
  // it (a native save dialog, not a folder picker — this is the .darkwave
  // file itself, distinct from the media-root folder chosen next), and
  // create it — separate from handleCreateLibrary above (used by the
  // always-available "New Library" modal for anyone who already has a
  // library open) since this one hands off into the folder-setup steps
  // instead of finishing immediately.
  const handleOnboardingCreateLibraryFile = useCallback(async () => {
    if (!libraryName.trim()) return;
    setOnboardingBusy(true);
    setOnboardingError(null);
    try {
      const savePath = await saveDialog({
        title: "Save your Darkwave library",
        defaultPath: `${libraryName.trim()}.darkwave`,
        filters: [{ name: "Darkwave Library", extensions: ["darkwave"] }]
      });
      if (typeof savePath !== "string") {
        setOnboardingBusy(false);
        return;
      }
      const tolerant = await invoke<boolean>("is_path_network_tolerant", { path: savePath }).catch(() => true);
      setOnboardingNetworkPathWarning(
        tolerant
          ? null
          : "That location looks like a network drive. Keeping your library file on local disk is safer — a dropped connection mid-save risks corrupting it."
      );
      const library = await invoke<LibraryRecord>("create_library_file", {
        libraryFilePath: savePath,
        name: libraryName.trim()
      });
      setLibraries([library]);
      setActiveLibraryFilePath(savePath);
      setOnboardingLibrary(library);
      setOnboardingLibraryFilePath(savePath);
      setLibraryName("");
      setOnboardingStep(2);
    } catch (error) {
      setOnboardingError(String(error));
    } finally {
      setOnboardingBusy(false);
    }
  }, [libraryName]);

  // First-run "Open Library" path — a native file picker filtered to
  // .darkwave files, distinct from New Setup's save dialog.
  const handleOpenLibraryFile = useCallback(async () => {
    setOnboardingBusy(true);
    setOnboardingError(null);
    try {
      const selected = await openDialog({
        multiple: false,
        title: "Open a Darkwave library",
        filters: [{ name: "Darkwave Library", extensions: ["darkwave"] }]
      });
      if (typeof selected !== "string") {
        setOnboardingBusy(false);
        return;
      }
      const library = await invoke<LibraryRecord>("open_library_file", { libraryFilePath: selected });
      finishOnboardingWithLibrary(library, selected);
    } catch (error) {
      setOnboardingError(String(error));
    } finally {
      setOnboardingBusy(false);
    }
  }, [finishOnboardingWithLibrary]);

  // A click on one of the recent-libraries entries shown alongside New
  // Setup / Open Library — same as Open Library but the path is already
  // known, so no file dialog. A path that no longer resolves (moved,
  // deleted, or its volume unmounted) is dropped from the list rather than
  // left to fail silently again next time.
  const handleOpenRecentLibraryFile = useCallback(
    async (path: string) => {
      setOnboardingBusy(true);
      setOnboardingError(null);
      try {
        const library = await invoke<LibraryRecord>("open_library_file", { libraryFilePath: path });
        finishOnboardingWithLibrary(library, path);
      } catch (error) {
        setOnboardingError(String(error));
        setRecentLibraryFiles((previous) => previous.filter((entry) => entry.path !== path));
      } finally {
        setOnboardingBusy(false);
      }
    },
    [finishOnboardingWithLibrary]
  );

  // Migration screen: moves one library out of the legacy shared catalog
  // into its own `.darkwave` file. A save dialog per library (same pattern
  // as New Setup) rather than a pre-computed default path — lets the user
  // put each migrated library wherever they want without us guessing at a
  // "Darkwave Projects" folder convention on their disk.
  const handleMigrateLibrary = useCallback(async (library: LibraryRecord) => {
    const savePath = await saveDialog({
      title: `Save "${library.name}" as a Darkwave library`,
      defaultPath: `${library.name}.darkwave`,
      filters: [{ name: "Darkwave Library", extensions: ["darkwave"] }]
    });
    if (typeof savePath !== "string") return;

    // Same corruption-risk check New Setup's save dialog already runs —
    // migrating onto a network drive is an equally critical single-file
    // write.
    const tolerant = await invoke<boolean>("is_path_network_tolerant", { path: savePath }).catch(() => true);

    setMigrationStatus((previous) => ({ ...previous, [library.id]: "migrating" }));
    setMigrationErrors((previous) => {
      const { [library.id]: _dropped, ...rest } = previous;
      return rest;
    });
    try {
      await invoke("migrate_legacy_library", { libraryId: library.id, libraryFilePath: savePath });
      setMigrationStatus((previous) => ({ ...previous, [library.id]: "done" }));
      if (!tolerant) {
        setMigrationErrors((previous) => ({
          ...previous,
          [library.id]: "Migrated — but that location looks like a network drive, which risks corruption on a dropped connection."
        }));
      }
    } catch (error) {
      setMigrationStatus((previous) => ({ ...previous, [library.id]: "failed" }));
      setMigrationErrors((previous) => ({ ...previous, [library.id]: String(error) }));
    }
  }, []);

  // Only enabled once every legacy library shows "done" — closes the
  // legacy catalog for good (renamed aside, never opened again) and lands
  // back on the first-run screen, now with the migrated libraries in
  // Recents so the user's next click opens one of them.
  const handleFinishMigration = useCallback(async () => {
    setMigrationBusy(true);
    try {
      await invoke("finish_legacy_migration");
      const [loadedLibraries, loadedRecents, loadedActivePath] = await Promise.all([
        invoke<LibraryRecord[]>("list_libraries"),
        invoke<RecentLibraryFileEntry[]>("list_recent_library_files"),
        invoke<string | null>("active_library_file_path")
      ]);
      setLibraries(loadedLibraries);
      setRecentLibraryFiles(loadedRecents);
      setActiveLibraryFilePath(loadedActivePath);
      setMigrationStatus({});
      setMigrationErrors({});
      // The legacy catalog these referred to is gone (retired by
      // finish_legacy_migration) — clear rather than leave stale ids
      // dangling until a later open/create overwrites them.
      setActiveLibraryId(null);
      setSelectedAssetId(null);
    } catch (error) {
      setOnboardingError(String(error));
    } finally {
      setMigrationBusy(false);
    }
  }, []);

  // "Remind me later" — a session-only dismissal, not a backend call: the
  // legacy catalog is already the live one (see the Rust bootstrap's
  // fallback order), so the app is already fully usable exactly as it was
  // before this feature existed. Nothing was migrated, so the prompt
  // reappears next launch.
  const handleDismissMigration = useCallback(() => {
    setMigrationDismissed(true);
  }, []);

  const handleOnboardingChooseMediaRoot = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose the folder your actual sound files will live in"
    });
    if (typeof selected === "string") setOnboardingMediaRoot(selected);
  }, []);

  // Advanced setup: "+" adds a blank row (type first, path empty) rather
  // than immediately opening a folder dialog — pick the category, then use
  // that row's own Browse button to fill in its path.
  const handleAddOnboardingFolderRow = useCallback(() => {
    setOnboardingAdvancedFolders((previous) => [
      ...previous,
      { id: crypto.randomUUID(), path: "", role: "" }
    ]);
  }, []);

  const handleBrowseOnboardingFolderRow = useCallback(async (id: string) => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a folder (e.g. your Soundtracks, SFX, or Licence folder)"
    });
    if (typeof selected !== "string") return;
    setOnboardingAdvancedFolders((previous) =>
      previous.map((folder) => (folder.id === id ? { ...folder, path: selected } : folder))
    );
  }, []);

  const handleSetOnboardingFolderRole = useCallback((id: string, role: string) => {
    setOnboardingAdvancedFolders((previous) =>
      previous.map((folder) => (folder.id === id ? { ...folder, role } : folder))
    );
  }, []);

  const handleRemoveOnboardingFolderRow = useCallback((id: string) => {
    setOnboardingAdvancedFolders((previous) => previous.filter((folder) => folder.id !== id));
  }, []);

  const handleOnboardingConfirmMediaRoot = useCallback(async () => {
    if (!onboardingLibrary) return;
    const configuredAdvancedFolders = onboardingAdvancedFolders.filter((folder) => folder.path.trim());
    // Single mode requires the one main folder; advanced mode requires at
    // least one of the typed rows to actually have a path chosen — the two
    // modes are mutually exclusive UI, so only the active one's requirement
    // gates Finish.
    if (onboardingFolderMode === "single" ? !onboardingMediaRoot.trim() : configuredAdvancedFolders.length === 0) {
      return;
    }
    setOnboardingBusy(true);
    setOnboardingError(null);
    try {
      let updated = onboardingLibrary;
      if (onboardingFolderMode === "single") {
        updated = await invoke<LibraryRecord>("set_library_media_root", {
          libraryId: onboardingLibrary.id,
          mediaRoot: onboardingMediaRoot.trim()
        });
        setOnboardingLibrary(updated);
        setLibraries((previous) => previous.map((library) => (library.id === updated.id ? updated : library)));
      }

      // set_library_media_root already keeps a matching `folders` row
      // (role null, kind "media_root") in sync on the Rust side — see
      // sync_media_root_folder — so the basic single-folder user needs
      // nothing further here. An advanced user's library keeps an empty
      // media_root at this point — it self-heals to whichever configured
      // folder gets imported into first (see import_folder), and the
      // background poller watches the `folders` rows directly regardless.
      // Independent inserts (no shared ordering, no uniqueness constraint
      // between them) — run concurrently rather than one round trip at a
      // time, same as a user with several advanced folders configured
      // would expect a single "Continue" click to resolve quickly.
      await Promise.allSettled(
        configuredAdvancedFolders.map((folder) =>
          invoke("add_folder", {
            libraryId: updated.id,
            path: folder.path,
            role: folder.role.trim() ? folder.role : null,
            kind: folder.role === "documents" ? "documents" : "watched"
          })
        )
      );

      // The folders just registered above are the canonical watched
      // folders — there's no separate "drop new sounds here" staging
      // folder to configure any more (that's what the old, now-removed
      // import-root wizard step asked for), so this step finishes the
      // wizard directly instead of advancing to another one.
      setActiveLibraryId(updated.id);
      setOnboardingLibrary(null);
      setOnboardingLibraryFilePath("");
      setOnboardingStep(0);
      setOnboardingMediaRoot("");
      setOnboardingFolderMode("single");
      setOnboardingAdvancedFolders([]);
      setOnboardingNetworkPathWarning(null);
    } catch (error) {
      setOnboardingError(String(error));
    } finally {
      setOnboardingBusy(false);
    }
  }, [onboardingLibrary, onboardingMediaRoot, onboardingFolderMode, onboardingAdvancedFolders]);

  // Settings → General → Library's "Import folder" row — sets or clears a
  // library's import folder after the fact, outside the first-run wizard.
  const handleChangeLibraryImportRoot = useCallback(async () => {
    if (!activeLibraryId) return;
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a folder to auto-import new sounds from"
    });
    if (typeof selected !== "string") return;
    const updated = await invoke<LibraryRecord>("set_library_import_root", {
      libraryId: activeLibraryId,
      importRoot: selected
    });
    setLibraries((previous) => previous.map((library) => (library.id === updated.id ? updated : library)));
  }, [activeLibraryId]);

  const handleClearLibraryImportRoot = useCallback(async () => {
    if (!activeLibraryId) return;
    const updated = await invoke<LibraryRecord>("set_library_import_root", {
      libraryId: activeLibraryId,
      importRoot: null
    });
    setLibraries((previous) => previous.map((library) => (library.id === updated.id ? updated : library)));
  }, [activeLibraryId]);

  // Named subfolders under media_root a file dropped onto the window can
  // be filed into (see the externalDropPrompt flow) — 0 or 1 means a drop
  // never has to ask "which one"; 2+ does, unless the user's remembered a
  // choice for this session.
  const handleAddImportSubfolder = useCallback(async () => {
    const name = newImportSubfolderName.trim();
    if (!activeLibraryId || !name) return;
    const next = [...(activeLibrary?.import_subfolders ?? []), name];
    const updated = await invoke<LibraryRecord>("set_library_import_subfolders", {
      libraryId: activeLibraryId,
      names: next
    }).catch(() => null);
    if (updated) {
      setLibraries((previous) => previous.map((library) => (library.id === updated.id ? updated : library)));
      setNewImportSubfolderName("");
    }
  }, [activeLibraryId, activeLibrary, newImportSubfolderName]);

  const handleRemoveImportSubfolder = useCallback(
    async (name: string) => {
      if (!activeLibraryId) return;
      const next = (activeLibrary?.import_subfolders ?? []).filter((existing) => existing !== name);
      const updated = await invoke<LibraryRecord>("set_library_import_subfolders", {
        libraryId: activeLibraryId,
        names: next
      }).catch(() => null);
      if (updated) {
        setLibraries((previous) => previous.map((library) => (library.id === updated.id ? updated : library)));
      }
    },
    [activeLibraryId, activeLibrary]
  );

  const handleImportFolder = useCallback(async () => {
    if (!activeLibraryId) return;

    const folderPath = await openDialog({ directory: true, multiple: false, title: "Choose a folder to import" });
    if (typeof folderPath !== "string") return;

    setImportStatus("Importing…");
    try {
      const result = await invoke<ImportFolderResult>("import_folder", {
        libraryId: activeLibraryId,
        folderPath,
        mode: "referenced"
      });
      setImportStatus(
        result.failed.length > 0
          ? `Imported ${result.imported.length}, ${result.failed.length} failed`
          : `Imported ${result.imported.length} sound${result.imported.length === 1 ? "" : "s"}`
      );
      // The very first import into a library sets its media root
      // automatically (see import_folder) — re-fetching here is what
      // picks that up so Settings/NAS status reflect it immediately
      // instead of only after the next library switch.
      if (!activeLibrary?.media_root) {
        invoke<LibraryRecord[]>("list_libraries").then(setLibraries).catch(() => {});
      }
      refreshAssets(activeLibraryId, searchQuery, activeFilter);
      refreshMaintenance(activeLibraryId);
      runJobDrain(activeLibraryId);
    } catch (error) {
      setImportStatus(`Import failed: ${String(error)}`);
    }
  }, [activeLibraryId, activeLibrary, searchQuery, activeFilter, refreshAssets, refreshMaintenance, runJobDrain]);

  // Actually runs a drop's import once the destination is known (either
  // there was nothing to ask, or handleExternalFileDrop/the prompt modal
  // already resolved one) — copies each path into the library's own
  // folder and catalogs it there; see import_dropped_paths.
  const runDroppedImport = useCallback(
    (libraryId: string, paths: string[], targetSubfolder: string | null) => {
      setImportStatus(`Importing ${paths.length} item${paths.length === 1 ? "" : "s"}…`);
      invoke<DroppedImportResult>("import_dropped_paths", {
        libraryId,
        paths,
        targetSubfolder
      })
        .then((result) => {
          const stemsNote =
            result.stem_groups_detected > 0
              ? ` · ${result.stem_groups_detected} stem group${result.stem_groups_detected === 1 ? "" : "s"} organized`
              : "";
          const summary =
            result.failed.length > 0
              ? `Imported ${result.imported.length}, ${result.failed.length} failed${stemsNote}`
              : `Imported ${result.imported.length} sound${result.imported.length === 1 ? "" : "s"}${stemsNote}`;
          setImportStatus(summary);
          showImportToast(summary);
          if (libraryId === activeLibraryId) {
            refreshAssets(libraryId, searchQuery, activeFilter);
            refreshMaintenance(libraryId);
          }
          runJobDrain(libraryId);
        })
        .catch((error) => setImportStatus(`Import failed: ${String(error)}`));
    },
    [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance, runJobDrain, showImportToast]
  );

  // Entry point for a real OS-level drop (see the onDragDropEvent effect
  // below) — decides whether the destination is already known (0 or 1
  // configured import_subfolders, or a remembered choice from earlier
  // this session) or whether the user needs to be asked.
  const handleExternalFileDrop = useCallback(
    (paths: string[]) => {
      if (!activeLibraryId || paths.length === 0) return;
      const subfolders = activeLibrary?.import_subfolders ?? [];

      if (subfolders.length <= 1) {
        runDroppedImport(activeLibraryId, paths, subfolders[0] ?? null);
        return;
      }

      const remembered = rememberedDropSubfolderRef.current;
      if (remembered && remembered.libraryId === activeLibraryId) {
        runDroppedImport(activeLibraryId, paths, remembered.subfolder);
        return;
      }

      setExternalDropRememberChoice(false);
      setExternalDropPrompt({ libraryId: activeLibraryId, paths, subfolders });
    },
    [activeLibraryId, activeLibrary, runDroppedImport]
  );

  // Real OS-level file drop (Finder, Downloads...) — distinct from the
  // in-app pointer-based drag (a track onto a project) built earlier;
  // this is Tauri's own window-level drag-drop, native drops from
  // outside the webview entirely. dragDropEnabled defaults to true and
  // nothing in this app relies on HTML5 drag-and-drop anymore (the
  // in-app drag uses pointer events specifically to avoid that), so
  // there's no conflict enabling both.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setIsExternalDragActive(true);
        } else if (event.payload.type === "drop") {
          setIsExternalDragActive(false);
          handleExternalFileDrop(event.payload.paths);
        } else if (event.payload.type === "leave") {
          setIsExternalDragActive(false);
        }
      })
      .then((off) => {
        if (cancelled) off();
        else unlisten = off;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handleExternalFileDrop]);

  const handleConfirmExternalDropDestination = useCallback(
    (subfolder: string | null) => {
      if (!externalDropPrompt) return;
      const { libraryId, paths } = externalDropPrompt;
      if (externalDropRememberChoice) {
        rememberedDropSubfolderRef.current = { libraryId, subfolder };
      }
      setExternalDropPrompt(null);
      runDroppedImport(libraryId, paths, subfolder);
    },
    [externalDropPrompt, externalDropRememberChoice, runDroppedImport]
  );

  // Checks both of a library's folders for new sounds: media_root (via
  // refresh_library, unchanged) and, if one is configured, the separate
  // import_root staging folder (via scan_import_folder) — the manual half
  // of the "scan on launch/refresh" auto-import described in the first-run
  // wizard. Promise.allSettled rather than Promise.all since a library
  // without a media_root yet (refresh_library rejects) shouldn't block the
  // import-folder scan from still running, and vice versa.
  const handleRefreshLibrary = useCallback(() => {
    if (!activeLibraryId) return;
    setRefreshStatus("Scanning for new files…");
    Promise.allSettled([
      invoke<ImportFolderResult>("refresh_library", { libraryId: activeLibraryId }),
      invoke<ImportFolderResult>("scan_import_folder", { libraryId: activeLibraryId })
    ]).then(([refreshResult, importResult]) => {
      if (refreshResult.status === "rejected" && importResult.status === "rejected") {
        setRefreshStatus(`Refresh failed: ${String(refreshResult.reason)}`);
        return;
      }
      const importedCount =
        (refreshResult.status === "fulfilled" ? refreshResult.value.imported.length : 0) +
        (importResult.status === "fulfilled" ? importResult.value.imported.length : 0);
      if (importedCount > 0) {
        runJobDrain(activeLibraryId);
      }
      setRefreshStatus(
        importedCount > 0 ? `Found ${importedCount} new sound${importedCount === 1 ? "" : "s"}` : "No new files found"
      );
      refreshAssets(activeLibraryId, searchQuery, activeFilter);
      refreshMaintenance(activeLibraryId);
    });
  }, [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance, runJobDrain]);

  // Sonic Radar "sync": re-run every analysis pass (waveform, tempo/key/
  // vocal, instruments) for the current selection, or the whole library
  // when nothing is selected. Backfills assets whose original jobs already
  // finished before a detector existed.
  // `kinds` omitted re-runs all three passes (waveform, audio analysis,
  // instrument detection) — the blunt "re-analyse everything" hammer.
  // Passing just ["audio_analysis"] (see handleBackfillAudioAnalysis
  // below) re-runs only the Tempo/Key/Pitch/Vocals pass, without also
  // re-queuing every instrument-detection job (an expensive ML pass) or
  // every waveform job just to pick up newer tempo/key/pitch logic.
  const handleResyncSonicRadar = useCallback(
    async (kinds?: string[]) => {
      if (!activeLibraryId || resyncing) return;
      const ids = bulkAssetIds;
      const scopeLabel = kinds ? "audio analysis (Tempo/Key/Pitch/Vocals)" : "every analysis pass";
      if (ids.length === 0) {
        const ok = await confirmDialog(
          `Re-run ${scopeLabel} for the whole library? Every matching sound is re-analysed — this can take a while on a large library.`,
          { title: "Re-analyse library", kind: "warning" }
        );
        if (!ok) return;
      }
      setResyncing(true);
      setRefreshStatus(
        ids.length === 0
          ? "Queuing re-analysis for the library…"
          : `Queuing re-analysis for ${ids.length} sound${ids.length === 1 ? "" : "s"}…`
      );
      invoke<number>("resync_analysis", { libraryId: activeLibraryId, assetIds: ids, kinds })
        .then((queued) => {
          setRefreshStatus(
            queued > 0 ? `Re-analysing — ${queued} job${queued === 1 ? "" : "s"} queued` : "Nothing to re-analyse"
          );
          if (queued > 0) {
            // The backend caches (waveform strips) are about to be
            // regenerated — drop the session memo so the next play re-fetches
            // the fresh version rather than showing the pre-resync shape.
            waveformStripCache.clear();
            runJobDrain(activeLibraryId);
          }
        })
        .catch((error) => setRefreshStatus(`Re-analysis failed: ${String(error)}`))
        .finally(() => setResyncing(false));
    },
    [activeLibraryId, bulkAssetIds, runJobDrain, resyncing]
  );

  // A track can complete its audio-analysis job and still have no key/
  // pitch on record if it was analysed by an older build of the app —
  // key and pitch detection landed after audio analysis did, so most of
  // an existing catalog's "completed" jobs never actually ran that code.
  // This re-runs just that one pass, library-wide, without touching
  // instrument detection or waveform generation.
  const handleBackfillAudioAnalysis = useCallback(() => {
    handleResyncSonicRadar(["audio_analysis"]);
  }, [handleResyncSonicRadar]);

  // Sonic Radar inspector panel: analyze just the selected sound, and only
  // the one job kind its missing row asked for. Tempo/key/pitch/vocals all
  // come out of the same decode+DSP pass (analyze_asset_audio decodes the
  // file once and fills every one of those fields together), so there's no
  // "just tempo" job to run — every row in that group queues the same
  // "audio_analysis" kind. Instrument detection is a genuinely separate
  // model/pass, so it gets its own kind.
  //
  // Outcome tracking polls asset_job_state directly — this asset's own job
  // row — rather than the shared, per-kind job_status/jobProgress counters
  // used earlier: those are library-wide, so a *different* in-flight job of
  // the same kind (another asset, a background drain) could leave this
  // asset's own completion (or failure) unreported. That was the bug behind
  // Pitch/Instruments silently reverting to "Analyze" with no error and no
  // result — the shared tracker sometimes never saw this job at all.
  const handleAnalyzeAssetKind = useCallback(
    async (kind: "audio_analysis" | "instrument_detection") => {
      if (!activeLibraryId || !selectedAssetId || sonicRadarBusyKind) return;
      const assetId = selectedAssetId;
      const requestId = (sonicRadarRequestIdRef.current += 1);
      const stillCurrent = () => sonicRadarRequestIdRef.current === requestId;

      setSonicRadarBusyKind(kind);
      setSonicRadarStatus(
        kind === "audio_analysis" ? "Queuing tempo/key/pitch/vocal analysis…" : "Queuing instrument detection…"
      );

      let queued = 0;
      try {
        queued = await invoke<number>("resync_analysis", {
          libraryId: activeLibraryId,
          assetIds: [assetId],
          kinds: [kind]
        });
      } catch (error) {
        if (stillCurrent()) {
          setSonicRadarStatus(`Couldn't queue analysis: ${String(error)}`);
          setSonicRadarBusyKind(null);
        }
        return;
      }
      if (!stillCurrent()) return;

      if (kind === "audio_analysis" && audioAnalysisPaused) {
        // Queued, but nothing will drive it until the user resumes audio
        // analysis (Maintenance) — don't sit here "Analyzing…" forever
        // waiting for a drain that isn't going to run.
        setSonicRadarStatus(
          queued > 0
            ? "Queued — audio analysis is paused; resume it in Maintenance to run this"
            : "Nothing new to queue"
        );
        setSonicRadarBusyKind(null);
        return;
      }

      setSonicRadarStatus("Analyzing…");
      runJobDrain(activeLibraryId, [kind]); // best-effort kick — harmless no-op if a drain for this kind is already running, since that one will pick our job up too

      type AssetJobState = { state: string; error: string | null };
      let finalState: AssetJobState | null = null;
      // Instrument detection decodes+resamples the whole source file before
      // it ever gets to inference, and this panel processes one asset at a
      // time behind whatever else is already draining (waveform
      // generation's own backlog, in particular) — a multi-minute track
      // under real background load can legitimately take longer than a
      // minute. 120s gives that real headroom without masking an actually
      // stuck job forever.
      const deadline = Date.now() + 120_000;
      while (Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 800));
        if (!stillCurrent()) return;
        try {
          const jobState = await invoke<AssetJobState | null>("asset_job_state", { assetId, kind });
          if (jobState && (jobState.state === "completed" || jobState.state === "failed")) {
            finalState = jobState;
            break;
          }
        } catch {
          // transient — keep polling until the deadline
        }
        if (!stillCurrent()) return;
      }
      if (!stillCurrent()) return;

      if (!finalState) {
        setSonicRadarStatus("Still taking a while — it'll keep running; check back or try again shortly");
        setSonicRadarBusyKind(null);
        return;
      }

      if (finalState.state === "failed") {
        setSonicRadarStatus(`Analysis failed: ${finalState.error ?? "unknown error"}`);
        setSonicRadarBusyKind(null);
        return;
      }

      // Completed — pull the fresh result before declaring success, so the
      // checkmark reflects what the database actually has, never an
      // assumption. Instrument detections live in a separate table
      // (asset_instruments), not on the asset record refreshAssets
      // re-fetches, so that needs its own explicit refetch.
      refreshAssets(activeLibraryId, searchQuery, activeFilter);
      if (kind === "instrument_detection") {
        try {
          const instruments = await invoke<{ name: string; confidence: number }[]>("asset_instruments", {
            assetId
          });
          if (stillCurrent()) setAssetInstruments(instruments);
        } catch {
          // leave whatever's already showing — refreshAssets still ran
        }
      }
      if (stillCurrent()) {
        setSonicRadarStatus("Done — results updated below");
        setSonicRadarBusyKind(null);
      }
    },
    [
      activeLibraryId,
      selectedAssetId,
      runJobDrain,
      audioAnalysisPaused,
      sonicRadarBusyKind,
      refreshAssets,
      searchQuery,
      activeFilter
    ]
  );

  const handleExportSelected = useCallback(
    async (format?: "wav24") => {
      if (!selectedAssetId) return;
      const destination = await openDialog({ directory: true, multiple: false, title: "Choose export destination" });
      if (typeof destination !== "string") return;

      try {
        const destinationPath = await invoke<string>("export_selected_asset", {
          assetId: selectedAssetId,
          destinationFolder: destination,
          format: format ?? null
        });
        setExportStatus(`Exported to ${destinationPath}`);
      } catch (error) {
        setExportStatus(`Export failed: ${String(error)}`);
      }
    },
    [selectedAssetId]
  );

  useEffect(() => {
    if (commandPaletteOpen) {
      commandPaletteInputRef.current?.focus();
    } else {
      setCommandPaletteQuery("");
    }
  }, [commandPaletteOpen]);

  useEffect(() => {
    if (!commandPaletteOpen) return;
    invoke<PaletteCommand[]>("search_commands", { query: commandPaletteQuery })
      .then((results) => {
        setCommandPaletteResults(results);
        setCommandPaletteActiveIndex(0);
      })
      .catch(() => setCommandPaletteResults([]));
  }, [commandPaletteOpen, commandPaletteQuery]);

  const executeCommand = useCallback(
    (commandId: PaletteCommandId) => {
      setCommandPaletteOpen(false);
      switch (commandId) {
        case "Import":
          handleImportFolder();
          break;
        case "ApplyTag":
          document.getElementById("tags-section")?.scrollIntoView({ behavior: "smooth" });
          break;
        case "AddToCollection":
          setEditorWorkflowOpen(true);
          break;
        case "Export":
          handleExportSelected();
          break;
        case "Reveal":
          if (bulkAssetIds.length > 0) handleRevealAssets(bulkAssetIds);
          else setEditorWorkflowOpen(true);
          break;
        case "Convert":
          handleExportSelected("wav24");
          break;
        case "Rescan":
          handleRefreshLibrary();
          break;
        case "OpenSettings":
          setSettingsOpen(true);
          break;
        case "RunMaintenance":
          document.getElementById("maintenance")?.scrollIntoView({ behavior: "smooth" });
          break;
        default:
          break;
      }
    },
    [handleImportFolder, handleExportSelected, bulkAssetIds, handleRevealAssets, handleRefreshLibrary]
  );

  const handleBulkFavorite = useCallback(() => {
    if (bulkAssetIds.length === 0 || !activeLibraryId) return;
    Promise.all(bulkAssetIds.map((assetId) => invoke("set_favorite", { assetId, favorite: true })))
      .then(() => refreshAssets(activeLibraryId, searchQuery, activeFilter))
      .catch(() => {});
  }, [bulkAssetIds, activeLibraryId, searchQuery, activeFilter, refreshAssets]);

  const handleBulkTrash = useCallback(() => {
    if (bulkAssetIds.length === 0 || !activeLibraryId) return;
    Promise.all(bulkAssetIds.map((assetId) => invoke("move_to_trash", { assetId, reason: "manual" })))
      .then(() => {
        setSelectedAssetId(null);
        refreshAssets(activeLibraryId, searchQuery, activeFilter);
        refreshTrashItems(activeLibraryId);
        refreshMaintenance(activeLibraryId);
      })
      .catch(() => {});
  }, [bulkAssetIds, activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshTrashItems, refreshMaintenance]);

  const handleBulkExport = useCallback(async () => {
    if (bulkAssetIds.length === 0) return;
    const destination = await openDialog({ directory: true, multiple: false, title: "Choose export destination" });
    if (typeof destination !== "string") return;

    try {
      await Promise.all(
        bulkAssetIds.map((assetId) =>
          invoke<string>("export_selected_asset", {
            assetId,
            destinationFolder: destination,
            format: null
          })
        )
      );
      setExportStatus(`Exported ${bulkAssetIds.length} sound${bulkAssetIds.length === 1 ? "" : "s"}`);
    } catch (error) {
      setExportStatus(`Export failed: ${String(error)}`);
    }
  }, [bulkAssetIds]);

  const handleExportLicenseReport = useCallback(async () => {
    if (typeof activeFilter !== "object" || !("project" in activeFilter)) {
      setExportStatus("Select a project in the sidebar first, then use Library → Export License Report.");
      return;
    }
    const destination = await saveDialog({
      defaultPath: "license-report.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }]
    });
    if (typeof destination !== "string") return;

    try {
      await invoke("export_project_license_report", {
        projectId: activeFilter.project,
        destinationPath: destination
      });
      setExportStatus(`License report exported to ${destination}`);
    } catch (error) {
      setExportStatus(`License report export failed: ${String(error)}`);
    }
  }, [activeFilter]);

  useEffect(() => {
    const unlistenLicense = listen("menu-export-license-report", () => handleExportLicenseReport());
    const unlistenShortcuts = listen("menu-keyboard-shortcuts", () => setShortcutsOpen((previous) => !previous));
    return () => {
      unlistenLicense.then((dispose) => dispose());
      unlistenShortcuts.then((dispose) => dispose());
    };
  }, [handleExportLicenseReport]);

  // File > Open Library… — the dialog itself (and the follow-up
  // open_library_file invoke) lives in handleOpenLibraryFile already, since
  // it's the same "browse for a .darkwave file" flow the first-run screen
  // uses; File > Last Open needs no round trip here at all — it's resolved
  // entirely on the Rust side (see the "open-last-library" menu handler)
  // and arrives as the same library-file-opened event a Finder double-click
  // does.
  useEffect(() => {
    const unlistenMenuOpenLibrary = listen("menu-open-library", () => handleOpenLibraryFile());
    return () => {
      unlistenMenuOpenLibrary.then((dispose) => dispose());
    };
  }, [handleOpenLibraryFile]);

  // The Rust-side standing worker (apps/desktop/src-tauri) ticks roughly
  // every 20s, requeuing retryable failed jobs and emitting this event —
  // this is what makes job processing actually run continuously in the
  // background rather than only right after Import/Refresh.
  useEffect(() => {
    if (!activeLibraryId) return;
    const unlistenBackgroundTick = listen("background-tick", () => {
      runJobDrain(activeLibraryId);
      refreshResolveSyncStatuses(activeLibraryId);
    });
    return () => {
      unlistenBackgroundTick.then((dispose) => dispose());
    };
  }, [activeLibraryId, runJobDrain, refreshResolveSyncStatuses]);

  // Refresh on open rather than only relying on the next background-tick
  // (up to ~20s away) — a project just created/edited should show its sync
  // state promptly once the user actually opens the panel to look at it.
  useEffect(() => {
    if (editorWorkflowOpen && activeLibraryId) refreshResolveSyncStatuses(activeLibraryId);
  }, [editorWorkflowOpen, activeLibraryId, refreshResolveSyncStatuses]);

  // A `.darkwave` library file opened from outside this window's own
  // Open Library/recent-libraries UI — a Finder double-click (macOS), or a
  // second launch attempt Windows forwards here instead of opening a
  // duplicate window (see tauri_plugin_single_instance/RunEvent::Opened in
  // the Rust backend). Reuses the same reset path New Setup/Open Library
  // finish with, since it's the identical "a different library file is now
  // the one open" transition either way.
  useEffect(() => {
    const unlistenLibraryFileOpened = listen<{ library: LibraryRecord; path: string }>(
      "library-file-opened",
      (event) => {
        finishOnboardingWithLibrary(event.payload.library, event.payload.path);
      }
    );
    return () => {
      unlistenLibraryFileOpened.then((dispose) => dispose());
    };
  }, [finishOnboardingWithLibrary]);

  // process_audio_analysis_jobs/process_waveform_jobs/process_instrument_jobs
  // each claim and fully process a batch per invoke — real per-file work,
  // easily minutes for a batch — so without this, a progress bar only
  // updates once the whole batch's invoke resolves and sits frozen the
  // entire time despite real work happening. These events (emitted per job,
  // not per batch) are what let each bar move continuously instead, and
  // what let the Background Activity panel show which file is currently
  // being worked on under each kind.
  useEffect(() => {
    type JobFileProgressEvent = { succeeded: boolean; asset_name: string };
    const subscriptions = [
      ["audio-analysis-progress", "audio_analysis"],
      ["waveform-progress", "waveform_generation"],
      ["instrument-detection-progress", "instrument_detection"]
    ].map(([eventName, kind]) =>
      listen<JobFileProgressEvent>(eventName, (event) => {
        setJobProgress((previous) =>
          previous.map((entry) =>
            entry.kind === kind
              ? {
                  ...entry,
                  pending: Math.max(0, entry.pending - 1),
                  failed: event.payload.succeeded ? entry.failed : entry.failed + 1,
                  currentFile: event.payload.asset_name
                }
              : entry
          )
        );
      })
    );
    return () => {
      subscriptions.forEach((subscription) => subscription.then((dispose) => dispose()));
    };
  }, []);

  const handleToggleReducedMotion = useCallback(() => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, reduced_motion: !previous.reduced_motion };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

  const handleToggleReducedTransparency = useCallback(() => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, reduced_transparency: !previous.reduced_transparency };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

  const handleToggleSleepPrevention = useCallback(() => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = {
        ...previous,
        prevent_sleep_during_analysis: !previous.prevent_sleep_during_analysis
      };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

  const handleSetTheme = useCallback((theme: AppPreferences["theme"]) => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, theme };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

  // Settings' "Watched Folders" section — add/remove/re-tag folders for the
  // active library after onboarding, the same underlying `folders` rows the
  // New Setup wizard's advanced step writes (see add_folder/remove_folder/
  // set_folder_role in the Rust backend). The standing background worker
  // picks up any change within one tick (~20s), no restart needed.
  const handleAddWatchedFolder = useCallback(async () => {
    if (!activeLibraryId) return;
    const folder = await openDialog({ directory: true, multiple: false, title: "Choose a folder to watch" });
    if (typeof folder !== "string") return;
    await invoke("add_folder", { libraryId: activeLibraryId, path: folder, role: null, kind: "watched" }).catch(
      () => {}
    );
    refreshLibraryFolders(activeLibraryId);
  }, [activeLibraryId, refreshLibraryFolders]);

  const handleSetWatchedFolderRole = useCallback(
    async (folderId: string, role: string) => {
      if (!activeLibraryId) return;
      await invoke("set_folder_role", { folderId, role: role.trim() ? role : null }).catch(() => {});
      refreshLibraryFolders(activeLibraryId);
    },
    [activeLibraryId, refreshLibraryFolders]
  );

  const handleRemoveWatchedFolder = useCallback(
    async (folderId: string) => {
      if (!activeLibraryId) return;
      await invoke("remove_folder", { folderId }).catch(() => {});
      refreshLibraryFolders(activeLibraryId);
    },
    [activeLibraryId, refreshLibraryFolders]
  );

  useEffect(() => {
    document.documentElement.classList.toggle("reduced-motion", preferences?.reduced_motion ?? false);
    document.documentElement.classList.toggle("reduced-transparency", preferences?.reduced_transparency ?? false);
  }, [preferences]);

  useEffect(() => {
    const theme = preferences?.theme ?? "Dark";
    const media = window.matchMedia("(prefers-color-scheme: light)");

    function applyResolvedTheme() {
      const resolved = theme === "System" ? (media.matches ? "light" : "dark") : theme.toLowerCase();
      document.documentElement.dataset.theme = resolved;
    }

    applyResolvedTheme();
    if (theme !== "System") return;
    media.addEventListener("change", applyResolvedTheme);
    return () => media.removeEventListener("change", applyResolvedTheme);
  }, [preferences?.theme]);

  useEffect(() => {
    function isTypingTarget(target: EventTarget | null) {
      return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
    }

    function acceleratorFor(event: KeyboardEvent): string {
      // Every check here reads metaKey OR ctrlKey, never one alone, so the
      // same accelerator string fires from Cmd on macOS and Ctrl on Windows.
      const mod = event.metaKey || event.ctrlKey;
      // event.key alone can't tell a numpad digit from a top-row one — with
      // NumLock on both report key "1", and with NumLock off the numpad
      // reports a navigation key ("End", "Home", ...) instead of a digit at
      // all. event.code encodes the physical key regardless of NumLock, so
      // numpad digits get their own "Numpad1".."Numpad0" accelerator,
      // bindable separately from (but alongside) the top-row digit.
      const isNumpadDigit = /^Numpad[0-9]$/.test(event.code);
      const key = isNumpadDigit
        ? event.code
        : event.key === " "
          ? "Space"
          : event.key.length === 1
            ? event.key.toUpperCase()
            : event.key;
      const parts: string[] = [];
      if (mod) parts.push("Mod");
      if (event.shiftKey) parts.push("Shift");
      parts.push(key);
      return parts.join("+");
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (isTypingTarget(event.target)) return;

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        if (browserState) {
          invoke<BrowserState>("apply_browser_command", {
            browserState,
            command: "SelectAllVisible"
          })
            .then(setBrowserState)
            .catch(() => {});
        }
        return;
      }

      const binding = preferences?.shortcuts.bindings.find(
        (candidate) => candidate.accelerator === acceleratorFor(event)
      );
      if (!binding) {
        // No configured shortcut claims this key. If it's a plain printable
        // character — no modifier (a real shortcut would have one, or would
        // have matched above), not the space bar (reserved for playback,
        // same as most media apps), and no modal currently open to capture
        // it instead — redirect it into the search box instead of letting
        // it silently do nothing. Matches type-ahead search in Finder/
        // Gmail/etc.: no need to click into the search field first just to
        // start typing a query.
        if (
          !event.metaKey &&
          !event.ctrlKey &&
          !event.altKey &&
          event.key.length === 1 &&
          event.key !== " " &&
          !document.querySelector(".modal-overlay")
        ) {
          event.preventDefault();
          // Replaces, not appends: this branch only ever runs while the
          // search input is NOT focused (isTypingTarget already returned
          // above otherwise), so it only fires once per typing burst — the
          // moment focus lands on the input below, every further keystroke
          // goes through its own onChange and appends normally there.
          // Appending here instead used to mean a search from a previous,
          // already-finished lookup was still sitting in the box, and a
          // brand new type-to-search burst kept building on top of it
          // instead of starting clean.
          setSearchQuery(event.key);
          const input = searchInputRef.current;
          if (input) {
            input.focus();
            // Wait a frame so the controlled input has actually re-rendered
            // with the new character before moving the caret — doing it
            // synchronously would still see the pre-update value length.
            requestAnimationFrame(() => {
              const end = input.value.length;
              input.setSelectionRange(end, end);
            });
          }
        }
        return;
      }

      switch (binding.command) {
        case "TogglePlayback":
          event.preventDefault();
          togglePlayback();
          break;
        case "PreviewSelected":
          if (selectedAsset) loadAssetForPlayback(selectedAsset, true);
          break;
        case "NextAsset":
          event.preventDefault();
          playRelative(1);
          break;
        case "PreviousAsset":
          event.preventDefault();
          playRelative(-1);
          break;
        case "ToggleFavorite":
          if (selectedAsset) handleToggleFavorite(selectedAsset);
          break;
        case "Import":
          event.preventDefault();
          handleImportFolder();
          break;
        case "ExportSelected":
          event.preventDefault();
          handleExportSelected();
          break;
        case "CommandPalette":
          event.preventDefault();
          setCommandPaletteOpen((previous) => !previous);
          break;
        case "ToggleLoop":
          event.preventDefault();
          setLooping((previous) => !previous);
          break;
        case "CopyPath":
          event.preventDefault();
          if (bulkAssetIds.length > 0) handleCopyAssetPaths(bulkAssetIds);
          break;
        case "ToggleReviewed":
          // preventDefault unconditionally, not just when an asset is
          // selected — Mod+R is the browser/webview's native "reload"
          // accelerator, and letting that through with nothing selected
          // would reload the whole app instead of doing nothing.
          event.preventDefault();
          if (selectedAsset) handleToggleReviewed(selectedAsset);
          break;
        case "ClassifySoundtrack":
        case "ClassifyVoiceover":
        case "ClassifySoundEffect":
        case "ClassifyFoley":
        case "ClassifyAmbience":
        case "ClassifyOther": {
          const mediaType = mediaTypeOptions.find(
            (option) => MEDIA_TYPE_CLASSIFY_COMMAND[option.value] === binding.command
          )?.value;
          if (selectedAsset && mediaType) {
            event.preventDefault();
            handleSetMediaType(selectedAsset, mediaType);
          }
          break;
        }
        default:
          break;
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    preferences,
    togglePlayback,
    selectedAsset,
    loadAssetForPlayback,
    playRelative,
    handleToggleFavorite,
    handleToggleReviewed,
    handleSetMediaType,
    handleImportFolder,
    bulkAssetIds,
    handleCopyAssetPaths,
    handleExportSelected,
    browserState
  ]);

  // Hard paywall: only reachable in the direct-dist build (the MAS build's
  // license.rs stub always reports active:true, so licenseMode is "full"
  // before this ever renders). Gates everything, including library
  // creation, which is why this early-return sits ahead of it.
  if (licenseMode === "trial_expired" || licenseMode === "inactive") {
    return (
      <main className="shell setup-shell">
        <section className="setup-card" aria-label="Activate Darkwave">
          <div className="brand">Darkwave</div>
          <h1>{licenseMode === "trial_expired" ? "Your trial has ended" : "Activate Darkwave"}</h1>
          <p>
            {licenseMode === "trial_expired"
              ? "Enter your license key to keep using Darkwave."
              : "Enter the license key from your purchase email to activate this copy of Darkwave."}
          </p>
          <label className="setup-field">
            <span>License key</span>
            <input
              autoFocus
              value={licenseKeyInput}
              onChange={(event) => setLicenseKeyInput(event.target.value)}
              placeholder="DW-XXXXXXXX-XXXXXXXX-XXXXXXXX"
            />
          </label>
          <label className="setup-field">
            <span>Email used at purchase</span>
            <input
              value={licenseEmailInput}
              onChange={(event) => setLicenseEmailInput(event.target.value)}
              placeholder="you@example.com"
              onKeyDown={(event) => {
                if (event.key === "Enter" && licenseKeyInput.trim() && licenseEmailInput.trim()) {
                  handleActivateLicense();
                }
              }}
            />
          </label>
          {licenseFormError ? <p className="setup-error">{licenseFormError}</p> : null}
          <button
            className="primary-action"
            type="button"
            onClick={handleActivateLicense}
            disabled={!licenseKeyInput.trim() || !licenseEmailInput.trim() || licenseFormBusy}
          >
            {licenseFormBusy ? "Activating…" : "Activate"}
          </button>
          <button
            className="text-button"
            type="button"
            onClick={handleRecoverLicenseKey}
            disabled={!licenseEmailInput.trim() || licenseFormBusy}
          >
            Resend my license key
          </button>
        </section>
      </main>
    );
  }

  if (librariesLoaded && activeLibraryFilePath === null && libraries.length > 0 && !migrationDismissed) {
    const allMigrated = libraries.every((library) => migrationStatus[library.id] === "done");
    return (
      <main className="shell setup-shell">
        <section className="setup-card" aria-label="Migrate to Darkwave library files">
          <div className="brand">Darkwave</div>
          <h1>Your libraries are moving to library files</h1>
          <p>
            Darkwave now keeps each library as its own <code>.darkwave</code> file you can save, move, and reopen
            like any other document — the way a video editor's project file stays separate from its footage. Save
            each library below to give it a home.
          </p>
          <ul className="setup-recent-list">
            {libraries.map((library) => {
              const status = migrationStatus[library.id] ?? "pending";
              return (
                <li key={library.id} className="setup-migration-row">
                  <span>{library.name}</span>
                  {status === "done" ? (
                    <span className="settings-hint">Migrated</span>
                  ) : (
                    <button
                      type="button"
                      onClick={() => handleMigrateLibrary(library)}
                      disabled={status === "migrating"}
                    >
                      {status === "migrating" ? "Saving…" : status === "failed" ? "Retry…" : "Choose location…"}
                    </button>
                  )}
                  {migrationErrors[library.id] ? (
                    <p className="settings-hint">{migrationErrors[library.id]}</p>
                  ) : null}
                </li>
              );
            })}
          </ul>
          <div className="setup-field-row">
            <button className="text-button" type="button" onClick={handleDismissMigration} disabled={migrationBusy}>
              Remind me later
            </button>
            <button
              className="primary-action"
              type="button"
              onClick={handleFinishMigration}
              disabled={!allMigrated || migrationBusy}
            >
              Continue
            </button>
          </div>
          {onboardingError ? <p className="settings-hint">{onboardingError}</p> : null}
        </section>
      </main>
    );
  }

  if (librariesLoaded && (libraries.length === 0 || onboardingLibrary)) {
    return (
      <main className="shell setup-shell">
        <section className="setup-card" aria-label="Set up Darkwave">
          <div className="brand">Darkwave</div>
          {onboardingStep > 0 ? <p className="settings-hint">Step {onboardingStep} of 2</p> : null}
          {onboardingStep === 0 ? (
            <>
              <h1>Welcome to Darkwave</h1>
              <p>Start a new library, or open a library file you already have.</p>
              <div className="setup-field-row">
                <button className="primary-action" type="button" onClick={handleBeginNewSetup}>
                  New Setup
                </button>
                <button type="button" onClick={handleOpenLibraryFile} disabled={onboardingBusy}>
                  Open Library…
                </button>
              </div>
              {recentLibraryFiles.length > 0 ? (
                <div className="setup-field">
                  <span>Recent libraries</span>
                  <ul className="setup-recent-list">
                    {recentLibraryFiles.map((entry) => (
                      <li key={entry.path}>
                        <button
                          type="button"
                          className="text-button"
                          onClick={() => handleOpenRecentLibraryFile(entry.path)}
                          disabled={onboardingBusy}
                        >
                          {entry.name}
                          <span className="settings-hint"> — {entry.path}</span>
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
            </>
          ) : onboardingStep === 1 ? (
            <>
              <h1>Name your library</h1>
              <p>
                Holds your tags, analysis, and settings — separate from where your sound files live, which you'll
                set up next.
              </p>
              <label className="setup-field">
                <span>Library name</span>
                <input
                  autoFocus
                  value={libraryName}
                  onChange={(event) => setLibraryName(event.target.value)}
                  placeholder="Home Studio"
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && libraryName.trim() && !onboardingBusy)
                      handleOnboardingCreateLibraryFile();
                  }}
                />
              </label>
              <button
                className="primary-action"
                type="button"
                onClick={handleOnboardingCreateLibraryFile}
                disabled={!libraryName.trim() || onboardingBusy}
              >
                Choose save location…
              </button>
              {onboardingNetworkPathWarning ? <p className="settings-hint">{onboardingNetworkPathWarning}</p> : null}
            </>
          ) : (
            <>
              <h1>Where do your actual sound files live?</h1>
              <p>A plain folder on disk or a NAS — separate from the library file you just saved.</p>
              <div className="setup-mode-toggle" role="radiogroup" aria-label="Sound files layout">
                <button
                  type="button"
                  role="radio"
                  aria-checked={onboardingFolderMode === "single"}
                  className={onboardingFolderMode === "single" ? "setup-mode-option active" : "setup-mode-option"}
                  onClick={() => setOnboardingFolderMode("single")}
                >
                  <strong>One main folder</strong>
                  <span>Everything in one place.</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={onboardingFolderMode === "advanced"}
                  className={onboardingFolderMode === "advanced" ? "setup-mode-option active" : "setup-mode-option"}
                  onClick={() => setOnboardingFolderMode("advanced")}
                >
                  <strong>Already organized</strong>
                  <span>Soundtracks, SFX, Foley, etc.</span>
                </button>
              </div>
              {onboardingFolderMode === "single" ? (
                <label className="setup-field">
                  <span>Sound files folder</span>
                  <div className="setup-field-row">
                    <input
                      autoFocus
                      placeholder="/Volumes/Sound Library"
                      value={onboardingMediaRoot}
                      onChange={(event) => setOnboardingMediaRoot(event.target.value)}
                    />
                    <button type="button" onClick={handleOnboardingChooseMediaRoot}>
                      Browse
                    </button>
                  </div>
                </label>
              ) : (
                <div className="setup-field">
                  <span>Add each folder and what it holds</span>
                  {onboardingAdvancedFolders.length > 0 ? (
                    <ul className="setup-folder-list">
                      {onboardingAdvancedFolders.map((folder) => (
                        <li key={folder.id} className="setup-folder-row">
                          <select
                            value={folder.role}
                            onChange={(event) => handleSetOnboardingFolderRole(folder.id, event.target.value)}
                          >
                            {FOLDER_ROLE_OPTIONS.map((option) => (
                              <option key={option.value} value={option.value}>
                                {option.label}
                              </option>
                            ))}
                          </select>
                          <span className="setup-folder-path" title={folder.path || undefined}>
                            {folder.path || "No folder chosen"}
                          </span>
                          <button type="button" onClick={() => handleBrowseOnboardingFolderRow(folder.id)}>
                            Browse
                          </button>
                          <button
                            type="button"
                            className="text-button"
                            onClick={() => handleRemoveOnboardingFolderRow(folder.id)}
                          >
                            Remove
                          </button>
                        </li>
                      ))}
                    </ul>
                  ) : null}
                  <button
                    type="button"
                    className="text-button setup-add-folder-button"
                    onClick={handleAddOnboardingFolderRow}
                  >
                    + Add folder
                  </button>
                </div>
              )}
              <button
                className="primary-action"
                type="button"
                onClick={handleOnboardingConfirmMediaRoot}
                disabled={
                  onboardingBusy ||
                  (onboardingFolderMode === "single"
                    ? !onboardingMediaRoot.trim()
                    : !onboardingAdvancedFolders.some((folder) => folder.path.trim()))
                }
              >
                Finish
              </button>
            </>
          )}
          {onboardingError ? <p className="settings-hint">{onboardingError}</p> : null}
        </section>
      </main>
    );
  }

  // Two distinct Background Activity states: "preparing" is indeterminate
  // work with no live progress to show yet (scanning a folder, hashing
  // files, walking the media root) — the blue sweep says "working" without
  // implying a percentage that doesn't exist. "busy" is real job-queue
  // draining with an actual pending count, which is what the steady green
  // LED already meant. Collapsing import/refresh status into this one
  // indicator (instead of also showing a separate topbar chip) means
  // there's one place to check for anything happening in the background.
  const activityPreparing = importStatus === "Importing…" || refreshStatus === "Scanning for new files…";
  const activityBusy = jobProgress.length > 0;
  const waveformActiveIndex = peaks && duration > 0 ? Math.floor((currentTime / duration) * peaks.length) : -1;
  const drTargetAssetId = playingAssetId ?? selectedAssetId;
  const drTargetProject = collections.find((project) => project.id === lastExportProjectId) ?? null;
  const playerMood = classifyPlayerMood(selectedAsset, appliedTags, selectedAsset?.vocal_ratio ?? null);
  // Default/unclassified mood keeps the brand orange, but with a real glow
  // value so the playing-state halo matches every other mood (they all set
  // one) instead of falling back to transparent.
  const playerTheme = playerMood
    ? playerMoodTheme[playerMood]
    : { from: "#ff7d3f", to: "#f24b12", glow: "rgba(255, 92, 0, 0.42)" };
  const playerMoodStyle = {
    "--player-accent-from": playerTheme.from,
    "--player-accent-to": playerTheme.to,
    "--player-glow-color": playerTheme.glow
  } as CSSProperties;

  return (
    <main
      className={[
        "shell",
        sidebarCollapsed ? "sidebar-collapsed" : "",
        inspectorCollapsed ? "inspector-collapsed" : "",
        dragPreview ? "dragging-asset" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      style={playerMoodStyle}
    >
      <audio
        ref={audioRef}
        loop={looping}
        onPlay={() => setIsPlaying(true)}
        onPause={() => setIsPlaying(false)}
        onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => setDuration(event.currentTarget.duration)}
        onEnded={() => setIsPlaying(false)}
      />
      <button
        type="button"
        className="panel-handle panel-handle-left"
        style={{ left: sidebarCollapsed ? "8px" : "260px" }}
        aria-label={sidebarCollapsed ? "Show library panel" : "Hide library panel"}
        onClick={() => setSidebarCollapsed((previous) => !previous)}
      >
        {sidebarCollapsed ? <ChevronRight size={13} /> : <ChevronLeft size={13} />}
      </button>
      <button
        type="button"
        className="panel-handle panel-handle-right"
        style={{ right: inspectorCollapsed ? "8px" : "312px" }}
        aria-label={inspectorCollapsed ? "Show inspector panel" : "Hide inspector panel"}
        onClick={() => setInspectorCollapsed((previous) => !previous)}
      >
        {inspectorCollapsed ? <ChevronLeft size={13} /> : <ChevronRight size={13} />}
      </button>
      <aside className={sidebarCollapsed ? "sidebar collapsed" : "sidebar"} aria-label="Library">
        <div className="panel-body">
          <div className="library-select-row">
            {libraries.length > 1 ? (
              <select
                className="library-select"
                value={activeLibraryId ?? ""}
                onChange={(event) => setActiveLibraryId(event.target.value)}
              >
                {libraries.map((library) => (
                  <option key={library.id} value={library.id}>
                    {library.name}
                  </option>
                ))}
              </select>
            ) : (
              <div className="library-select-static" title={activeLibrary?.media_root}>
                {activeLibrary?.name ?? "Library"}
              </div>
            )}
            <button
              type="button"
              className="icon-button"
              aria-label="Create a new library"
              title="Create a new library"
              onClick={() => setCreateLibraryModalOpen(true)}
            >
              <Plus size={16} />
            </button>
          </div>
          <button
            className={activeFilter === "all" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
            onClick={() => setActiveFilter("all")}
            title="All sounds in the library"
            aria-label="All sounds in the library"
          >
            <Library size={13} />
            All Sounds
          </button>

          <div className="nav-item-row">
            <button
              className={activeFilter === "favorites" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
              onClick={() => setActiveFilter("favorites")}
            >
              <Star size={13} />
              Favorites
            </button>
            <button
              type="button"
              className="nav-heading-add"
              aria-label={favoritesCategoriesOpen ? "Collapse Favorites categories" : "Expand Favorites categories"}
              onClick={() => setFavoritesCategoriesOpen((previous) => !previous)}
            >
              {favoritesCategoriesOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </button>
          </div>
          {favoritesCategoriesOpen ? (
            <>
              <button
                className={
                  typeof activeFilter === "object" && "favoritesCategory" in activeFilter && activeFilter.favoritesCategory === "music"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ favoritesCategory: "music" })}
              >
                <Music2 size={12} />
                Soundtracks
              </button>
              <button
                className={
                  typeof activeFilter === "object" && "favoritesCategory" in activeFilter && activeFilter.favoritesCategory === "voice"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ favoritesCategory: "voice" })}
              >
                <Mic size={12} />
                Voice
              </button>
              <button
                className={
                  typeof activeFilter === "object" &&
                  "favoritesCategory" in activeFilter &&
                  activeFilter.favoritesCategory === "instrumental"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ favoritesCategory: "instrumental" })}
              >
                <MicOff size={12} />
                No Voice
              </button>
              <button
                className={
                  typeof activeFilter === "object" &&
                  "favoritesCategory" in activeFilter &&
                  activeFilter.favoritesCategory === "sound_effect"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ favoritesCategory: "sound_effect" })}
              >
                <Waves size={12} />
                Sound FX
              </button>
            </>
          ) : null}

          <div className="nav-item-row">
            <button
              className={activeFilter === "unreviewed" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
              onClick={() => setActiveFilter("unreviewed")}
            >
              <Eye size={13} />
              Unreviewed
            </button>
            <button
              type="button"
              className="nav-heading-add"
              aria-label={unreviewedCategoriesOpen ? "Collapse Unreviewed categories" : "Expand Unreviewed categories"}
              onClick={() => setUnreviewedCategoriesOpen((previous) => !previous)}
            >
              {unreviewedCategoriesOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </button>
          </div>
          {unreviewedCategoriesOpen ? (
            <>
              <button
                className={
                  typeof activeFilter === "object" && "unreviewedCategory" in activeFilter && activeFilter.unreviewedCategory === "music"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ unreviewedCategory: "music" })}
              >
                <Music2 size={12} />
                Soundtracks
              </button>
              <button
                className={
                  typeof activeFilter === "object" && "unreviewedCategory" in activeFilter && activeFilter.unreviewedCategory === "voice"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ unreviewedCategory: "voice" })}
              >
                <Mic size={12} />
                Voice
              </button>
              <button
                className={
                  typeof activeFilter === "object" &&
                  "unreviewedCategory" in activeFilter &&
                  activeFilter.unreviewedCategory === "instrumental"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ unreviewedCategory: "instrumental" })}
              >
                <MicOff size={12} />
                No Voice
              </button>
              <button
                className={
                  typeof activeFilter === "object" &&
                  "unreviewedCategory" in activeFilter &&
                  activeFilter.unreviewedCategory === "sound_effect"
                    ? "nav-subitem sidebar-styled-item active"
                    : "nav-subitem sidebar-styled-item"
                }
                onClick={() => setActiveFilter({ unreviewedCategory: "sound_effect" })}
              >
                <Waves size={12} />
                Sound FX
              </button>
            </>
          ) : null}

          <button
            className={activeFilter === "needs_review" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
            onClick={() => setActiveFilter("needs_review")}
          >
            <Flag size={13} />
            Needs Review
          </button>

          <div className="nav-divider" role="separator" />

          <div className="nav-heading-row">
            <span className="nav-heading-lg radar-heading" onClick={() => toggleSection("sidebar-sonic-radar")}>
              <Activity size={14} />
              Sonic Radar
            </span>
            <div style={{ position: "relative" }}>
              <button
                ref={radarSyncButtonRef}
                type="button"
                className="nav-heading-add radar-sync"
                disabled={resyncing || !activeLibraryId}
                aria-label="Re-analyse options"
                title={resyncing ? "Queuing re-analysis…" : "Re-analyse options"}
                onClick={(event) => {
                  event.stopPropagation();
                  if (!radarSyncMenuOpen && radarSyncButtonRef.current) {
                    const rect = radarSyncButtonRef.current.getBoundingClientRect();
                    // Clamped so the menu (up to 340px, see .radar-sync-menu)
                    // can't be positioned partway off the right edge of a
                    // narrow window — the button sits near the sidebar's
                    // own right edge, so left-aligning on it unclamped runs
                    // out of room on anything but a wide window.
                    const left = Math.min(rect.left, window.innerWidth - 340 - 16);
                    setRadarSyncMenuPosition({ top: rect.bottom + 6, left: Math.max(16, left) });
                  }
                  setRadarSyncMenuOpen((previous) => !previous);
                }}
              >
                <RefreshCw size={13} className={resyncing ? "spin" : undefined} />
              </button>
              {/* Portaled to document.body rather than left in place like
                  .filter-menu's own position:fixed trick: that trick only
                  escapes an ancestor's overflow-x-via-overflow-y-auto
                  inheritance, not this one — .sidebar has its own
                  backdrop-filter (the glass panel look), and *any* filter
                  or transform on an ancestor makes it the containing block
                  for position:fixed descendants too, right back inside
                  .sidebar's overflow:hidden. That's what was cropping this
                  menu down to a sliver. A real portal has no such
                  ancestor at all. */}
              {createPortal(
                <AnimatePresence>
                  {radarSyncMenuOpen && radarSyncMenuPosition ? (
                    <motion.div
                      className="modal-card filter-menu radar-sync-menu"
                      style={{ top: radarSyncMenuPosition.top, left: radarSyncMenuPosition.left }}
                      initial={{ opacity: 0, scale: 0.95, y: -6 }}
                      animate={{ opacity: 1, scale: 1, y: 0 }}
                      exit={{ opacity: 0, scale: 0.95, y: -6 }}
                      transition={{ duration: 0.16, ease: [0.4, 0, 0.2, 1] }}
                    >
                      <button
                        type="button"
                        className="radar-sync-menu-item"
                        disabled={resyncing || !activeLibraryId}
                        onClick={() => {
                          setRadarSyncMenuOpen(false);
                          handleResyncSonicRadar();
                        }}
                      >
                        <span className="radar-sync-menu-item-icon">
                          <RefreshCw size={14} />
                        </span>
                        <span className="radar-sync-menu-item-body">
                          <span className="radar-sync-menu-item-title">Re-analyse everything</span>
                          <span className="radar-sync-menu-item-desc">
                            {bulkAssetIds.length > 0
                              ? `Waveform, analysis, and instruments for ${bulkAssetIds.length} selected sound${bulkAssetIds.length === 1 ? "" : "s"}.`
                              : "Waveform, analysis, and instruments for the whole library."}
                          </span>
                        </span>
                      </button>
                      {/* A track's audio-analysis job can show "completed" and
                          still have no Key/Pitch on record — that pass was
                          added after a lot of this catalog was already
                          analysed, so most existing "completed" jobs never
                          actually ran the newer detection. This backfills
                          just that (Tempo/Key/Pitch/Vocals) for the whole
                          library without re-touching instrument detection or
                          waveform generation, unlike the option above. */}
                      <button
                        type="button"
                        className="radar-sync-menu-item"
                        disabled={resyncing || !activeLibraryId || bulkAssetIds.length > 0}
                        title={bulkAssetIds.length > 0 ? "Clear the selection first — this one is whole-library only" : undefined}
                        onClick={() => {
                          setRadarSyncMenuOpen(false);
                          handleBackfillAudioAnalysis();
                        }}
                      >
                        <span className="radar-sync-menu-item-icon">
                          <Sparkles size={14} />
                        </span>
                        <span className="radar-sync-menu-item-body">
                          <span className="radar-sync-menu-item-title">Backfill Tempo/Key/Pitch/Vocals</span>
                          <span className="radar-sync-menu-item-desc">
                            Whole library only. Backfills tracks analysed before key/pitch detection existed.
                          </span>
                        </span>
                      </button>
                    </motion.div>
                  ) : null}
                </AnimatePresence>,
                document.body
              )}
            </div>
            <button
              type="button"
              className="nav-heading-add"
              aria-label={collapsedSections.has("sidebar-sonic-radar") ? "Expand Sonic Radar" : "Collapse Sonic Radar"}
              onClick={() => toggleSection("sidebar-sonic-radar")}
            >
              {collapsedSections.has("sidebar-sonic-radar") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            </button>
          </div>
          {collapsedSections.has("sidebar-sonic-radar") ? null : (
            <>
              <button
                className={activeFilter === "has_vocals" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("has_vocals")}
              >
                <Mic size={13} />
                Has Vocals
              </button>
              <button
                className={activeFilter === "instrumental" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("instrumental")}
              >
                <MicOff size={13} />
                Instrumental Only
              </button>
              <button
                className={activeFilter === "has_tempo" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("has_tempo")}
                title="Sounds with an estimated tempo (BPM)"
              >
                <Gauge size={13} />
                Detected Tempo
              </button>
              <button
                className={activeFilter === "has_key" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("has_key")}
                title="Sounds with a detected musical key (e.g. A minor) — Krumhansl-Schmuckler analysis over the whole clip, works on polyphonic music"
              >
                <KeyRound size={13} />
                Detected Key
              </button>
              <button
                className={activeFilter === "has_pitch" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("has_pitch")}
                title="Sounds with a detected monophonic pitch — best for single-source SFX, ambience, and drones, not full mixes"
              >
                <Music size={13} />
                Detected Pitch
              </button>
              <button
                className={instrumentFilter ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter({ instrumentPage: true, instruments: [] })}
                title="Detected instruments — browse the library by instrument (piano, guitar, drums, strings…), pick one or combine several"
              >
                <Piano size={13} />
                Instrument Detection
              </button>
            </>
          )}

          <div className="nav-heading-row">
            <span className="nav-heading-lg" onClick={() => toggleSection("sidebar-categories")}>
              <FolderOpen size={14} />
              Categories
            </span>
            <button
              type="button"
              className="nav-heading-add"
              aria-label={collapsedSections.has("sidebar-categories") ? "Expand Categories" : "Collapse Categories"}
              onClick={() => toggleSection("sidebar-categories")}
            >
              {collapsedSections.has("sidebar-categories") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            </button>
          </div>
          {collapsedSections.has("sidebar-categories") ? null : (
            <>
              <button
                className={activeFilter === "music" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
                onClick={() => setActiveFilter("music")}
              >
                <Music2 size={13} />
                Soundtracks
              </button>
              <button
                className={activeFilter === "voiceover" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
                onClick={() => setActiveFilter("voiceover")}
              >
                <Mic size={13} />
                Voiceover
              </button>
              <button
                className={activeFilter === "sound_effect" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
                onClick={() => setActiveFilter("sound_effect")}
              >
                <Waves size={13} />
                Sound Effects
              </button>
              <button
                className="nav-subitem"
                onClick={() => setSfxSubcategoriesOpen((previous) => !previous)}
              >
                {sfxSubcategoriesOpen ? <ChevronDown size={11} /> : <ChevronRight size={11} />} By category
              </button>
              {sfxSubcategoriesOpen
                ? tags
                    .filter((tag) => tag.facet === "action")
                    .map((tag) => (
                      <button
                        key={tag.id}
                        className={
                          typeof activeFilter === "object" && "tag" in activeFilter && activeFilter.tag === tag.id
                            ? "nav-subitem active"
                            : "nav-subitem"
                        }
                        onClick={() => setActiveFilter({ tag: tag.id })}
                      >
                        {tag.name}
                      </button>
                    ))
                : null}
              <button
                className={activeFilter === "foley" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
                onClick={() => setActiveFilter("foley")}
              >
                <Footprints size={13} />
                Foley
              </button>
              <button
                className={activeFilter === "ambience" ? "nav-item sidebar-styled-item active" : "nav-item sidebar-styled-item"}
                onClick={() => setActiveFilter("ambience")}
              >
                <Wind size={13} />
                Ambience
              </button>
            </>
          )}
          <div className="nav-heading-row">
            <span className="nav-heading-lg" onClick={() => toggleSection("sidebar-projects")}>
              <Clapperboard size={14} />
              Projects
            </span>
            <button
              type="button"
              className="nav-heading-add"
              aria-label="New project"
              title="New project"
              onClick={(event) => {
                event.stopPropagation();
                setNewProjectModalOpen(true);
              }}
            >
              <Plus size={16} />
            </button>
            <button
              type="button"
              className="nav-heading-add"
              aria-label={collapsedSections.has("sidebar-projects") ? "Expand Projects" : "Collapse Projects"}
              onClick={() => toggleSection("sidebar-projects")}
            >
              {collapsedSections.has("sidebar-projects") ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            </button>
          </div>
          {collapsedSections.has("sidebar-projects")
            ? null
            : collections.map((project) => (
                <div
                  className={dragOverProjectId === project.id ? "nav-item-row drag-over" : "nav-item-row"}
                  key={project.id}
                  data-drop-project-id={project.collection_type === "Project" ? project.id : undefined}
                >
                  {project.collection_type === "Project" ? (
                    <button
                      type="button"
                      className="project-quick-add"
                      aria-label={
                        playingAssetId
                          ? `Add currently playing track to ${project.name}`
                          : "Play a track first to add it to a project"
                      }
                      title={
                        playingAssetId
                          ? `Add currently playing track to ${project.name}`
                          : "Play a track first to add it to a project"
                      }
                      disabled={!playingAssetId}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleAddPlayingToProject(project);
                      }}
                    >
                      <Plus size={14} />
                    </button>
                  ) : null}
                  <button
                    className={
                      typeof activeFilter === "object" && "project" in activeFilter && activeFilter.project === project.id
                        ? "nav-item active"
                        : "nav-item"
                    }
                    onClick={() => setActiveFilter({ project: project.id, smart: project.collection_type === "Smart" })}
                  >
                    {project.collection_type === "Smart" ? <Zap size={14} /> : null}
                    {project.name}
                  </button>
                  {project.collection_type === "Project" ? (
                    <button
                      type="button"
                      className="project-edit-button"
                      aria-label={`Edit ${project.name}`}
                      title={`Edit ${project.name}`}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleOpenEditProject(project);
                      }}
                    >
                      <Pencil size={13} />
                    </button>
                  ) : null}
                  {project.collection_type === "Project" ? (
                    <button
                      type="button"
                      className={projectHasExportFolder(project) ? "dr-button" : "dr-button disabled"}
                      aria-label={projectExportFolderSummary(project)}
                      title={
                        projectHasExportFolder(project)
                          ? projectExportFolderSummary(project)
                          : "Set a sound folder or sound effects folder for this project to enable quick export"
                      }
                      disabled={!projectHasExportFolder(project) || bulkAssetIds.length === 0}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleExportToProject(project, bulkAssetIds);
                      }}
                    >
                      <Clapperboard size={16} />
                    </button>
                  ) : null}
                </div>
              ))}
          <div className="virtualization-bar" aria-label="Browser performance">
            <span>{visibleAssets.length} row{visibleAssets.length === 1 ? "" : "s"}</span>
            <span>{browserVisibleRange.endExclusive - browserVisibleRange.start} rendered</span>
          </div>
        </div>
      </aside>
      <section className="workspace">
        <AnimatePresence>
          {isExternalDragActive ? (
            <motion.div
              className="external-drop-overlay"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.12 }}
            >
              <motion.div
                className="external-drop-overlay-frame"
                initial={{ opacity: 0, scale: 0.9, y: 6 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.92, y: 4 }}
                transition={{ duration: 0.16, ease: [0.4, 0, 0.2, 1] }}
              >
                <Import size={22} />
                <p>Drop to import</p>
              </motion.div>
            </motion.div>
          ) : null}
        </AnimatePresence>
        <AnimatePresence>
          {importToast ? (
            <motion.div
              className="import-toast"
              key={importToast.id}
              initial={{ opacity: 0, scale: 0.94 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.96 }}
              transition={{ duration: 0.18, ease: [0.4, 0, 0.2, 1] }}
            >
              <div className="import-toast-pill">
                <CheckCircle2 size={15} />
                <span>{importToast.message}</span>
              </div>
            </motion.div>
          ) : null}
        </AnimatePresence>
        <header className="topbar">
          <button
            type="button"
            className={editorWorkflowOpen ? "primary-action active" : "primary-action"}
            onClick={() => setEditorWorkflowOpen((previous) => !previous)}
            aria-pressed={editorWorkflowOpen}
          >
            <Workflow size={16} />
            Editor Workflow
          </button>
          <button className="primary-action" onClick={() => handleExportSelected()} disabled={!selectedAssetId}>
            Export Selected
          </button>
          <button className="primary-action" type="button" onClick={handleImportFolder}>
            <Import size={16} />
            Import
          </button>
          <label className="search">
            <Search size={16} />
            <input
              ref={searchInputRef}
              placeholder="Search sounds, tags, source, license"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
            />
          </label>
          <div style={{ position: "relative" }}>
            <button
              ref={filterButtonRef}
              className="icon-button"
              aria-label="Filter"
              onClick={() => {
                if (!filterMenuOpen && filterButtonRef.current) {
                  const rect = filterButtonRef.current.getBoundingClientRect();
                  setFilterMenuPosition({ top: rect.bottom + 6, left: rect.left });
                }
                setFilterMenuOpen((previous) => !previous);
              }}
            >
              <ListFilter size={17} />
            </button>
            <AnimatePresence>
              {filterMenuOpen && filterMenuPosition ? (
                <motion.div
                  className="modal-card filter-menu"
                  style={{ top: filterMenuPosition.top, left: filterMenuPosition.left }}
                  initial={{ opacity: 0, scale: 0.95, y: -6 }}
                  animate={{ opacity: 1, scale: 1, y: 0 }}
                  exit={{ opacity: 0, scale: 0.95, y: -6 }}
                  transition={{ duration: 0.16, ease: [0.4, 0, 0.2, 1] }}
                >
                  {smartFilters.map((filter) => (
                    <button
                      key={filter.label}
                      className={activeFilter === filter.id ? "nav-item active" : "nav-item"}
                      onClick={() => {
                        setActiveFilter(filter.id);
                        setFilterMenuOpen(false);
                      }}
                    >
                      {filter.label}
                    </button>
                  ))}
                </motion.div>
              ) : null}
            </AnimatePresence>
          </div>
          <button
            type="button"
            className="icon-button"
            aria-label="Refresh library"
            onClick={() => handleRefreshLibrary()}
            disabled={!activeLibrary?.media_root}
            title={activeLibrary?.media_root ? "Scan the media root for new files" : "Import a folder first to set this library's media root"}
          >
            <RefreshCw size={16} />
          </button>
          <button
            type="button"
            className={[
              "icon-button",
              "activity-button",
              activityBusy ? "busy" : "",
              activityPreparing ? "preparing" : ""
            ]
              .filter(Boolean)
              .join(" ")}
            aria-label="Background activity"
            title={
              activityBusy
                ? "Background work is running — click for details"
                : activityPreparing
                  ? "Preparing — click for details"
                  : "Background activity: all caught up"
            }
            onClick={() => setBackgroundActivityOpen(true)}
          >
            <Activity size={16} />
            {activityPreparing ? (
              <span className="activity-pulse-icon" aria-hidden="true">
                <svg width={16} height={16} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
                  <path
                    pathLength={1}
                    d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"
                  />
                </svg>
              </span>
            ) : null}
            <span className="activity-led" aria-hidden="true" />
          </button>
          <button
            className="icon-button"
            aria-label={trashItems.length > 0 ? `Open Trash (${trashItems.length} items)` : "Open Trash"}
            title="Trash"
            onClick={() => setTrashModalOpen(true)}
          >
            <Archive size={17} />
            {trashItems.length > 0 ? <span className="icon-button-badge">{trashItems.length}</span> : null}
          </button>
          <button className="icon-button" aria-label="Open settings" onClick={() => setSettingsOpen(true)}>
            <Settings size={17} />
          </button>
        </header>
        {licenseMode === "trial" ? (
          <button
            type="button"
            className="trial-banner"
            onClick={() => setLicenseFormOpen(true)}
            aria-label="Trial active — click to activate your license"
          >
            <span>
              {licenseStatus?.trial_days_remaining ?? 0} day
              {(licenseStatus?.trial_days_remaining ?? 0) === 1 ? "" : "s"} left in trial
            </span>
            <span className="trial-banner-action">Activate</span>
          </button>
        ) : null}
        {queryFilters.length > 0 ? (
          <div className="chip-row status-strip" aria-label="Status">
            {queryFilters.map((filter, index) => (
              <span key={index} className="suggestion-chip">
                {filter.field} {filter.operator} {filter.value}
              </span>
            ))}
          </div>
        ) : null}
        <section className="filter-panel" aria-label="Range filters">
          <div className="selection-bar" aria-label="Selection actions">
            <strong>{(selectedCount || (selectedAsset ? 1 : 0))} selected</strong>
          </div>
          <ListFilter size={13} />
          <label>
            Duration
            <input
              type="number"
              min={0}
              placeholder="min s"
              value={rangeFilters.durationMinSec ?? ""}
              onChange={(event) =>
                setRangeFilters((previous) => ({
                  ...previous,
                  durationMinSec: event.target.value === "" ? undefined : Number(event.target.value)
                }))
              }
            />
            <span>–</span>
            <input
              type="number"
              min={0}
              placeholder="max s"
              value={rangeFilters.durationMaxSec ?? ""}
              onChange={(event) =>
                setRangeFilters((previous) => ({
                  ...previous,
                  durationMaxSec: event.target.value === "" ? undefined : Number(event.target.value)
                }))
              }
            />
          </label>
          <label>
            BPM
            <input
              type="number"
              min={0}
              placeholder="min"
              value={rangeFilters.bpmMin ?? ""}
              onChange={(event) =>
                setRangeFilters((previous) => ({
                  ...previous,
                  bpmMin: event.target.value === "" ? undefined : Number(event.target.value)
                }))
              }
            />
            <span>–</span>
            <input
              type="number"
              min={0}
              placeholder="max"
              value={rangeFilters.bpmMax ?? ""}
              onChange={(event) =>
                setRangeFilters((previous) => ({
                  ...previous,
                  bpmMax: event.target.value === "" ? undefined : Number(event.target.value)
                }))
              }
            />
          </label>
          <label>
            Format
            <select
              aria-label="Filter by audio format"
              value={formatFilter ?? ""}
              onChange={(event) => setFormatFilter(event.target.value === "" ? null : (event.target.value as AudioFormat))}
            >
              <option value="">All</option>
              {AUDIO_FORMATS.map((format) => (
                <option key={format} value={format}>
                  {format.toUpperCase()}
                </option>
              ))}
            </select>
          </label>
          {hasActiveRangeFilters(rangeFilters) || formatFilter ? (
            <>
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  setRangeFilters({});
                  setFormatFilter(null);
                }}
              >
                Clear
              </button>
              <button type="button" className="text-button" onClick={() => setSmartCollectionModalOpen(true)}>
                <Zap size={13} />
                Save as Smart Collection
              </button>
            </>
          ) : null}
        </section>
        {instrumentFilter?.instrumentPage ? (
          <InstrumentDetectionPage
            libraryId={activeLibraryId}
            selected={instrumentFilter.instruments}
            onPick={pickInstrument}
            onToggle={toggleInstrument}
            onApply={() =>
              setActiveFilter({ instrumentPage: false, instruments: instrumentFilter.instruments })
            }
          />
        ) : (
        <section
          className="browser"
          aria-label="Sound browser"
          data-density={preferences?.browser_density ?? "Comfortable"}
          ref={browserScrollRef}
          onScroll={(event) => setBrowserScrollTop(event.currentTarget.scrollTop)}
        >
          {instrumentFilter ? (
            <div className="instrument-filter-bar">
              <button
                type="button"
                className="instrument-filter-back"
                onClick={() =>
                  setActiveFilter({ instrumentPage: true, instruments: instrumentFilter.instruments })
                }
              >
                <ChevronLeft size={13} />
                Instruments
              </button>
              {instrumentFilter.instruments.map((name) => (
                <button
                  key={name}
                  type="button"
                  className="instrument-chip"
                  onClick={() => toggleInstrument(name)}
                  aria-label={`Remove ${name} filter`}
                >
                  {name}
                  <X size={11} />
                </button>
              ))}
            </div>
          ) : null}
          <div className="browser-header">
            <span aria-hidden="true" />
            <span aria-hidden="true" />
            <span>Name</span>
            <span>Type</span>
            <span>Storage</span>
            <span>Size</span>
            <span>Status</span>
            <span aria-hidden="true" />
          </div>
          {visibleAssets.length === 0 ? (
            <p className="empty-browser">No sounds here yet.</p>
          ) : (
            <>
              {browserVisibleRange.offsetTopPx > 0 ? (
                <div style={{ height: browserVisibleRange.offsetTopPx }} aria-hidden="true" />
              ) : null}
              {visibleAssets.slice(browserVisibleRange.start, browserVisibleRange.endExclusive).map((asset, sliceIndex) => {
              const index = browserVisibleRange.start + sliceIndex;
              const isSelected = browserState
                ? browserState.selected_indices.includes(index)
                : asset.id === selectedAssetId;
              const { Icon: RowIcon, color: rowIconColor } = rowIconMeta(asset);
              return (
              <article
                className={isSelected ? "asset-row selected" : "asset-row"}
                key={asset.id}
                data-asset-id={asset.id}
                onClick={(event) => handleRowClick(asset, index, event)}
                onPointerDown={(event) => handleAssetPointerDown(event, asset)}
              >
                <button
                  className="play-cell"
                  aria-label={`Preview ${asset.display_name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedAssetId(asset.id);
                    if (playingAssetId === asset.id) {
                      const audio = audioRef.current;
                      if (audio) (audio.paused ? audio.play() : audio.pause());
                    } else {
                      loadAssetForPlayback(asset, true);
                    }
                  }}
                >
                  {playingAssetId === asset.id && isPlaying ? <Pause size={15} /> : <Play size={15} />}
                </button>
                <div className="waveform" aria-hidden="true" style={{ color: rowIconColor }}>
                  <RowIcon size={16} />
                </div>
                <strong>
                  {asset.display_name}
                  {asset.stem_group_id ? (
                    <span className="stem-badge" title="Stems available — select to view in the inspector">
                      <Layers size={11} />
                    </span>
                  ) : null}
                </strong>
                <span>{asset.media_type}</span>
                <span>{asset.storage_mode}</span>
                <span>{formatFileSize(asset.file_size)}</span>
                <span>{asset.availability_state}</span>
                {(() => {
                  const memberships = membershipsByAsset.get(asset.id) ?? [];
                  if (memberships.length === 0) return <span />;
                  const anyExported = memberships.some((membership) => membership.exported);
                  const sendableCount = memberships.filter((membership) => projectHasExportFolder(membership)).length;
                  const projectNames = memberships.map((membership) => membership.project_name).join(", ");
                  const label = anyExported
                    ? `Already sent to a project folder (${projectNames}) — click to send again`
                    : sendableCount > 0
                      ? `Send to project folder (${projectNames})`
                      : `In project "${projectNames}" — no export folder configured yet`;
                  return (
                    <button
                      type="button"
                      className={anyExported ? "icon-button project-status exported" : "icon-button project-status pending"}
                      aria-label={label}
                      title={label}
                      disabled={sendableCount === 0}
                      onClick={(event) => {
                        event.stopPropagation();
                        handleSendAssetToItsProjects(asset, memberships);
                      }}
                    >
                      <Clapperboard size={15} />
                    </button>
                  );
                })()}
                <button
                  className="favorite"
                  aria-label={`Favorite ${asset.display_name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    handleToggleFavorite(asset);
                  }}
                >
                  <Star size={15} fill={asset.favorite ? "currentColor" : "none"} />
                </button>
                {asset.availability_state === "Missing" ? (
                  <button
                    className="icon-button"
                    aria-label={`Relink ${asset.display_name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      handleRelinkAsset(asset);
                    }}
                  >
                    <Link2 size={14} />
                  </button>
                ) : (
                  <span />
                )}
              </article>
              );
              })}
              {browserVisibleRange.spacerBottomPx > 0 ? (
                <div style={{ height: browserVisibleRange.spacerBottomPx }} aria-hidden="true" />
              ) : null}
            </>
          )}
        </section>
        )}
        <AnimatePresence>
          {editorWorkflowOpen ? (
            <motion.section
              className="editor-workflow"
              aria-label="Editor workflow"
              initial={{ opacity: 0, y: 18, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 18, scale: 0.98 }}
              transition={{ duration: 0.28, ease: [0.4, 0, 0.2, 1] }}
            >
              <div className="editor-workflow-head">
                <div className="editor-workflow-heading">
                  <span className="editor-workflow-heading-icon">
                    <Workflow size={17} />
                  </span>
                  <div>
                    <h1>Editor Workflow</h1>
                    <p>
                      {bulkAssetIds.length === 0
                        ? "Select a sound to send it into your edit"
                        : bulkAssetIds.length === 1
                        ? (selectedAsset?.display_name ?? "1 sound selected")
                        : `${bulkAssetIds.length} sounds selected`}
                    </p>
                  </div>
                </div>
                <button
                  type="button"
                  className="icon-button"
                  aria-label="Close editor workflow"
                  onClick={() => setEditorWorkflowOpen(false)}
                >
                  <X size={16} />
                </button>
              </div>

              <div className="editor-workflow-hero" aria-hidden="true">
                <svg viewBox="0 0 640 148" className="editor-workflow-flow" preserveAspectRatio="xMidYMid meet">
                  <defs>
                    <linearGradient id="editorFlowGradient" x1="0" y1="0" x2="1" y2="0">
                      <stop offset="0%" stopColor="var(--text-muted)" />
                      <stop offset="100%" stopColor="var(--text-secondary)" />
                    </linearGradient>
                  </defs>
                  <line x1="96" y1="74" x2="544" y2="74" className="ew-flow-track" />
                  <motion.line
                    x1="96"
                    y1="74"
                    x2="544"
                    y2="74"
                    className="ew-flow-dash"
                    strokeDasharray="2 16"
                    animate={{ strokeDashoffset: [0, -36] }}
                    transition={{ repeat: Infinity, duration: 1.1, ease: "linear" }}
                  />
                  <g className="ew-node ew-node-source">
                    <circle cx="60" cy="74" r="32" />
                    <path d="M48 66 v16 M56 60 v28 M64 64 v20 M72 68 v12" className="ew-node-glyph" />
                  </g>
                  <g className="ew-node ew-node-project">
                    <circle cx="320" cy="74" r="24" />
                    <path d="M309 66h22v18h-22z M309 66l4-6h14l4 6" className="ew-node-glyph" />
                  </g>
                  <g className="ew-node ew-node-dest">
                    <circle cx="580" cy="74" r="32" />
                    <path d="M572 74 L588 63 M572 74 L588 85" className="ew-node-glyph" />
                    <circle cx="571" cy="74" r="3.4" className="ew-node-glyph-dot" />
                    <circle cx="589" cy="62" r="3.4" className="ew-node-glyph-dot" />
                    <circle cx="589" cy="86" r="3.4" className="ew-node-glyph-dot" />
                  </g>
                </svg>
              </div>

              <div className="editor-workflow-actions">
                <button
                  type="button"
                  className="editor-action-button"
                  disabled={bulkAssetIds.length === 0}
                  onClick={() => handleRevealAssets(bulkAssetIds)}
                >
                  <FolderOpen size={16} />
                  Reveal in {isMacPlatform ? "Finder" : "Explorer"}
                </button>
                <button
                  type="button"
                  className="editor-action-button"
                  disabled={bulkAssetIds.length === 0}
                  onClick={() => handleCopyAssetPaths(bulkAssetIds)}
                >
                  <Copy size={16} />
                  Copy File Path
                  <kbd>{modKeyLabel}⇧C</kbd>
                </button>
              </div>

              {editorActionStatus ? <div className="editor-workflow-status">{editorActionStatus}</div> : null}

              <div className="editor-workflow-projects">
                <h2>Send to Resolve timeline</h2>
                {collections.filter((project) => project.collection_type === "Project").length === 0 ? (
                  <p className="editor-workflow-empty">
                    Create a project with an export folder — its structure syncs into Resolve automatically.
                  </p>
                ) : (
                  <div className="editor-project-list">
                    {collections
                      .filter((project) => project.collection_type === "Project")
                      .map((project) => {
                        const sync: ResolveSyncStatus = resolveSyncStatuses[project.id] ?? {
                          collection_id: project.id,
                          status: "not_synced",
                          error: null,
                          approved_at: null,
                          last_synced_at: null
                        };
                        return (
                          <div className="editor-project-card" key={project.id}>
                            <span className="editor-project-thumb">
                              <Music size={14} />
                            </span>
                            <div className="editor-project-meta">
                              <strong>{project.name}</strong>
                              <small>
                                {sync.status === "syncing"
                                  ? "Syncing structure to Resolve…"
                                  : sync.status === "failed"
                                  ? `Sync failed: ${sync.error ?? "unknown error"}`
                                  : sync.status === "synced" && !sync.approved_at
                                  ? "Structure synced — approve to enable sending"
                                  : sync.status === "synced" && sync.approved_at
                                  ? "Approved — ready to send"
                                  : projectHasExportFolder(project)
                                  ? "Not yet synced"
                                  : "No export folder set"}
                              </small>
                            </div>
                            {sync.status === "synced" && !sync.approved_at ? (
                              <button
                                type="button"
                                className="dr-button"
                                aria-label={`Approve Resolve structure for ${project.name}`}
                                onClick={() => handleApproveResolveSync(project)}
                              >
                                <ShieldCheck size={15} />
                                Approve
                              </button>
                            ) : (
                              <button
                                type="button"
                                className={
                                  sync.approved_at && bulkAssetIds.length > 0 ? "dr-button" : "dr-button disabled"
                                }
                                disabled={!sync.approved_at || bulkAssetIds.length === 0}
                                aria-label={`Send selected to ${project.name}'s Resolve timeline`}
                                title={
                                  sync.approved_at
                                    ? undefined
                                    : "Structure must be synced and approved before sending to the timeline"
                                }
                                onClick={() => handleSendAssetsToResolveTimeline(bulkAssetIds, project)}
                              >
                                <Clapperboard size={15} />
                                Send to Timeline
                              </button>
                            )}
                          </div>
                        );
                      })}
                  </div>
                )}
              </div>
            </motion.section>
          ) : null}
        </AnimatePresence>
      </section>
      <aside className={inspectorCollapsed ? "inspector collapsed" : "inspector"} aria-label="Inspector">
        <div className="panel-body">
        {selectedCount > 1 ? (
          <CollapsibleSection
            id="bulk"
            title={`${selectedCount} Selected`}
            collapsed={collapsedSections.has("bulk")}
            onToggle={toggleSection}
            aria-label="Bulk actions"
          >
            <div className="drop-target-grid">
              <button type="button" onClick={handleBulkFavorite}>
                Favorite All
              </button>
              <button type="button" onClick={handleBulkExport}>
                Export All
              </button>
              <button type="button" className="text-button" onClick={handleBulkTrash}>
                Move All to Trash
              </button>
            </div>
          </CollapsibleSection>
        ) : null}
        {selectedAsset ? (
          <CollapsibleSection
            id="quick"
            title="Quick Actions"
            collapsed={collapsedSections.has("quick")}
            onToggle={toggleSection}
          >
            <div className="quick-action-row">
              <button
                type="button"
                className={selectedAsset.review_state === "Reviewed" ? "icon-button reviewed active" : "icon-button reviewed"}
                aria-pressed={selectedAsset.review_state === "Reviewed"}
                aria-label={selectedAsset.review_state === "Reviewed" ? "Mark unreviewed" : "Mark reviewed"}
                title={`${selectedAsset.review_state === "Reviewed" ? "Mark unreviewed" : "Mark reviewed"} (${modKeyLabel}R)`}
                onClick={() => handleToggleReviewed(selectedAsset)}
              >
                <ShieldCheck size={17} />
              </button>
              <button
                type="button"
                className="icon-button danger"
                aria-label="Move to Trash"
                title="Move to Trash"
                onClick={handleMoveToTrash}
              >
                <Trash2 size={17} />
              </button>
            </div>
            <h2>Classify</h2>
            <div className="classification-row">
              {mediaTypeOptions.map(({ value, label, Icon, color, shortcutDigit }) => {
                const active = selectedAsset.media_type === value;
                return (
                  <button
                    key={value}
                    type="button"
                    className={active ? "classification-chip active" : "classification-chip"}
                    style={active ? ({ "--classification-color": color } as CSSProperties) : undefined}
                    aria-pressed={active}
                    title={`${label} (${shortcutDigit})`}
                    onClick={() => handleSetMediaType(selectedAsset, value)}
                  >
                    <Icon size={14} style={{ color: active ? color : undefined }} />
                    {label}
                    <kbd className="classification-chip-shortcut">{shortcutDigit}</kbd>
                  </button>
                );
              })}
            </div>
          </CollapsibleSection>
        ) : null}
        <CollapsibleSection id="tags-section" title="Tags" collapsed={collapsedSections.has("tags-section")} onToggle={toggleSection}>
          <h2>Suggested Tags</h2>
          <div className="chip-row">
            {suggestedTags.length === 0 ? (
              <span className="empty-hint">No pending suggestions</span>
            ) : (
              suggestedTags.map((tag) => (
                <span className="suggestion-chip" key={tag.id}>
                  <span className="chip-label">{tag.name}</span>
                  <button onClick={() => handleAcceptSuggestion(tag)} aria-label={`Accept ${tag.name}`}>
                    ✓
                  </button>
                  <button onClick={() => handleRejectSuggestion(tag)} aria-label={`Reject ${tag.name}`}>
                    ✗
                  </button>
                </span>
              ))
            )}
          </div>
          <h2>Applied Tags</h2>
          <div className="chip-row">
            {appliedTags.length === 0 ? (
              <span className="empty-hint">No tags applied</span>
            ) : (
              appliedTags.map((tag) => (
                <span key={tag.id} className="suggestion-chip">
                  <span className="chip-label">{tag.name}</span>
                  <button onClick={() => handleRemoveTag(tag)} aria-label={`Remove ${tag.name}`}>
                    ✗
                  </button>
                </span>
              ))
            )}
          </div>
          {(() => {
            // Quick-tag chips built straight from what Sonic Radar has
            // already detected for this sound (instruments, vocal
            // presence) — the tag recommendations the user actually has
            // real signal for, not just guesses from the filename. Skips
            // anything already applied.
            const quickTagCandidates: { key: string; label: string; facet: string }[] = [];
            for (const entry of assetInstruments) {
              if (!appliedTags.some((tag) => tag.name.toLowerCase() === entry.name.toLowerCase())) {
                quickTagCandidates.push({ key: `instrument-${entry.name}`, label: entry.name, facet: "instrument" });
              }
            }
            if (selectedAsset?.vocal_ratio != null) {
              const label = selectedAsset.vocal_ratio >= VOCAL_RATIO_THRESHOLD ? "Vocal" : "Instrumental";
              if (!appliedTags.some((tag) => tag.name.toLowerCase() === label.toLowerCase())) {
                quickTagCandidates.push({ key: `vocal-${label}`, label, facet: "vocal" });
              }
            }
            return quickTagCandidates.length > 0 ? (
              <>
                <h2>From Analysis</h2>
                <div className="chip-row" aria-label="Tag suggestions from Sonic Radar analysis">
                  {quickTagCandidates.map((candidate) => (
                    <button
                      key={candidate.key}
                      type="button"
                      className="quick-tag-chip"
                      title={`Add "${candidate.label}" as a tag`}
                      onClick={() => handleQuickTag(candidate.label, candidate.facet)}
                    >
                      + {candidate.label}
                    </button>
                  ))}
                </div>
              </>
            ) : null;
          })()}
          <h2>Apply Tag</h2>
          <div className="tag-grid">
            {tags
              .filter((tag) => {
                // Action-shape tags (Impact/Whoosh/Rise, and anything else
                // under the same facet) describe a transient hit's shape —
                // only meaningful for a short SFX/Foley hit, not a
                // Soundtrack/Voiceover/Ambience track. Keeps the grid from
                // offering tags that can't actually apply to what's
                // selected — e.g. a Soundtrack shouldn't be cluttered with
                // SFX-only tags.
                if (tag.facet !== "action") return true;
                return selectedAsset?.media_type === "sound_effect" || selectedAsset?.media_type === "foley";
              })
              .map((tag) => (
                <button key={tag.id} onClick={() => handleApplyTag(tag)} disabled={bulkAssetIds.length === 0}>
                  {tag.name}
                </button>
              ))}
          </div>
          <div className="setup-field-row">
            <input
              placeholder="New tag name"
              value={newTagName}
              onChange={(event) => setNewTagName(event.target.value)}
            />
            <input
              placeholder="facet"
              value={newTagFacet}
              onChange={(event) => setNewTagFacet(event.target.value)}
              style={{ width: 90 }}
            />
            <button type="button" onClick={handleCreateAndApplyTag} disabled={!newTagName.trim() || !selectedAssetId}>
              Add
            </button>
          </div>
        </CollapsibleSection>
        <CollapsibleSection id="projects" title="Projects" collapsed={collapsedSections.has("projects")} onToggle={toggleSection}>
          <div className="project-grid">
            {collections
              .filter((project) => project.collection_type !== "Smart")
              .map((project) => {
                const trackCount = trackCountByProject.get(project.id) ?? 0;
                return (
                  <div
                    key={project.id}
                    className={dragOverProjectId === project.id ? "project-card drag-over" : "project-card"}
                    data-drop-project-id={project.id}
                  >
                    <button
                      type="button"
                      className={bulkAssetIds.length === 0 ? "project-card-add no-selection" : "project-card-add"}
                      title={`Add selected to ${project.name} (or drag a track here)`}
                      aria-label={`Add selected to ${project.name}`}
                      onClick={() => handleAddSelectedToProject(project)}
                      // Not a native `disabled` button: dragging an unselected
                      // row here (nothing in bulkAssetIds) is exactly the case
                      // this exists for, and a disabled control is invisible to
                      // elementFromPoint's drop hit-testing. The click path
                      // already no-ops on an empty selection; `.no-selection`
                      // above just dims it to match.
                      aria-disabled={bulkAssetIds.length === 0}
                    >
                      <span className="project-card-icon">
                        <Clapperboard size={16} />
                      </span>
                      <span className="project-card-name">{project.name}</span>
                    </button>
                    {trackCount > 0 ? (
                      <button
                        type="button"
                        className="project-card-count"
                        aria-label={`${trackCount} track${trackCount === 1 ? "" : "s"} sent to ${project.name} — click to view`}
                        title={`${trackCount} track${trackCount === 1 ? "" : "s"} sent to ${project.name} — click to view`}
                        onClick={(event) => {
                          event.stopPropagation();
                          handleToggleProjectTracks(project, event.currentTarget);
                        }}
                      >
                        {trackCount}
                      </button>
                    ) : null}
                  </div>
                );
              })}
          </div>
        </CollapsibleSection>
        <AnimatePresence>
          {projectTracksPanel ? (
            <motion.div
              className="modal-card project-tracks-popover"
              style={{ top: projectTracksPanel.position.top, left: projectTracksPanel.position.left }}
              initial={{ opacity: 0, scale: 0.95, y: -6 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.95, y: -6 }}
              transition={{ duration: 0.16, ease: [0.4, 0, 0.2, 1] }}
            >
              <div className="project-tracks-popover-head">
                <span className="project-tracks-popover-title">{projectTracksPanel.project.name}</span>
                <button
                  type="button"
                  className="icon-button"
                  aria-label="Close"
                  onClick={() => setProjectTracksPanel(null)}
                >
                  <X size={14} />
                </button>
              </div>
              {projectTracksPanel.assets === null ? (
                <p className="empty-hint">Loading…</p>
              ) : projectTracksPanel.assets.length === 0 ? (
                <p className="empty-hint">No tracks yet.</p>
              ) : (
                <ul className="project-tracks-popover-list">
                  {projectTracksPanel.assets.map((asset) => (
                    <li key={asset.id}>{asset.display_name}</li>
                  ))}
                </ul>
              )}
            </motion.div>
          ) : null}
        </AnimatePresence>
        {selectedAsset && stemGroupMembers.length > 0 ? (
          <CollapsibleSection
            id="stems-track"
            title="Stems"
            icon={<Layers size={14} />}
            className="stems-track-section"
            collapsed={collapsedSections.has("stems-track")}
            onToggle={toggleSection}
          >
            <p className="settings-hint">
              {stemGroupMembers.length} part{stemGroupMembers.length === 1 ? "" : "s"} — pick one to
              play, independent of the browser's own selection.
            </p>
            <div className="stem-member-list">
              {stemGroupMembers.map((member) => {
                const isLoaded = playingAssetId === member.id;
                return (
                  <button
                    key={member.id}
                    type="button"
                    className={isLoaded ? "stem-member-row active" : "stem-member-row"}
                    onClick={() => {
                      if (isLoaded) {
                        const audio = audioRef.current;
                        if (audio) (audio.paused ? audio.play() : audio.pause());
                      } else {
                        loadAssetForPlayback(member, true);
                      }
                    }}
                  >
                    <span className="stem-member-icon">
                      {isLoaded && isPlaying ? <Pause size={13} /> : <Play size={13} />}
                    </span>
                    <span className="stem-member-label">{member.stem_label ?? member.display_name}</span>
                    {member.stem_is_primary ? <span className="stem-member-primary-tag">Primary</span> : null}
                  </button>
                );
              })}
            </div>
          </CollapsibleSection>
        ) : null}
        <CollapsibleSection
          id="sonic-radar-track"
          title="Sonic Radar"
          icon={<Activity size={14} />}
          className="sonic-radar-track-section"
          collapsed={collapsedSections.has("sonic-radar-track")}
          onToggle={toggleSection}
        >
          {selectedAsset ? (
            <>
              <div className="radar-status-list">
                {[
                  {
                    key: "tempo",
                    label: "Tempo",
                    detected: selectedAsset.bpm != null,
                    value: selectedAsset.bpm != null ? `~${Math.round(selectedAsset.bpm)} BPM` : null,
                    kind: "audio_analysis" as const,
                    hint: "Runs the full audio analysis pass (tempo, key, pitch, and vocals are detected together in one decode)"
                  },
                  {
                    key: "key",
                    label: "Key",
                    detected: selectedAsset.detected_key != null,
                    value: selectedAsset.detected_key,
                    kind: "audio_analysis" as const,
                    hint: "Runs the full audio analysis pass (tempo, key, pitch, and vocals are detected together in one decode)"
                  },
                  {
                    key: "pitch",
                    label: "Pitch",
                    detected: selectedAsset.musical_key != null,
                    value: selectedAsset.musical_key,
                    kind: "audio_analysis" as const,
                    hint: "Runs the full audio analysis pass (tempo, key, pitch, and vocals are detected together in one decode)"
                  },
                  {
                    key: "vocals",
                    label: "Vocals",
                    detected: selectedAsset.vocal_ratio != null,
                    value:
                      selectedAsset.vocal_ratio != null
                        ? `${(selectedAsset.vocal_ratio ?? 0) >= VOCAL_RATIO_THRESHOLD ? "Vocal" : "Instrumental"} · ${Math.round(
                            (selectedAsset.vocal_ratio ?? 0) * 100
                          )}%`
                        : null,
                    kind: "audio_analysis" as const,
                    hint: "Runs the full audio analysis pass (tempo, key, pitch, and vocals are detected together in one decode)"
                  },
                  {
                    key: "instruments",
                    label: "Instruments",
                    detected: assetInstruments.length > 0,
                    value: assetInstruments.length > 0 ? `${assetInstruments.length} detected` : null,
                    kind: "instrument_detection" as const,
                    hint: "Runs instrument detection separately — it's its own model/pass, not part of the audio analysis above"
                  }
                ].map((row) => {
                  const busyHere = sonicRadarBusyKind === row.kind;
                  const modelMissing = row.kind === "instrument_detection" && instrumentDetectionAvailable === false;
                  return (
                    <div className="radar-status-row" key={row.key}>
                      <span className={row.detected ? "radar-status-icon detected" : "radar-status-icon"}>
                        {row.detected ? <CheckCircle2 size={15} /> : <Circle size={15} />}
                      </span>
                      <span className="radar-status-name">{row.label}</span>
                      {row.detected ? (
                        <span className="radar-status-value">{row.value}</span>
                      ) : modelMissing ? (
                        <span className="radar-status-hint" title="Detection model isn't installed — see docs">
                          Model not installed
                        </span>
                      ) : (
                        <button
                          type="button"
                          className="radar-analyze-btn"
                          // Only one analysis runs from this panel at a time — a
                          // second click here would just requeue the same job
                          // (or the other kind's job) on top of one already
                          // in flight, with no way to show two progress bars
                          // at once, so every not-yet-detected row disables
                          // while any kind is busy, not just its own.
                          disabled={sonicRadarBusyKind !== null}
                          title={row.hint}
                          onClick={() => handleAnalyzeAssetKind(row.kind)}
                        >
                          {busyHere ? "Analyzing…" : "Analyze"}
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
              {sonicRadarBusyKind ? (
                (() => {
                  const job = jobProgress.find((entry) => entry.kind === sonicRadarBusyKind);
                  const done = job ? job.total - job.pending : 0;
                  const percent = job && job.total > 0 ? Math.round((done / job.total) * 100) : null;
                  return (
                    <div className="radar-progress" aria-label="Analysis progress" role="status">
                      <span className="radar-progress-track">
                        <span
                          className={percent == null ? "radar-progress-fill indeterminate" : "radar-progress-fill"}
                          style={{ width: `${percent ?? 30}%` }}
                        />
                      </span>
                      <span className="radar-progress-label">
                        {sonicRadarStatus ?? "Analyzing…"}
                        {job ? ` (${done}/${job.total})` : ""}
                      </span>
                    </div>
                  );
                })()
              ) : sonicRadarStatus ? (
                <div className="status-line">{sonicRadarStatus}</div>
              ) : null}
            </>
          ) : (
            <span className="empty-hint">Select a sound to see its Sonic Radar status</span>
          )}
        </CollapsibleSection>
        {selectedAsset && (selectedAsset.embedded_title || selectedAsset.embedded_genre || selectedAsset.embedded_comment) ? (
          <CollapsibleSection
            id="embedded"
            title="Embedded Metadata"
            collapsed={collapsedSections.has("embedded")}
            onToggle={toggleSection}
          >
            {selectedAsset.embedded_title ? <div className="status-line">Title: {selectedAsset.embedded_title}</div> : null}
            {selectedAsset.embedded_genre ? <div className="status-line">Genre: {selectedAsset.embedded_genre}</div> : null}
            {selectedAsset.embedded_comment ? <div className="status-line">Comment: {selectedAsset.embedded_comment}</div> : null}
          </CollapsibleSection>
        ) : null}
        {selectedAsset &&
        (selectedAsset.bpm != null ||
          selectedAsset.detected_key != null ||
          selectedAsset.musical_key != null ||
          selectedAsset.duration_ms != null ||
          assetInstruments.length > 0) ? (
          <CollapsibleSection
            id="detected"
            title="Detected Audio Attributes"
            collapsed={collapsedSections.has("detected")}
            onToggle={toggleSection}
          >
            <div className="attribute-grid">
              {selectedAsset.duration_ms != null ? (
                <div className="attribute-pill">
                  <span className="attribute-label">Duration</span>
                  <strong className="attribute-value">{formatTime(selectedAsset.duration_ms / 1000)}</strong>
                </div>
              ) : null}
              {selectedAsset.sample_rate != null ? (
                <div className="attribute-pill">
                  <span className="attribute-label">Sample rate</span>
                  <strong className="attribute-value">
                    {(selectedAsset.sample_rate / 1000).toFixed(1)} kHz
                    {selectedAsset.channels != null ? ` · ${selectedAsset.channels === 1 ? "Mono" : selectedAsset.channels === 2 ? "Stereo" : `${selectedAsset.channels}ch`}` : ""}
                  </strong>
                </div>
              ) : null}
              {selectedAsset.bpm != null ? (
                <div className="attribute-pill" title="Best-effort estimate">
                  <span className="attribute-label">Tempo</span>
                  <strong className="attribute-value">~{Math.round(selectedAsset.bpm)} BPM</strong>
                </div>
              ) : null}
              {selectedAsset.detected_key != null ? (
                <div className="attribute-pill" title="Best-effort musical key (Krumhansl-Schmuckler)">
                  <span className="attribute-label">Key</span>
                  <strong className="attribute-value">{selectedAsset.detected_key}</strong>
                </div>
              ) : null}
              {selectedAsset.musical_key != null ? (
                <div className="attribute-pill" title="Monophonic pitch estimate — best for single-source SFX, not full mixes">
                  <span className="attribute-label">Pitch</span>
                  <strong className="attribute-value">{selectedAsset.musical_key}</strong>
                </div>
              ) : null}
            </div>
            {assetInstruments.length > 0 ? (
              <div className="instrument-tag-row" aria-label="Detected instruments">
                {assetInstruments.map((entry) => (
                  <button
                    key={entry.name}
                    type="button"
                    className="instrument-tag"
                    title={`Detected instrument (${Math.round(entry.confidence * 100)}% confidence) — click to browse`}
                    onClick={() => setActiveFilter({ instrumentPage: false, instruments: [entry.name] })}
                  >
                    {entry.name}
                  </button>
                ))}
              </div>
            ) : null}
            <button type="button" className="primary-action" onClick={handleFindSimilar}>
              Find Similar Sounds
            </button>
            {similarStatus ? <div className="status-line">{similarStatus}</div> : null}
          </CollapsibleSection>
        ) : null}
        <CollapsibleSection id="source" title="Source & License" collapsed={collapsedSections.has("source")} onToggle={toggleSection}>
          {sourceDraft ? (
            <>
              <div className="source-fields">
                <input
                  placeholder="Provider"
                  value={sourceDraft.provider ?? ""}
                  onChange={(event) => setSourceDraft({ ...sourceDraft, provider: event.target.value || null })}
                />
                <input
                  placeholder="Source URL"
                  value={sourceDraft.source_url ?? ""}
                  onChange={(event) => setSourceDraft({ ...sourceDraft, source_url: event.target.value || null })}
                />
                <input
                  placeholder="License type"
                  value={sourceDraft.license_type ?? ""}
                  onChange={(event) => setSourceDraft({ ...sourceDraft, license_type: event.target.value || null })}
                />
                <input
                  placeholder="License status"
                  value={sourceDraft.license_status ?? ""}
                  onChange={(event) => setSourceDraft({ ...sourceDraft, license_status: event.target.value || null })}
                />
              </div>

              <div className="license-document">
                <div className="license-document-row">
                  {sourceDraft.license_document_path ? (
                    <>
                      <FileText size={14} />
                      <span className="license-document-name" title={sourceDraft.license_document_path}>
                        {sourceDraft.license_document_path.split("/").pop()}
                      </span>
                      <button type="button" className="text-button" onClick={handleRevealLicenseDocument}>
                        Reveal in {isMacPlatform ? "Finder" : "Explorer"}
                      </button>
                      <button type="button" className="text-button" onClick={handleRemoveLicenseDocument}>
                        Remove
                      </button>
                    </>
                  ) : (
                    <button type="button" className="text-button" onClick={handleAttachLicenseDocument}>
                      <Import size={14} />
                      Attach License PDF…
                    </button>
                  )}
                </div>

                <div className="license-dates">
                  <label>
                    Valid from
                    <input
                      type="date"
                      value={sourceDraft.license_valid_from ?? ""}
                      onChange={(event) =>
                        setSourceDraft({ ...sourceDraft, license_valid_from: event.target.value || null })
                      }
                    />
                  </label>
                  <label>
                    Valid until
                    <input
                      type="date"
                      value={sourceDraft.license_valid_until ?? ""}
                      onChange={(event) =>
                        setSourceDraft({
                          ...sourceDraft,
                          license_valid_until: event.target.value || null,
                          license_expiry_source: event.target.value ? "manual" : null
                        })
                      }
                    />
                  </label>
                </div>

                {licenseDateCandidates.length > 0 ? (
                  <div className="license-candidates">
                    <span className="license-candidates-label">Detected in the PDF:</span>
                    {licenseDateCandidates.map((candidate) => (
                      <button
                        type="button"
                        key={`${candidate.date}-${candidate.keyword}`}
                        className="license-candidate-chip"
                        title={candidate.context}
                        onClick={() => handleUseLicenseDateCandidate(candidate.date)}
                      >
                        Use {candidate.date} ({candidate.keyword})
                      </button>
                    ))}
                  </div>
                ) : null}

                {(() => {
                  const validity = licenseValidityStatus(sourceDraft.license_valid_until);
                  return (
                    <span className={`license-validity-badge license-validity-${validity.tone}`}>
                      {validity.label}
                    </span>
                  );
                })()}
                {sourceDraft.license_expiry_source === "extracted" ? (
                  <span className="license-expiry-hint">Detected from the PDF — please confirm</span>
                ) : null}
              </div>

              <button type="button" className="primary-action" onClick={handleSaveSource}>
                <Save size={15} />
                Save source
              </button>
            </>
          ) : (
            <span className="empty-hint">Select a sound to edit source and license</span>
          )}
          {licenseDocumentStatus ? <div className="status-line">{licenseDocumentStatus}</div> : null}
          {exportStatus ? <div className="status-line">{exportStatus}</div> : null}
        </CollapsibleSection>
        <CollapsibleSection id="maintenance" title="Maintenance" collapsed={collapsedSections.has("maintenance")} onToggle={toggleSection}>
          <div className="maintenance-list">
            {Object.entries(maintenanceLabels).map(([kind, label]) => (
              <div className="maintenance-row" key={kind}>
                <span>{label}</span>
                <strong>{maintenanceReport?.counts_by_kind[kind] ?? 0}</strong>
              </div>
            ))}
          </div>
          {maintenanceReport && maintenanceReport.findings.some((finding) => finding.kind === "DuplicateContent") ? (
            <div className="maintenance-list">
              {maintenanceReport.findings
                .filter((finding) => finding.kind === "DuplicateContent")
                .map((finding, index) => (
                  <div className="maintenance-row" key={index}>
                    <span>{finding.asset_ids.length} duplicate files sharing content</span>
                    <button type="button" className="text-button" onClick={() => handleTrashDuplicateGroup(finding.asset_ids)}>
                      Keep oldest, trash rest
                    </button>
                  </div>
                ))}
            </div>
          ) : null}
        </CollapsibleSection>
        <CollapsibleSection id="nas" title="NAS & Offline" collapsed={collapsedSections.has("nas")} onToggle={toggleSection}>
          {offlineControl && activeLibrary?.media_root ? (
            <>
              <div className="status-line">
                {offlineControl.media_root} — {formatMediaRootStatus(mediaRootStatus?.status)}
                {offlineControl.catalog_only ? " (catalog only)" : ""}
              </div>
              <div className="action-list">
                <button type="button" onClick={() => handleOfflineCommand("UseCatalogOnly")}>
                  Use Catalog Only
                </button>
                <button type="button" onClick={handleRetryReconnect}>
                  Retry Reconnect
                </button>
                <button
                  type="button"
                  onClick={() =>
                    handleOfflineCommand(offlineControl.validation_paused ? "ResumeValidation" : "PauseValidation")
                  }
                >
                  {offlineControl.validation_paused ? "Resume Validation" : "Pause Validation"}
                </button>
              </div>
              {reconnectStatus ? <div className="status-line">{reconnectStatus}</div> : null}
            </>
          ) : activeLibrary ? (
            <span className="empty-hint">No media root yet — import a folder to enable NAS/offline detection</span>
          ) : (
            <span className="empty-hint">No library selected</span>
          )}
        </CollapsibleSection>
        <CollapsibleSection id="backup" title="Backup" collapsed={collapsedSections.has("backup")} onToggle={toggleSection}>
          <button type="button" className="text-button" onClick={handleBackupLibrary} disabled={!activeLibraryId}>
            Back Up Library
          </button>
          <button type="button" className="text-button" onClick={handleRestoreLibrary}>
            Restore From Backup…
          </button>
          <div className="status-line">{backupStatus ?? "Saves a single .darkwavebak file — a snapshot of this library you choose where to keep"}</div>
        </CollapsibleSection>
        </div>
      </aside>
      <footer
        className={`transport${sidebarCollapsed ? " sidebar-collapsed" : ""}${inspectorCollapsed ? " inspector-collapsed" : ""}${isPlaying ? " is-playing" : ""}`}
        aria-label="Transport"
      >
        <button className="icon-button" aria-label="Previous" onClick={() => playRelative(-1)}>
          <SkipBack size={17} />
        </button>
        <button className="transport-play" aria-label="Play or pause" onClick={togglePlayback}>
          {isPlaying ? <Pause size={18} /> : <Play size={18} />}
        </button>
        <button className="icon-button" aria-label="Next" onClick={() => playRelative(1)}>
          <SkipForward size={17} />
        </button>
        <button
          className={looping ? "icon-button active" : "icon-button"}
          aria-label={looping ? "Disable loop" : "Enable loop"}
          aria-pressed={looping}
          title="Loop (L)"
          onClick={() => setLooping((previous) => !previous)}
        >
          <Repeat size={15} />
        </button>
        <div
          className="transport-waveform"
          role="slider"
          tabIndex={0}
          aria-label="Seek"
          aria-valuemin={0}
          aria-valuemax={Math.round(duration) || 0}
          aria-valuenow={Math.round(currentTime) || 0}
          onClick={(event) => {
            const audio = audioRef.current;
            if (!audio || !duration) return;
            const rect = event.currentTarget.getBoundingClientRect();
            const fraction = (event.clientX - rect.left) / rect.width;
            audio.currentTime = Math.max(0, Math.min(duration, fraction * duration));
          }}
          onKeyDown={(event) => {
            const audio = audioRef.current;
            if (!audio || !duration) return;
            const seekStepSeconds = event.shiftKey ? 15 : 5;
            if (event.key === "ArrowLeft") {
              event.preventDefault();
              audio.currentTime = Math.max(0, audio.currentTime - seekStepSeconds);
            } else if (event.key === "ArrowRight") {
              event.preventDefault();
              audio.currentTime = Math.min(duration, audio.currentTime + seekStepSeconds);
            }
          }}
        >
          <div className="waveform-track">
            {(peaks ?? []).map((peak, i) => (
              <span key={i} style={{ height: `${4 + peak * 96}%` }} />
            ))}
          </div>
          <div
            className="waveform-progress"
            style={{ clipPath: `inset(0 ${100 - (duration > 0 ? (currentTime / duration) * 100 : 0)}% 0 0)` }}
          >
            {(peaks ?? []).map((peak, i) => {
              const trailDistance = waveformActiveIndex - i;
              const isActive = isPlaying && trailDistance >= 0 && trailDistance < activeBarTrailLength;
              return (
                <span
                  key={i}
                  className={isActive ? "is-active" : undefined}
                  style={
                    isActive
                      ? ({ height: `${4 + peak * 96}%`, "--trail": trailDistance } as CSSProperties)
                      : { height: `${4 + peak * 96}%` }
                  }
                />
              );
            })}
          </div>
        </div>
        <span className="time">
          {formatTime(currentTime)} / {formatTime(duration)}
        </span>
        <button
          type="button"
          className={drTargetProject ? "dr-button" : "dr-button disabled"}
          aria-label={
            drTargetProject
              ? projectExportFolderSummary(drTargetProject)
              : "Send selected to a project's sound/SFX folder — click a project's send button first to set the target"
          }
          title={
            drTargetProject
              ? projectExportFolderSummary(drTargetProject)
              : "No send target yet — click a project's send button in the sidebar first"
          }
          disabled={!drTargetProject || !drTargetAssetId}
          onClick={() => {
            if (drTargetProject && drTargetAssetId) handleExportToProject(drTargetProject, [drTargetAssetId]);
          }}
        >
          <Clapperboard size={16} />
        </button>
      </footer>
      <AnimatePresence>
      {settingsOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setSettingsOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card settings-modal"
            onClick={(event) => event.stopPropagation()}
            aria-label="Settings"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Settings</h1>
              <button type="button" className="icon-button" aria-label="Close settings" onClick={() => setSettingsOpen(false)}>
                <X size={16} />
              </button>
            </div>

            <div className="settings-body">
              <nav className="settings-nav" aria-label="Settings categories">
                {SETTINGS_CATEGORIES.map((category) => (
                  <button
                    key={category.id}
                    type="button"
                    className={settingsCategory === category.id ? "settings-nav-item active" : "settings-nav-item"}
                    onClick={() => setSettingsCategory(category.id)}
                  >
                    <category.icon size={15} />
                    {category.label}
                  </button>
                ))}
              </nav>

              <div className="settings-content">
                {settingsCategory === "general" ? (
                  <>
                    <div className="settings-section">
                      <h2>Library</h2>
                      <div className="settings-grid">
                        <div className="settings-row">
                          <SlidersHorizontal size={14} />
                          <span>Name</span>
                          <strong>{activeLibrary?.name ?? "—"}</strong>
                        </div>
                        <div className="settings-row">
                          <Volume2 size={14} />
                          <span>Media root</span>
                          <strong className="settings-value-path" title={activeLibrary?.media_root || undefined}>
                            {activeLibrary?.media_root || "Not set yet — import a folder"}
                          </strong>
                        </div>
                        <div className="settings-row">
                          <Import size={14} />
                          <span>Import folder</span>
                          <div className="settings-row-value">
                            <strong className="settings-value-path" title={activeLibrary?.import_root || undefined}>
                              {activeLibrary?.import_root || "Not set — new sounds aren't auto-imported"}
                            </strong>
                            <button type="button" className="text-button" onClick={handleChangeLibraryImportRoot}>
                              {activeLibrary?.import_root ? "Change" : "Set"}
                            </button>
                            {activeLibrary?.import_root ? (
                              <button type="button" className="text-button" onClick={handleClearLibraryImportRoot}>
                                Clear
                              </button>
                            ) : null}
                          </div>
                        </div>
                        <div className="settings-row">
                          <Gauge size={14} />
                          <span>Status</span>
                          <strong>{formatMediaRootStatus(mediaRootStatus?.status)}</strong>
                        </div>
                        <div className="settings-row">
                          <FolderOpen size={14} />
                          <span>Import subfolders</span>
                          <div className="settings-row-value">
                            {(activeLibrary?.import_subfolders ?? []).length === 0 ? (
                              <span className="settings-inline-hint">
                                None — a file dropped on the window goes straight to the media root
                              </span>
                            ) : (
                              (activeLibrary?.import_subfolders ?? []).map((name) => (
                                <span key={name} className="import-subfolder-chip">
                                  {name}
                                  <button
                                    type="button"
                                    aria-label={`Remove ${name}`}
                                    onClick={() => handleRemoveImportSubfolder(name)}
                                  >
                                    <X size={11} />
                                  </button>
                                </span>
                              ))
                            )}
                          </div>
                        </div>
                        <div className="settings-row">
                          <Plus size={14} />
                          <span>Add subfolder</span>
                          <div className="settings-row-value">
                            <input
                              placeholder="e.g. Soundtrack"
                              value={newImportSubfolderName}
                              onChange={(event) => setNewImportSubfolderName(event.target.value)}
                              onKeyDown={(event) => {
                                if (event.key === "Enter") handleAddImportSubfolder();
                              }}
                            />
                            <button
                              type="button"
                              className="text-button"
                              disabled={!newImportSubfolderName.trim()}
                              onClick={handleAddImportSubfolder}
                            >
                              Add
                            </button>
                          </div>
                        </div>
                      </div>
                      <p className="settings-hint">
                        With 2 or more subfolders, dropping a file onto the window asks which one it
                        goes into — with a "remember for this session" option. 0 or 1 means it never
                        asks.
                      </p>
                    </div>

                    <div className="settings-section">
                      <div className="settings-section-head">
                        <h2>Manage Libraries</h2>
                        <button type="button" className="text-button" onClick={() => setCreateLibraryModalOpen(true)}>
                          <Plus size={13} />
                          New Library
                        </button>
                      </div>
                      <p className="settings-hint">
                        Deleting a library or emptying its trash only removes Darkwave's own catalog records — tags,
                        collections, source/license notes, trash entries. The audio files at each library's media
                        root are never touched.
                      </p>
                      <div className="library-admin-list">
                        {libraries.map((library) => (
                          <div className="library-admin-row" key={library.id}>
                            <span className="library-admin-icon">
                              <Database size={14} />
                            </span>
                            <div className="library-admin-meta">
                              <strong>{library.name}</strong>
                              <small title={library.media_root || undefined}>
                                {library.media_root || "No media root yet — import a folder to set it"}
                              </small>
                            </div>
                            <div className="library-admin-actions">
                              <button type="button" className="text-button" onClick={() => handleCleanLibraryCache(library)}>
                                Clean Cache
                              </button>
                              <button type="button" className="text-button" onClick={() => handleEmptyLibraryTrash(library)}>
                                Empty Trash
                              </button>
                              <button
                                type="button"
                                className="text-button danger"
                                onClick={() => handleDeleteLibrary(library)}
                              >
                                <Trash2 size={13} />
                                Delete
                              </button>
                            </div>
                          </div>
                        ))}
                      </div>
                      {libraryAdminStatus ? <div className="status-line">{libraryAdminStatus}</div> : null}
                    </div>

                    <div className="settings-section">
                      <h2>Watched Folders</h2>
                      <div className="settings-grid">
                        <div className="status-line">
                          New, stable files dropped into any folder below import automatically (as referenced
                          files) into {activeLibrary?.name ?? "the active library"}, checked roughly every 20
                          seconds by the background worker. A folder's type is a hint for classifying files that
                          don't already say what they are by name — it never overrides a file whose name or
                          embedded metadata is already specific.
                        </div>
                        {libraryFolders.filter((folder) => folder.kind !== "media_root").length > 0 ? (
                          <ul className="setup-folder-list">
                            {libraryFolders
                              .filter((folder) => folder.kind !== "media_root")
                              .map((folder) => (
                                <li key={folder.id} className="setup-folder-row">
                                  <select
                                    value={folder.role ?? ""}
                                    onChange={(event) => handleSetWatchedFolderRole(folder.id, event.target.value)}
                                  >
                                    {FOLDER_ROLE_OPTIONS.map((option) => (
                                      <option key={option.value} value={option.value}>
                                        {option.label}
                                      </option>
                                    ))}
                                  </select>
                                  <span className="setup-folder-path" title={folder.path}>
                                    {folder.path}
                                  </span>
                                  <button
                                    type="button"
                                    className="text-button"
                                    onClick={() => handleRemoveWatchedFolder(folder.id)}
                                  >
                                    Stop Watching
                                  </button>
                                </li>
                              ))}
                          </ul>
                        ) : null}
                        <button
                          type="button"
                          className="text-button setup-add-folder-button"
                          onClick={handleAddWatchedFolder}
                          disabled={!activeLibraryId}
                        >
                          + Add Folder…
                        </button>
                      </div>
                    </div>

                    <div className="settings-section">
                      <h2>Background Analysis</h2>
                      <div className="settings-grid">
                        <label className="settings-row">
                          <Coffee size={14} />
                          <span>Keep the computer awake while analyzing</span>
                          <input
                            type="checkbox"
                            checked={preferences?.prevent_sleep_during_analysis ?? true}
                            onChange={handleToggleSleepPrevention}
                          />
                        </label>
                        <div className="status-line">
                          Prevents idle system sleep (not display sleep) while audio analysis,
                          waveform generation, or instrument detection jobs are pending and not
                          paused — so a large overnight batch finishes instead of being cut short.
                        </div>
                      </div>
                    </div>
                  </>
                ) : null}

                {settingsCategory === "playback" ? (
                  <div className="settings-section">
                    <h2>Playback</h2>
                    <div className="settings-grid">
                      <label className="settings-row">
                        <SlidersHorizontal size={14} />
                        <span>Browser density</span>
                        <select
                          value={preferences?.browser_density ?? "Comfortable"}
                          onChange={(event) => {
                            setPreferences((previous) => {
                              if (!previous) return previous;
                              const next = {
                                ...previous,
                                browser_density: event.target.value as AppPreferences["browser_density"]
                              };
                              invoke("save_app_preferences", { preferences: next }).catch(() => {});
                              return next;
                            });
                          }}
                        >
                          <option value="Compact">Compact</option>
                          <option value="Comfortable">Comfortable</option>
                          <option value="Expanded">Expanded</option>
                        </select>
                      </label>
                      <div className="settings-row">
                        <Volume2 size={14} />
                        <span>Output route</span>
                        <strong>{preferences?.output_device === "SystemDefault" ? "System default" : "Custom device"}</strong>
                      </div>
                      <div className="status-line">
                        Output device selection isn't enforced yet on macOS — WKWebView doesn't
                        support routing audio to a specific device.
                      </div>
                    </div>
                  </div>
                ) : null}

                {settingsCategory === "storage" ? (
                  <>
                    <div className="settings-section">
                      <h2>Cache</h2>
                      <div className="settings-grid">
                        <label className="settings-row">
                          <Gauge size={14} />
                          <span>Local playback cache limit (MB)</span>
                          <input
                            type="number"
                            min={64}
                            step={64}
                            value={preferences?.preview_cache_limit_mb ?? 0}
                            onChange={(event) => {
                              const value = Number(event.target.value);
                              setPreferences((previous) => {
                                if (!previous || Number.isNaN(value)) return previous;
                                const next = { ...previous, preview_cache_limit_mb: value };
                                invoke("save_app_preferences", { preferences: next }).catch(() => {});
                                return next;
                              });
                            }}
                          />
                        </label>
                        <div className="settings-row">
                          <RefreshCw size={14} />
                          <span>{cacheStatus ?? "Cache warms automatically while browsing"}</span>
                          <button type="button" className="text-button" onClick={handlePurgeCache}>
                            Purge Cache
                          </button>
                        </div>
                      </div>
                    </div>

                    <div className="settings-section">
                      <h2>Trash</h2>
                      <div className="status-line">{trashRetentionDays} day retention before explicit purge</div>
                      {trashItems.length === 0 ? (
                        <span className="empty-hint">Trash is empty</span>
                      ) : (
                        <div className="maintenance-list">
                          {trashItems.map((item) => (
                            <div className="maintenance-row" key={item.asset_id}>
                              <span>{item.original_path.split(/[/\\]/).pop()}</span>
                              <button type="button" className="text-button" onClick={() => handleRestoreFromTrash(item)}>
                                Restore
                              </button>
                              <button
                                type="button"
                                className="text-button danger"
                                onClick={() => handleDeleteTrashItemPermanently(item)}
                              >
                                Delete
                              </button>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  </>
                ) : null}

                {settingsCategory === "appearance" ? (
                  <div className="settings-section">
                    <h2>Appearance</h2>
                    <div className="settings-grid">
                      <label className="settings-row">
                        <Palette size={14} />
                        <span>Theme</span>
                        <select
                          value={preferences?.theme ?? "Dark"}
                          onChange={(event) => handleSetTheme(event.target.value as AppPreferences["theme"])}
                        >
                          <option value="Dark">Dark</option>
                          <option value="Light">Light</option>
                          <option value="System">Match system</option>
                        </select>
                      </label>
                    </div>

                    <h2 className="settings-subhead">Player accent colors</h2>
                    <div className="accent-legend">
                      {PLAYER_ACCENT_LEGEND.map((entry) => (
                        <div className="accent-legend-row" key={entry.mood}>
                          <span
                            className="accent-legend-swatch"
                            style={{
                              background:
                                entry.mood === "default"
                                  ? "linear-gradient(135deg, #ff7940, #f14800)"
                                  : `linear-gradient(135deg, ${playerMoodTheme[entry.mood].from}, ${playerMoodTheme[entry.mood].to})`
                            }}
                          />
                          <strong>{entry.label}</strong>
                          <span>{entry.description}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                ) : null}

                {settingsCategory === "accessibility" ? (
                  <div className="settings-section">
                    <h2>Accessibility</h2>
                    <div className="settings-grid">
                      <label className="settings-row">
                        <Contrast size={14} />
                        <span>Reduced transparency</span>
                        <input
                          type="checkbox"
                          checked={preferences?.reduced_transparency ?? false}
                          onChange={handleToggleReducedTransparency}
                        />
                      </label>
                      <label className="settings-row">
                        <Zap size={14} />
                        <span>Reduced motion</span>
                        <input
                          type="checkbox"
                          checked={preferences?.reduced_motion ?? false}
                          onChange={handleToggleReducedMotion}
                        />
                      </label>
                    </div>
                  </div>
                ) : null}

                {settingsCategory === "release" ? (
                  <div className="settings-section">
                    <h2>Release Readiness</h2>
                    <div className="release-grid">
                      {releaseItems.map((item) => (
                        <div className="release-item" key={item.label}>
                          <span>{item.label}</span>
                          <mark className={item.state === "Passed" ? "passed" : "planned"}>{item.state}</mark>
                        </div>
                      ))}
                    </div>
                    <div className="status-line">
                      <ShieldCheck size={18} />
                      Distribution gates tracked
                    </div>
                    <div className="status-line">
                      <Bell size={18} />
                      {updateChannelState === "Passed" ? "Update channel ready" : "Update channel planned"}
                    </div>
                  </div>
                ) : null}

                {settingsCategory === "maintenance" ? (
                  <div className="settings-section">
                    <h2>Maintenance</h2>
                    <div className="maintenance-list">
                      {Object.entries(maintenanceLabels).map(([kind, label]) => (
                        <div className="maintenance-row" key={kind}>
                          <span>{label}</span>
                          <strong>{maintenanceReport?.counts_by_kind[kind] ?? 0}</strong>
                          {kind === "MissingMedia" ? (
                            <button
                              type="button"
                              className="text-button"
                              onClick={() => {
                                setActiveFilter("missing");
                                setSettingsOpen(false);
                              }}
                            >
                              View in Browser
                            </button>
                          ) : null}
                        </div>
                      ))}
                    </div>
                    {maintenanceReport && maintenanceReport.findings.some((finding) => finding.kind === "DuplicateContent") ? (
                      <div className="maintenance-list">
                        {maintenanceReport.findings
                          .filter((finding) => finding.kind === "DuplicateContent")
                          .map((finding, index) => (
                            <div className="maintenance-row" key={index}>
                              <span>{finding.asset_ids.length} duplicate files sharing content</span>
                              <button type="button" className="text-button" onClick={() => handleTrashDuplicateGroup(finding.asset_ids)}>
                                Keep oldest, trash rest
                              </button>
                            </div>
                          ))}
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </div>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {commandPaletteOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setCommandPaletteOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
        >
          <motion.div
            className="modal-card command-palette"
            onClick={(event) => event.stopPropagation()}
            aria-label="Command palette"
            initial={{ opacity: 0, scale: 0.97, y: -8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.97, y: -8 }}
            transition={{ duration: 0.16, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="command-palette-input-row">
              <Search size={15} />
              <input
                ref={commandPaletteInputRef}
                value={commandPaletteQuery}
                onChange={(event) => setCommandPaletteQuery(event.target.value)}
                placeholder="Type a command…"
                aria-label="Command palette search"
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    setCommandPaletteOpen(false);
                  } else if (event.key === "ArrowDown") {
                    event.preventDefault();
                    setCommandPaletteActiveIndex((previous) => Math.min(previous + 1, commandPaletteResults.length - 1));
                  } else if (event.key === "ArrowUp") {
                    event.preventDefault();
                    setCommandPaletteActiveIndex((previous) => Math.max(previous - 1, 0));
                  } else if (event.key === "Enter") {
                    event.preventDefault();
                    const command = commandPaletteResults[commandPaletteActiveIndex];
                    if (command) executeCommand(command.id);
                  }
                }}
              />
              <kbd>Esc</kbd>
            </div>
            <div className="command-palette-list">
              {commandPaletteResults.length === 0 ? (
                <p className="command-palette-empty">No matching commands</p>
              ) : (
                commandPaletteResults.map((command, index) => (
                  <button
                    type="button"
                    key={command.id}
                    className={index === commandPaletteActiveIndex ? "command-palette-row active" : "command-palette-row"}
                    onMouseEnter={() => setCommandPaletteActiveIndex(index)}
                    onClick={() => executeCommand(command.id)}
                  >
                    <span>{command.title}</span>
                    <small>{command.category}</small>
                  </button>
                ))
              )}
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {backgroundActivityOpen ? (
        <motion.div
          className="modal-overlay activity-modal-overlay"
          onClick={() => setBackgroundActivityOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
        >
          <motion.div
            className="modal-card activity-modal"
            onClick={(event) => event.stopPropagation()}
            aria-label="Background activity"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Background Activity</h1>
              <button
                type="button"
                className="icon-button"
                aria-label="Close background activity"
                onClick={() => setBackgroundActivityOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            {(() => {
              const syncingProjects = collections.filter(
                (project) => resolveSyncStatuses[project.id]?.status === "syncing"
              );
              const failedSyncProjects = collections.filter(
                (project) => resolveSyncStatuses[project.id]?.status === "failed"
              );
              if (
                jobProgress.length === 0 &&
                !refreshStatus &&
                !importStatus &&
                !audioAnalysisPaused &&
                !waveformGenerationPaused &&
                syncingProjects.length === 0 &&
                failedSyncProjects.length === 0 &&
                Object.keys(jobCompletionSummaries).length === 0
              ) {
                return <p className="empty-hint activity-empty-hint">All caught up — nothing running right now.</p>;
              }
              return (
                <div className="job-progress-panel" aria-label="Background work">
                  {syncingProjects.map((project) => (
                    <div className="job-progress-row" key={`resolve-sync-${project.id}`}>
                      <span className="job-progress-icon">
                        <Workflow size={15} />
                      </span>
                      <div className="job-progress-body">
                        <div className="job-progress-head">
                          <span className="job-progress-label">Resolve Sync</span>
                        </div>
                        <div className="status-line">Syncing "{project.name}" structure to Resolve…</div>
                      </div>
                    </div>
                  ))}
                  {failedSyncProjects.map((project) => (
                    <div className="job-progress-row" key={`resolve-sync-failed-${project.id}`}>
                      <span className="job-progress-icon">
                        <Workflow size={15} />
                      </span>
                      <div className="job-progress-body">
                        <div className="job-progress-head">
                          <span className="job-progress-label">Resolve Sync</span>
                        </div>
                        <div className="status-line job-progress-failed">
                          "{project.name}": {resolveSyncStatuses[project.id]?.error ?? "sync failed"}
                        </div>
                      </div>
                    </div>
                  ))}
                  {importStatus ? (
                  <div className="job-progress-row">
                    <span className="job-progress-icon">
                      <Import size={15} />
                    </span>
                    <div className="job-progress-body">
                      <div className="job-progress-head">
                        <span className="job-progress-label">Import</span>
                      </div>
                      <div className="status-line">{importStatus}</div>
                    </div>
                  </div>
                ) : null}
                {refreshStatus ? (
                  <div className="job-progress-row">
                    <span className="job-progress-icon">
                      <RefreshCw size={15} />
                    </span>
                    <div className="job-progress-body">
                      <div className="job-progress-head">
                        <span className="job-progress-label">Library Sync</span>
                      </div>
                      <div className="status-line">{refreshStatus}</div>
                    </div>
                  </div>
                ) : null}
                {jobProgress.map((job) => {
                  const percent = job.total > 0 ? Math.round(((job.total - job.pending) / job.total) * 100) : 0;
                  return (
                    <div className="job-progress-row" key={job.kind}>
                      <span className="job-progress-icon">
                        <Activity size={15} />
                      </span>
                      <div className="job-progress-body">
                        <div className="job-progress-head">
                          <span className="job-progress-label">{job.label}</span>
                          <span className="job-progress-count">
                            {job.total - job.pending}/{job.total} · {percent}%
                            {job.failed > 0 ? <span className="job-progress-failed"> · {job.failed} failed</span> : null}
                          </span>
                        </div>
                        <div className="job-progress-track">
                          <div className="job-progress-fill" style={{ width: `${percent}%` }} />
                        </div>
                        {job.currentFile && job.pending > 0 ? (
                          <div className="job-progress-current-file" title={job.currentFile}>
                            {job.currentFile}
                          </div>
                        ) : null}
                      </div>
                      {job.kind === "audio_analysis" ? (
                        <button
                          type="button"
                          className="icon-button"
                          aria-label="Pause audio analysis"
                          title="Pause audio analysis — queued work stays queued, nothing is lost"
                          onClick={handleToggleAudioAnalysisPaused}
                        >
                          <Pause size={15} />
                        </button>
                      ) : job.kind === "waveform_generation" ? (
                        <button
                          type="button"
                          className="icon-button"
                          aria-label="Pause waveform generation"
                          title="Pause waveform generation — queued work stays queued, nothing is lost"
                          onClick={handleToggleWaveformGenerationPaused}
                        >
                          <Pause size={15} />
                        </button>
                      ) : null}
                    </div>
                  );
                })}
                {audioAnalysisPaused ? (
                  <div className="job-progress-row">
                    <span className="job-progress-icon">
                      <Pause size={15} />
                    </span>
                    <div className="job-progress-body">
                      <div className="job-progress-head">
                        <span className="job-progress-label">Audio analysis paused</span>
                      </div>
                      <div className="status-line">Queued files stay queued — nothing is lost, just deferred.</div>
                    </div>
                    <button
                      type="button"
                      className="icon-button"
                      aria-label="Resume audio analysis"
                      title="Resume audio analysis"
                      onClick={handleToggleAudioAnalysisPaused}
                    >
                      <Play size={15} />
                    </button>
                  </div>
                ) : null}
                {waveformGenerationPaused ? (
                  <div className="job-progress-row">
                    <span className="job-progress-icon">
                      <Pause size={15} />
                    </span>
                    <div className="job-progress-body">
                      <div className="job-progress-head">
                        <span className="job-progress-label">Waveform generation paused</span>
                      </div>
                      <div className="status-line">Queued files stay queued — nothing is lost, just deferred.</div>
                    </div>
                    <button
                      type="button"
                      className="icon-button"
                      aria-label="Resume waveform generation"
                      title="Resume waveform generation"
                      onClick={handleToggleWaveformGenerationPaused}
                    >
                      <Play size={15} />
                    </button>
                  </div>
                ) : null}
                {Object.values(jobCompletionSummaries).map((summary) => {
                  const { headline, detail } = describeJobCompletion(summary);
                  return (
                    <div className={summary.failed > 0 ? "job-progress-row job-progress-done warning" : "job-progress-row job-progress-done"} key={summary.kind}>
                      <span className="job-progress-icon">
                        {summary.failed > 0 ? <FileWarning size={15} /> : <ShieldCheck size={15} />}
                      </span>
                      <div className="job-progress-body">
                        <div className="job-progress-head">
                          <span className="job-progress-label">{headline}</span>
                        </div>
                        {detail ? <div className="status-line">{detail}</div> : null}
                      </div>
                      {summary.failed > 0 ? (
                        <button type="button" className="text-button" onClick={() => handleRetryFailedJobs(summary.kind)}>
                          Retry Now
                        </button>
                      ) : null}
                    </div>
                  );
                })}
                </div>
              );
            })()}
            <p className="settings-hint activity-footnote">
              Analysis runs in the background, one file at a time — browsing and playback stay responsive.
            </p>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {newProjectModalOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setNewProjectModalOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="New project"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>New Project</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setNewProjectModalOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <div className="settings-stack">
              <input
                autoFocus
                placeholder="Project name"
                value={newProjectName}
                onChange={(event) => setNewProjectName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && newProjectName.trim()) {
                    handleCreateProject();
                    setNewProjectModalOpen(false);
                  }
                }}
              />
              <label className="setup-field">
                <span>Sound folder</span>
                <div className="setup-field-row">
                  <input
                    placeholder="/Volumes/Edit/MyFilm/Sounds"
                    value={newProjectExportPath}
                    onChange={(event) => setNewProjectExportPath(event.target.value)}
                  />
                  <button type="button" onClick={handleChooseProjectExportPath}>
                    Browse
                  </button>
                </div>
                <p className="settings-hint">For music.</p>
              </label>
              <label className="setup-field">
                <span>Sound effects folder</span>
                <div className="setup-field-row">
                  <input
                    placeholder="/Volumes/Edit/MyFilm/SFX"
                    value={newProjectSfxExportPath}
                    onChange={(event) => setNewProjectSfxExportPath(event.target.value)}
                  />
                  <button type="button" onClick={handleChooseProjectSfxExportPath}>
                    Browse
                  </button>
                </div>
                <p className="settings-hint">Everything else — auto-sorted into subfolders.</p>
              </label>
              <div className="setup-field">
                <span>More export folders (optional)</span>
                <p className="settings-hint">
                  Add a folder for a specific category — a track already tagged that way exports here instead of
                  the sound/sound effects folders above.
                </p>
                {newProjectExportFolders.length > 0 ? (
                  <ul className="setup-folder-list">
                    {newProjectExportFolders.map((folder) => (
                      <li key={folder.id} className="setup-folder-row">
                        <select
                          value={folder.role}
                          onChange={(event) => handleSetNewProjectExportFolderRole(folder.id, event.target.value)}
                        >
                          {PROJECT_EXPORT_FOLDER_ROLE_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <span className="setup-folder-path" title={folder.path || undefined}>
                          {folder.path || "No folder chosen"}
                        </span>
                        <button type="button" onClick={() => handleBrowseNewProjectExportFolderRow(folder.id)}>
                          Browse
                        </button>
                        <button
                          type="button"
                          className="text-button"
                          onClick={() => handleRemoveNewProjectExportFolderRow(folder.id)}
                        >
                          Remove
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : null}
                <button
                  type="button"
                  className="text-button setup-add-folder-button"
                  onClick={handleAddNewProjectExportFolderRow}
                >
                  + Add Export Folder
                </button>
              </div>
              <button
                type="button"
                className="primary-action"
                disabled={!newProjectName.trim()}
                onClick={() => {
                  handleCreateProject();
                  setNewProjectModalOpen(false);
                }}
              >
                Create Project
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {externalDropPrompt ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setExternalDropPrompt(null)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Where should this go?"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Where should this go?</h1>
              <button
                type="button"
                className="icon-button"
                aria-label="Close"
                onClick={() => setExternalDropPrompt(null)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="settings-stack">
              <p className="settings-hint">
                {externalDropPrompt.paths.length === 1
                  ? "1 dropped item — pick a folder to import it into."
                  : `${externalDropPrompt.paths.length} dropped items — pick a folder to import them into.`}
              </p>
              <div className="drop-target-grid">
                {externalDropPrompt.subfolders.map((subfolder) => (
                  <button
                    key={subfolder}
                    type="button"
                    onClick={() => handleConfirmExternalDropDestination(subfolder)}
                  >
                    {subfolder}
                  </button>
                ))}
              </div>
              <label className="drop-remember-row">
                <input
                  type="checkbox"
                  checked={externalDropRememberChoice}
                  onChange={(event) => setExternalDropRememberChoice(event.target.checked)}
                />
                <span>Remember my choice for this session</span>
              </label>
              <p className="settings-hint">You'll be asked again the next time the app opens.</p>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {editProjectId ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setEditProjectId(null)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Edit project"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Edit Project</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setEditProjectId(null)}>
                <X size={16} />
              </button>
            </div>
            <div className="settings-stack">
              <input
                autoFocus
                placeholder="Project name"
                value={editProjectName}
                onChange={(event) => setEditProjectName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && editProjectName.trim()) handleSaveProjectEdits();
                }}
              />
              <label className="setup-field">
                <span>Sound folder</span>
                <div className="setup-field-row">
                  <input
                    placeholder="/Volumes/Edit/MyFilm/Sounds"
                    value={editProjectExportPath}
                    onChange={(event) => setEditProjectExportPath(event.target.value)}
                  />
                  <button type="button" onClick={handleChooseEditProjectExportPath}>
                    Browse
                  </button>
                </div>
                <p className="settings-hint">For music.</p>
              </label>
              <label className="setup-field">
                <span>Sound effects folder</span>
                <div className="setup-field-row">
                  <input
                    placeholder="/Volumes/Edit/MyFilm/SFX"
                    value={editProjectSfxExportPath}
                    onChange={(event) => setEditProjectSfxExportPath(event.target.value)}
                  />
                  <button type="button" onClick={handleChooseEditProjectSfxExportPath}>
                    Browse
                  </button>
                </div>
                <p className="settings-hint">Everything else — auto-sorted into subfolders.</p>
              </label>
              <div className="setup-field">
                <span>More export folders</span>
                <p className="settings-hint">
                  Add a folder for a specific category — a track already tagged that way exports here instead of
                  the sound/sound effects folders above. Changes here apply immediately.
                </p>
                {editProjectExportFolders.length > 0 ? (
                  <ul className="setup-folder-list">
                    {editProjectExportFolders.map((folder) => (
                      <li key={folder.id} className="setup-folder-row">
                        <select
                          value={folder.role}
                          onChange={(event) => handleSetEditProjectExportFolderRole(folder.id, event.target.value)}
                        >
                          {PROJECT_EXPORT_FOLDER_ROLE_OPTIONS.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))}
                        </select>
                        <span className="setup-folder-path" title={folder.path}>
                          {folder.path}
                        </span>
                        <button
                          type="button"
                          className="text-button"
                          onClick={() => handleRemoveEditProjectExportFolder(folder.id)}
                        >
                          Remove
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : null}
                <button
                  type="button"
                  className="text-button setup-add-folder-button"
                  onClick={handleAddEditProjectExportFolder}
                >
                  + Add Export Folder…
                </button>
              </div>
              <button
                type="button"
                className="primary-action"
                disabled={!editProjectName.trim()}
                onClick={handleSaveProjectEdits}
              >
                Save Changes
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {createLibraryModalOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setCreateLibraryModalOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Create library"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>New Library</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setCreateLibraryModalOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <div className="settings-stack">
              <label className="setup-field">
                <span>Library name</span>
                <input
                  autoFocus
                  value={libraryName}
                  onChange={(event) => setLibraryName(event.target.value)}
                  placeholder="Home Studio"
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && libraryName.trim()) handleCreateLibrary();
                  }}
                />
              </label>
              <p className="settings-hint">The first folder you import becomes this library's media location automatically.</p>
              <button
                className="primary-action"
                type="button"
                onClick={handleCreateLibrary}
                disabled={!libraryName.trim()}
              >
                Create Library
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {smartCollectionModalOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setSmartCollectionModalOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Save as smart collection"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Save as Smart Collection</h1>
              <button
                type="button"
                className="icon-button"
                aria-label="Close"
                onClick={() => setSmartCollectionModalOpen(false)}
              >
                <X size={16} />
              </button>
            </div>
            <div className="settings-stack">
              <div className="status-line">
                Saves the current search text and range filters as a live-updating collection
                in the sidebar — it re-runs the filter each time you open it, rather than storing
                a fixed list of sounds.
              </div>
              <input
                autoFocus
                placeholder="Smart collection name"
                value={smartCollectionName}
                onChange={(event) => setSmartCollectionName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && smartCollectionName.trim()) {
                    handleCreateSmartCollection();
                    setSmartCollectionModalOpen(false);
                  }
                }}
              />
              <button
                type="button"
                className="primary-action"
                disabled={!smartCollectionName.trim()}
                onClick={() => {
                  handleCreateSmartCollection();
                  setSmartCollectionModalOpen(false);
                }}
              >
                Save Smart Collection
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {shortcutsOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setShortcutsOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Keyboard shortcuts"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Keyboard Shortcuts</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setShortcutsOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <div className="shortcut-list">
              {(preferences?.shortcuts.bindings ?? []).map((item) => (
                <div className="shortcut-row" key={`${item.command}-${item.accelerator}`}>
                  <span>{item.command}</span>
                  <kbd>{item.accelerator}</kbd>
                </div>
              ))}
              <div className="shortcut-row">
                <span>Undo</span>
                <kbd>Mod+Z</kbd>
              </div>
              <div className="shortcut-row">
                <span>Redo</span>
                <kbd>Mod+Shift+Z</kbd>
              </div>
              <div className="shortcut-row">
                <span>Select All Visible</span>
                <kbd>Mod+A</kbd>
              </div>
              <div className="shortcut-row">
                <span>Keyboard Shortcuts</span>
                <kbd>Mod+/</kbd>
              </div>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {licenseFormOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setLicenseFormOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Activate license"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Activate License</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setLicenseFormOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <div className="settings-grid">
              <label className="setup-field">
                <span>License key</span>
                <input
                  autoFocus
                  value={licenseKeyInput}
                  onChange={(event) => setLicenseKeyInput(event.target.value)}
                  placeholder="DW-XXXXXXXX-XXXXXXXX-XXXXXXXX"
                />
              </label>
              <label className="setup-field">
                <span>Email used at purchase</span>
                <input
                  value={licenseEmailInput}
                  onChange={(event) => setLicenseEmailInput(event.target.value)}
                  placeholder="you@example.com"
                />
              </label>
              {licenseFormError ? <p className="setup-error">{licenseFormError}</p> : null}
              <button
                className="primary-action"
                type="button"
                onClick={handleActivateLicense}
                disabled={!licenseKeyInput.trim() || !licenseEmailInput.trim() || licenseFormBusy}
              >
                {licenseFormBusy ? "Activating…" : "Activate"}
              </button>
              <button
                className="text-button"
                type="button"
                onClick={handleRecoverLicenseKey}
                disabled={!licenseEmailInput.trim() || licenseFormBusy}
              >
                Resend my license key
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
      {trashModalOpen ? (
        <motion.div
          className="modal-overlay"
          onClick={() => setTrashModalOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <motion.div
            className="modal-card"
            onClick={(event) => event.stopPropagation()}
            aria-label="Trash"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 10 }}
            transition={{ duration: 0.2, ease: [0.4, 0, 0.2, 1] }}
          >
            <div className="modal-head">
              <h1>Trash</h1>
              <button type="button" className="icon-button" aria-label="Close" onClick={() => setTrashModalOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <div className="status-line">{trashRetentionDays} day retention before automatic purge</div>
            {trashItems.length === 0 ? (
              <p className="empty-hint">Trash is empty</p>
            ) : (
              <div className="maintenance-list">
                {trashItems.map((item) => (
                  <div className="maintenance-row" key={item.asset_id}>
                    <span>{item.original_path.split(/[/\\]/).pop()}</span>
                    <button type="button" className="text-button" onClick={() => handleRestoreFromTrash(item)}>
                      Restore
                    </button>
                    <button
                      type="button"
                      className="text-button danger"
                      onClick={() => handleDeleteTrashItemPermanently(item)}
                    >
                      Delete
                    </button>
                  </div>
                ))}
              </div>
            )}
          </motion.div>
        </motion.div>
      ) : null}
      </AnimatePresence>
      <AnimatePresence>
        {dragPreview ? (
          <motion.div
            className="drag-preview"
            style={{ left: dragPreviewLeft, top: dragPreviewTop }}
            initial={{ opacity: 0, scale: 0.3, borderRadius: 10 }}
            animate={{ opacity: 1, scale: 1, borderRadius: 18 }}
            exit={{ opacity: 0, scale: 0.4 }}
            transition={{ type: "spring", stiffness: 520, damping: 30, mass: 0.6 }}
            aria-hidden="true"
          >
            <Music2 size={20} />
            {dragPreview.count > 1 ? <span className="drag-preview-count">{dragPreview.count}</span> : null}
          </motion.div>
        ) : null}
      </AnimatePresence>
      {/* Drag dock — pops up the moment a drag activates, fanning out the
          up-to-4 most recently used projects as big one-hop drop targets.
          It's positioned independently of either Projects list, so a track
          can always be dropped even when the sidebar's section is
          collapsed, scrolled away, or the inspector is closed. The anchor
          strip spans the full width so the dock can stay centered without
          a static CSS transform (which motion's own x/y/scale animation on
          the card below would otherwise clobber); pointer-events are off
          on the strip and back on for the card itself so it doesn't
          swallow drop hit-testing in the empty margins beside it. */}
      <AnimatePresence>
        {dragPreview && dragDockProjects.length > 0 ? (
          <div className="drag-dock-anchor" aria-hidden="true">
            <motion.div
              className="drag-dock"
              initial={{ opacity: 0, y: 26, scale: 0.92 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, y: 18, scale: 0.92 }}
              transition={{ type: "spring", stiffness: 360, damping: 30, mass: 0.7 }}
            >
              <div className="drag-dock-cards">
                {dragDockProjects.map((project, index) => {
                  const isOver = dragOverProjectId === project.id;
                  return (
                    <motion.div
                      key={project.id}
                      className={isOver ? "drag-dock-card drag-over" : "drag-dock-card"}
                      data-drop-project-id={project.id}
                      title={project.name}
                      initial={{ opacity: 0, y: 16, scale: 0.8 }}
                      animate={{ opacity: 1, y: 0, scale: isOver ? 1.08 : 1 }}
                      transition={{
                        type: "spring",
                        stiffness: 460,
                        damping: 24,
                        delay: index * 0.05
                      }}
                    >
                      <span className="drag-dock-card-icon">
                        <Clapperboard size={20} />
                      </span>
                      <span className="drag-dock-card-name">{project.name}</span>
                    </motion.div>
                  );
                })}
              </div>
            </motion.div>
          </div>
        ) : null}
      </AnimatePresence>
    </main>
  );
}
