import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "motion/react";
import { open as openDialog, save as saveDialog, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { writeText as writeClipboardText } from "@tauri-apps/plugin-clipboard-manager";
import {
  Activity,
  Bell,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clapperboard,
  Contrast,
  Copy,
  Database,
  Eye,
  FileWarning,
  Flag,
  FolderOpen,
  Gauge,
  HardDrive,
  Import,
  Library,
  Link2,
  ListFilter,
  Mic,
  MicOff,
  Music,
  Music2,
  Palette,
  Pause,
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
  Star,
  Trash2,
  Volume2,
  Waves,
  Wind,
  Workflow,
  X,
  Zap
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent, type ReactNode } from "react";

type LibraryRecord = {
  id: string;
  name: string;
  media_root: string;
};

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
  /** Best-effort detected pitch note name (e.g. "A4"), not a musical key. */
  musical_key: string | null;
  key_confidence: number | null;
  /** Fraction of the clip classified as speech (Silero VAD). */
  vocal_ratio: number | null;
};

type ImportFailure = {
  filename: string;
  reason: string;
};

type ImportFolderResult = {
  imported: AssetRecord[];
  failed: ImportFailure[];
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

type BrowserCommand =
  | { MoveSelection: { delta: number } }
  | { FocusRow: { index: number } }
  | { SelectFocused: { mode: SelectionMode } }
  | "SelectAllVisible";

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
};

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
};

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
  watched_folder_path: string | null;
  watched_folder_library_id: string | null;
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
  | "has_vocals"
  | "instrumental"
  | "has_tempo"
  | "has_pitch"
  | { favoritesCategory: SoundCategory }
  | { unreviewedCategory: SoundCategory }
  | { project: string; smart?: boolean }
  | { tag: string };

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

/** Every format the real Symphonia-backed decoder accepts (see Exporting
 * docs) — a format filter that only covered WAV/MP3 would silently miss
 * whatever fraction of the library came in as FLAC, AAC, M4A, OGG, or AIFF. */
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
  manifest_revision: number;
  media_root: string;
  catalog_snapshot_path: string;
  manifest_path: string;
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

const activeBarTrailLength = 10;

async function computePeaks(path: string, bucketCount = 200): Promise<number[] | null> {
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
    audioContext.close();
    return peaks;
  } catch {
    return null;
  }
}

type ReleaseReadinessItem = {
  label: string;
  blocker: string;
  state: "Passed" | "Planned";
};

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
  { id: "sound_effect", label: "Sound Effects" },
  { id: "ambience", label: "Ambience" }
];

type JobProgress = { kind: string; label: string; pending: number; total: number };

const JOB_KINDS: { command: "process_pending_jobs" | "process_audio_analysis_jobs"; kind: string; label: string }[] = [
  { command: "process_pending_jobs", kind: "metadata_extraction", label: "Reading metadata" },
  { command: "process_audio_analysis_jobs", kind: "audio_analysis", label: "Analyzing audio" }
];

const maintenanceLabels: Record<string, string> = {
  MissingMedia: "Missing media",
  LicenseReviewRequired: "License review",
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

// Derives a playback "mood" from the sound's applied tags (falling back to
// media_type) — there's no dedicated speech/music classifier yet, so this
// reuses the app's existing tagging system as the classification signal.
// Below this fraction of the clip detected as speech, treat it as noise in
// the Silero VAD signal rather than a real vocal presence (a few misfired
// frames on a transient shouldn't flip a whole SFX into "has voice").
const VOCAL_RATIO_THRESHOLD = 0.15;

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
  { id: "playback", label: "Playback", icon: Volume2 },
  { id: "storage", label: "Storage", icon: HardDrive },
  { id: "appearance", label: "Appearance", icon: Palette },
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

export function App() {
  const [releaseItems, setReleaseItems] = useState(fallbackReleaseItems);
  const updateChannelState = releaseItems.find((item) => item.blocker === "update_system")?.state ?? "Planned";

  const [librariesLoaded, setLibrariesLoaded] = useState(false);
  const [libraries, setLibraries] = useState<LibraryRecord[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState<string | null>(null);
  const [assets, setAssets] = useState<AssetRecord[]>([]);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [browserState, setBrowserState] = useState<BrowserState | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [rangeFilters, setRangeFilters] = useState<RangeFilters>({});
  const [smartCollectionModalOpen, setSmartCollectionModalOpen] = useState(false);
  const [smartCollectionName, setSmartCollectionName] = useState("");
  const [queryFilters, setQueryFilters] = useState<VisibleFilter[]>([]);
  const [libraryName, setLibraryName] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>("all");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsCategory, setSettingsCategory] = useState<SettingsCategory>("general");
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);
  const [sfxSubcategoriesOpen, setSfxSubcategoriesOpen] = useState(false);
  const [favoritesCategoriesOpen, setFavoritesCategoriesOpen] = useState(false);
  const [unreviewedCategoriesOpen, setUnreviewedCategoriesOpen] = useState(false);
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(
    () => new Set(["projects", "embedded", "detected", "source", "maintenance", "nas", "backup"])
  );
  const [refreshStatus, setRefreshStatus] = useState<string | null>(null);
  const [newProjectModalOpen, setNewProjectModalOpen] = useState(false);
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
  const [newTagName, setNewTagName] = useState("");
  const [newTagFacet, setNewTagFacet] = useState("action");

  const [collections, setCollections] = useState<CollectionRecord[]>([]);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectExportPath, setNewProjectExportPath] = useState("");
  const [lastExportProjectId, setLastExportProjectId] = useState<string | null>(null);
  const [drExportStatus, setDrExportStatus] = useState<string | null>(null);
  const [editorWorkflowOpen, setEditorWorkflowOpen] = useState(false);
  const [editorActionStatus, setEditorActionStatus] = useState<string | null>(null);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [commandPaletteQuery, setCommandPaletteQuery] = useState("");
  const [commandPaletteResults, setCommandPaletteResults] = useState<PaletteCommand[]>([]);
  const [commandPaletteActiveIndex, setCommandPaletteActiveIndex] = useState(0);
  const commandPaletteInputRef = useRef<HTMLInputElement | null>(null);

  const [undoStack, setUndoStack] = useState<{ id: string; label: string }[]>([]);
  const [redoStack, setRedoStack] = useState<{ id: string; label: string }[]>([]);

  const [sourceDraft, setSourceDraft] = useState<SourceRecordDraft | null>(null);
  const [maintenanceReport, setMaintenanceReport] = useState<MaintenanceReport | null>(null);
  const [mediaRootStatus, setMediaRootStatus] = useState<{ status: string; reconnectRequired: boolean } | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [formatFilter, setFormatFilter] = useState<AudioFormat | null>(null);
  const [similarStatus, setSimilarStatus] = useState<string | null>(null);
  const [jobProgress, setJobProgress] = useState<JobProgress[]>([]);
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
      activeFilter === "ambience"
    ) {
      base = assets.filter((asset) => asset.media_type === activeFilter);
    } else if (activeFilter === "has_vocals") {
      base = assets.filter((asset) => (asset.vocal_ratio ?? 0) >= VOCAL_RATIO_THRESHOLD);
    } else if (activeFilter === "instrumental") {
      base = assets.filter((asset) => asset.vocal_ratio != null && asset.vocal_ratio < VOCAL_RATIO_THRESHOLD);
    } else if (activeFilter === "has_tempo") {
      base = assets.filter((asset) => asset.bpm != null);
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
    return formatFilter ? base.filter((asset) => detectAudioFormat(asset) === formatFilter) : base;
  }, [assets, activeFilter, formatFilter]);

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

    (async () => {
      let next = await invoke<BrowserState>("create_browser_state", { visibleAssetIds: ids }).catch(() => null);
      if (!next) return;

      if (selectedAssetId) {
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
      if (typeof filter === "object" && "project" in filter) {
        const command = filter.smart ? "assets_in_smart_collection" : "assets_in_collection";
        invoke<AssetRecord[]>(command, { collectionId: filter.project })
          .then(setAssets)
          .catch(() => setAssets([]));
        return;
      }
      if (typeof filter === "object" && "tag" in filter) {
        invoke<AssetRecord[]>("assets_for_tag", { libraryId, tagId: filter.tag })
          .then(setAssets)
          .catch(() => setAssets([]));
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

      request.then(setAssets).catch(() => setAssets([]));
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
      const configs = only ? JOB_KINDS.filter((config) => only.includes(config.kind)) : JOB_KINDS;

      invoke<{ kind: string; pending: number }[]>("job_status", { libraryId })
        .then((statuses) => {
          const pendingByKind = new Map(statuses.map((entry) => [entry.kind, entry.pending]));

          configs.forEach((config) => {
            const startPending = pendingByKind.get(config.kind) ?? 0;
            // A prior drain for this same kind (e.g. from the last
            // background-tick) may still be mid-flight — importing a large
            // batch of files can easily take longer than the ~20s tick
            // interval. Starting a second overlapping loop on top of it was
            // stacking concurrent analysis work with nothing bounding it,
            // which is exactly what was driving CPU/memory usage far past
            // what a single drain needs.
            if (startPending === 0 || drainingJobKinds.current.has(config.kind)) return;
            drainingJobKinds.current.add(config.kind);

            setJobProgress((previous) => [
              ...previous.filter((entry) => entry.kind !== config.kind),
              { kind: config.kind, label: config.label, pending: startPending, total: startPending }
            ]);

            (async () => {
              let remaining = startPending;
              for (let iterations = 0; iterations < 200 && remaining > 0; iterations += 1) {
                const processed = await invoke<number>(config.command).catch(() => 0);
                if (processed === 0) break;
                remaining = Math.max(0, remaining - processed);
                setJobProgress((previous) =>
                  previous.map((entry) => (entry.kind === config.kind ? { ...entry, pending: remaining } : entry))
                );
              }
              setJobProgress((previous) => previous.filter((entry) => entry.kind !== config.kind));
              drainingJobKinds.current.delete(config.kind);
              refreshAssets(libraryId, searchQuery, activeFilter);
            })();
          });
        })
        .catch(() => {});
    },
    [refreshAssets, searchQuery, activeFilter]
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
      .then((record) =>
        setSourceDraft(
          record ?? {
            asset_id: assetId,
            provider: null,
            source_url: null,
            license_type: null,
            license_status: null,
            attribution: null,
            restrictions: null,
            receipt_path: null
          }
        )
      )
      .catch(() => setSourceDraft(null));
  }, []);

  useEffect(() => {
    invoke<ReleaseReadinessItem[]>("release_readiness_items")
      .then(setReleaseItems)
      .catch(() => setReleaseItems(fallbackReleaseItems));
  }, []);

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
  }, []);

  useEffect(() => {
    invoke<TagRecord[]>("list_tags").then(setTags).catch(() => setTags([]));
  }, [activeLibraryId]);

  useEffect(() => {
    if (!activeLibraryId) return;
    refreshCollections(activeLibraryId);
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
  }, [activeLibraryId, refreshCollections, refreshMaintenance, refreshTrashItems, runJobDrain]);

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
    if (!selectedAssetId) {
      setAppliedTags([]);
      setSuggestedTags([]);
      setSourceDraft(null);
      return;
    }
    refreshAssetTags(selectedAssetId);
  }, [selectedAssetId, refreshAssetTags]);

  const loadAssetForPlayback = useCallback(async (asset: AssetRecord, autoplay: boolean) => {
    const path = await invoke<string>("asset_playback_path", { assetId: asset.id });
    const audio = audioRef.current;
    if (!audio) return;

    audio.src = convertFileSrc(path);
    setPlayingAssetId(asset.id);
    setPeaks(null);
    if (autoplay) {
      audio.play().catch(() => setIsPlaying(false));
    }

    const requestId = ++peakRequestId.current;
    computePeaks(path).then((computed) => {
      if (peakRequestId.current === requestId) setPeaks(computed);
      if (computed) invoke("mark_waveform_ready", { assetId: asset.id }).catch(() => {});
    });
  }, []);

  const togglePlayback = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;

    if (playingAssetId && playingAssetId === selectedAssetId) {
      if (audio.paused) audio.play().catch(() => {});
      else audio.pause();
      return;
    }

    if (selectedAsset) {
      loadAssetForPlayback(selectedAsset, true);
    }
  }, [playingAssetId, selectedAssetId, selectedAsset, loadAssetForPlayback]);

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

  const handleCreateProject = useCallback(() => {
    if (!activeLibraryId || !newProjectName.trim()) return;
    invoke<CollectionRecord>("create_project", {
      libraryId: activeLibraryId,
      name: newProjectName.trim(),
      exportPath: newProjectExportPath.trim() || null
    })
      .then((project) => {
        setCollections((previous) => [...previous, project]);
        setNewProjectName("");
        setNewProjectExportPath("");
      })
      .catch(() => {});
  }, [activeLibraryId, newProjectName, newProjectExportPath]);

  const handleChooseProjectExportPath = useCallback(async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose a DaVinci Resolve sounds folder"
    });
    if (typeof selected === "string") setNewProjectExportPath(selected);
  }, []);

  const handleExportToProject = useCallback(
    (project: CollectionRecord, assetIds: string[]) => {
      if (assetIds.length === 0 || !project.export_path) return;
      setDrExportStatus(`Sending to ${project.name}…`);
      Promise.all(assetIds.map((assetId) => invoke<string>("export_asset_to_project", { assetId, projectId: project.id })))
        .then((destinations) => {
          setLastExportProjectId(project.id);
          setDrExportStatus(
            destinations.length === 1
              ? `Sent to ${project.name}`
              : `Sent ${destinations.length} sounds to ${project.name}`
          );
        })
        .catch((error) => setDrExportStatus(`Send to ${project.name} failed: ${String(error)}`));
    },
    []
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
      setSelectedAssetId(asset.id);
      const mode: SelectionMode = event.shiftKey ? "Range" : event.metaKey || event.ctrlKey ? "Toggle" : "Replace";
      if (mode === "Replace" && playingAssetId !== asset.id) {
        loadAssetForPlayback(asset, true);
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

  const handleAddSelectedToProject = useCallback(
    (project: CollectionRecord) => {
      if (bulkAssetIds.length === 0) return;
      invoke<string>("add_to_collection", { collectionId: project.id, assetIds: bulkAssetIds })
        .then((undoId) => {
          setUndoStack((previous) => [...previous, { id: undoId, label: `Add to "${project.name}"` }]);
          setRedoStack([]);
        })
        .catch(() => {});
    },
    [bulkAssetIds]
  );

  const handleSaveSource = useCallback(() => {
    if (!sourceDraft) return;
    invoke("set_source_record", { draft: sourceDraft })
      .then(() => activeLibraryId && refreshMaintenance(activeLibraryId))
      .catch(() => {});
  }, [sourceDraft, activeLibraryId, refreshMaintenance]);

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

  const handlePurgeTrashItem = useCallback(
    (item: TrashItem) => {
      if (!activeLibraryId) return;
      invoke<boolean>("purge_from_trash", { assetId: item.asset_id })
        .then((purged) => {
          if (purged && activeLibraryId) refreshTrashItems(activeLibraryId);
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
    const destination = await openDialog({ directory: true, multiple: false, title: "Choose backup destination" });
    if (typeof destination !== "string") return;

    setBackupStatus("Backing up…");
    try {
      const backupPackage = await invoke<BackupPackage>("backup_library", {
        libraryId: activeLibraryId,
        backupDir: destination
      });
      setBackupStatus(`Backed up to ${backupPackage.catalog_snapshot_path}`);
    } catch (error) {
      setBackupStatus(`Backup failed: ${String(error)}`);
    }
  }, [activeLibraryId]);

  const handleRestoreLibrary = useCallback(async () => {
    const source = await openDialog({ directory: true, multiple: false, title: "Choose a backup folder to restore" });
    if (typeof source !== "string") return;

    const confirmed = await confirmDialog(
      "This replaces the current catalog with the selected backup. A safety copy of the current catalog is kept, but any changes made since the backup was taken will no longer be visible.",
      { title: "Restore library from backup", kind: "warning" }
    );
    if (!confirmed) return;

    setBackupStatus("Restoring…");
    try {
      const libraryCount = await invoke<number>("restore_library", { backupDir: source });
      setBackupStatus(`Restored ${libraryCount} librar${libraryCount === 1 ? "y" : "ies"} from backup`);
      setSelectedAssetId(null);
      const loaded = await invoke<LibraryRecord[]>("list_libraries");
      setLibraries(loaded);
      setActiveLibraryId(loaded.length > 0 ? loaded[0].id : null);
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

  const handleRefreshLibrary = useCallback(() => {
    if (!activeLibraryId) return;
    setRefreshStatus("Scanning for new files…");
    invoke<ImportFolderResult>("refresh_library", { libraryId: activeLibraryId })
      .then((result) => {
        if (result.imported.length > 0) {
          runJobDrain(activeLibraryId);
        }
        setRefreshStatus(
          result.imported.length > 0
            ? `Found ${result.imported.length} new sound${result.imported.length === 1 ? "" : "s"}`
            : "No new files found"
        );
        refreshAssets(activeLibraryId, searchQuery, activeFilter);
        refreshMaintenance(activeLibraryId);
      })
      .catch((error) => setRefreshStatus(`Refresh failed: ${String(error)}`));
  }, [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance, runJobDrain]);

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

  // The Rust-side standing worker (apps/desktop/src-tauri) ticks roughly
  // every 20s, requeuing retryable failed jobs and emitting this event —
  // this is what makes job processing actually run continuously in the
  // background rather than only right after Import/Refresh.
  useEffect(() => {
    if (!activeLibraryId) return;
    const unlistenBackgroundTick = listen("background-tick", () => runJobDrain(activeLibraryId));
    return () => {
      unlistenBackgroundTick.then((dispose) => dispose());
    };
  }, [activeLibraryId, runJobDrain]);

  // process_audio_analysis_jobs claims and fully decodes/analyzes up to 20
  // files per invoke — real per-file work, easily minutes for a batch — so
  // without this, the progress bar only updates once the whole batch's
  // invoke resolves and sits frozen at 0% the entire time despite real
  // work happening. This event (emitted per job, not per batch) is what
  // lets it move continuously instead.
  useEffect(() => {
    const unlistenAnalysisProgress = listen("audio-analysis-progress", () => {
      setJobProgress((previous) =>
        previous.map((entry) =>
          entry.kind === "audio_analysis" ? { ...entry, pending: Math.max(0, entry.pending - 1) } : entry
        )
      );
    });
    return () => {
      unlistenAnalysisProgress.then((dispose) => dispose());
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

  const handleSetTheme = useCallback((theme: AppPreferences["theme"]) => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, theme };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

  const handleChooseWatchedFolder = useCallback(async () => {
    if (!activeLibraryId) return;
    const folder = await openDialog({ directory: true, multiple: false, title: "Choose a folder to watch" });
    if (typeof folder !== "string") return;

    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, watched_folder_path: folder, watched_folder_library_id: activeLibraryId };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, [activeLibraryId]);

  const handleClearWatchedFolder = useCallback(() => {
    setPreferences((previous) => {
      if (!previous) return previous;
      const next = { ...previous, watched_folder_path: null, watched_folder_library_id: null };
      invoke("save_app_preferences", { preferences: next }).catch(() => {});
      return next;
    });
  }, []);

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
      const key = event.key === " " ? "Space" : event.key.length === 1 ? event.key.toUpperCase() : event.key;
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
      if (!binding) return;

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
    handleImportFolder,
    bulkAssetIds,
    handleCopyAssetPaths,
    handleExportSelected,
    browserState
  ]);

  if (librariesLoaded && libraries.length === 0) {
    return (
      <main className="shell setup-shell">
        <section className="setup-card" aria-label="Create library">
          <div className="brand">Darkwave</div>
          <h1>Create your library</h1>
          <p>Choose a name. The first folder you import becomes this library's media location automatically.</p>
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
          <button
            className="primary-action"
            type="button"
            onClick={handleCreateLibrary}
            disabled={!libraryName.trim()}
          >
            Create Library
          </button>
        </section>
      </main>
    );
  }

  const waveformActiveIndex = peaks && duration > 0 ? Math.floor((currentTime / duration) * peaks.length) : -1;
  const drTargetAssetId = playingAssetId ?? selectedAssetId;
  const drTargetProject = collections.find((project) => project.id === lastExportProjectId) ?? null;
  const playerMood = classifyPlayerMood(selectedAsset, appliedTags, selectedAsset?.vocal_ratio ?? null);
  const playerMoodStyle = playerMood
    ? ({
        "--player-accent-from": playerMoodTheme[playerMood].from,
        "--player-accent-to": playerMoodTheme[playerMood].to,
        "--player-glow-color": playerMoodTheme[playerMood].glow
      } as CSSProperties)
    : undefined;

  return (
    <main
      className={[
        "shell",
        sidebarCollapsed ? "sidebar-collapsed" : "",
        inspectorCollapsed ? "inspector-collapsed" : "",
      ]
        .filter(Boolean)
        .join(" ")}
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

          <div className="nav-heading-row">
            <span className="nav-heading sonic-radar-heading sonic-radar-root" onClick={() => toggleSection("sidebar-sonic-radar")}>
              <Activity size={13} />
              Sonic Radar
            </span>
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
              >
                <Gauge size={13} />
                Detected Tempo
              </button>
              <button
                className={activeFilter === "has_pitch" ? "nav-item sonic-radar-item active" : "nav-item sonic-radar-item"}
                onClick={() => setActiveFilter("has_pitch")}
              >
                <Music size={13} />
                Detected Pitch
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
                <div className="nav-item-row" key={project.id}>
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
                      className={project.export_path ? "dr-button" : "dr-button disabled"}
                      aria-label={
                        project.export_path
                          ? `Send selected to ${project.name}'s DaVinci Resolve folder`
                          : `${project.name} has no DaVinci Resolve folder configured yet`
                      }
                      title={
                        project.export_path
                          ? `Send selected to ${project.name} (${project.export_path})`
                          : "Set a DaVinci Resolve sounds folder for this project to enable quick export"
                      }
                      disabled={!project.export_path || bulkAssetIds.length === 0}
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
              placeholder="Search sounds, tags, source, license"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
            />
          </label>
          <div style={{ position: "relative" }}>
            <button
              className="icon-button"
              aria-label="Filter"
              onClick={() => setFilterMenuOpen((previous) => !previous)}
            >
              <ListFilter size={17} />
            </button>
            <AnimatePresence>
              {filterMenuOpen ? (
                <motion.div
                  className="modal-card filter-menu"
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
            className={jobProgress.length > 0 ? "icon-button activity-button busy" : "icon-button activity-button"}
            aria-label="Background activity"
            title={jobProgress.length > 0 ? "Background work is running — click for details" : "Background activity: all caught up"}
            onClick={() => setBackgroundActivityOpen(true)}
          >
            <Activity size={16} />
            <span className="activity-led" aria-hidden="true" />
          </button>
          <button className="icon-button" aria-label="Open settings" onClick={() => setSettingsOpen(true)}>
            <Settings size={17} />
          </button>
        </header>
        {queryFilters.length > 0 || refreshStatus || importStatus ? (
          <div className="chip-row status-strip" aria-label="Status">
            {queryFilters.map((filter, index) => (
              <span key={index} className="suggestion-chip">
                {filter.field} {filter.operator} {filter.value}
              </span>
            ))}
            {!queryFilters.length && (refreshStatus || importStatus) ? (
              <span className="suggestion-chip">{refreshStatus ?? importStatus}</span>
            ) : null}
          </div>
        ) : null}
        <section className="filter-panel" aria-label="Range filters">
          <div className="selection-bar" aria-label="Selection actions">
            <strong>{selectedAsset ? "1 selected" : "0 selected"}</strong>
            <span>Click a row to select</span>
            <span>Click a tag or project to apply it</span>
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
        <section
          className="browser"
          aria-label="Sound browser"
          data-density={preferences?.browser_density ?? "Comfortable"}
          ref={browserScrollRef}
          onScroll={(event) => setBrowserScrollTop(event.currentTarget.scrollTop)}
        >
          <div className="browser-header">
            <span aria-hidden="true" />
            <span aria-hidden="true" />
            <span>Name</span>
            <span>Type</span>
            <span>Storage</span>
            <span>Size</span>
            <span>Status</span>
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
                <strong>{asset.display_name}</strong>
                <span>{asset.media_type}</span>
                <span>{asset.storage_mode}</span>
                <span>{formatFileSize(asset.file_size)}</span>
                <span>{asset.availability_state}</span>
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
                      <stop offset="0%" stopColor="var(--player-accent-from, #ff7940)" />
                      <stop offset="100%" stopColor="var(--player-accent-to, #f14800)" />
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
                <h2>Send to project</h2>
                {collections.filter((project) => project.collection_type === "Project").length === 0 ? (
                  <p className="editor-workflow-empty">
                    Create a project with an export folder to send sounds straight into it.
                  </p>
                ) : (
                  <div className="editor-project-list">
                    {collections
                      .filter((project) => project.collection_type === "Project")
                      .map((project) => (
                        <div className="editor-project-card" key={project.id}>
                          <span className="editor-project-thumb">
                            <Music size={14} />
                          </span>
                          <div className="editor-project-meta">
                            <strong>{project.name}</strong>
                            <small>{project.export_path ?? "No export folder set"}</small>
                          </div>
                          <button
                            type="button"
                            className={project.export_path ? "dr-button" : "dr-button disabled"}
                            disabled={!project.export_path || bulkAssetIds.length === 0}
                            aria-label={`Send selected to ${project.name}`}
                            onClick={() => handleExportToProject(project, bulkAssetIds)}
                          >
                            <Clapperboard size={15} />
                            Send
                          </button>
                        </div>
                      ))}
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
            <label className="setting-row">
              <input
                type="checkbox"
                checked={selectedAsset.review_state === "Reviewed"}
                onChange={() => handleToggleReviewed(selectedAsset)}
              />
              <span>Mark reviewed</span>
            </label>
            <button type="button" className="text-button" onClick={handleMoveToTrash}>
              Move to Trash
            </button>
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
          <h2>Apply Tag</h2>
          <div className="tag-grid">
            {tags.map((tag) => (
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
          <div className="drop-target-grid">
            {collections
              .filter((project) => project.collection_type !== "Smart")
              .map((project) => (
                <button
                  key={project.id}
                  className="project-chip"
                  title={`Add selected to ${project.name}`}
                  aria-label={`Add selected to ${project.name}`}
                  onClick={() => handleAddSelectedToProject(project)}
                  disabled={bulkAssetIds.length === 0}
                >
                  <span className="project-thumb">
                    <Music size={12} />
                  </span>
                  <span className="project-chip-label">{project.name}</span>
                </button>
              ))}
          </div>
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
          selectedAsset.musical_key != null ||
          selectedAsset.duration_ms != null) ? (
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
              {selectedAsset.musical_key != null ? (
                <div className="attribute-pill" title="Best-effort estimate, not a musical key">
                  <span className="attribute-label">Pitch</span>
                  <strong className="attribute-value">{selectedAsset.musical_key}</strong>
                </div>
              ) : null}
            </div>
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
              <button type="button" className="primary-action" onClick={handleSaveSource}>
                <Save size={15} />
                Save source
              </button>
            </>
          ) : (
            <span className="empty-hint">Select a sound to edit source and license</span>
          )}
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
          <div className="status-line">{backupStatus ?? "Copies the catalog snapshot and manifest to a folder you choose"}</div>
        </CollapsibleSection>
        </div>
      </aside>
      <footer
        className={`transport${sidebarCollapsed ? " sidebar-collapsed" : ""}${inspectorCollapsed ? " inspector-collapsed" : ""}${isPlaying ? " is-playing" : ""}`}
        aria-label="Transport"
        style={playerMoodStyle}
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
              ? `Send to ${drTargetProject.name}'s DaVinci Resolve folder`
              : "Send selected to a project's DaVinci Resolve folder — click a project's DR button first to set the target"
          }
          title={
            drTargetProject
              ? `Send to ${drTargetProject.name} (${drTargetProject.export_path})`
              : "No DaVinci Resolve target yet — click a project's DR button in the sidebar first"
          }
          disabled={!drTargetProject || !drTargetAssetId}
          onClick={() => {
            if (drTargetProject && drTargetAssetId) handleExportToProject(drTargetProject, [drTargetAssetId]);
          }}
        >
          <Clapperboard size={16} />
        </button>
        {drExportStatus ? <span className="dr-status">{drExportStatus}</span> : null}
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
                          <Gauge size={14} />
                          <span>Status</span>
                          <strong>{formatMediaRootStatus(mediaRootStatus?.status)}</strong>
                        </div>
                      </div>
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
                      <h2>Watched Folder</h2>
                      <div className="settings-grid">
                        <div className="settings-row">
                          <Import size={14} />
                          <span>Folder</span>
                          <strong className="settings-value-path" title={preferences?.watched_folder_path ?? undefined}>
                            {preferences?.watched_folder_path ?? "Not watching"}
                          </strong>
                        </div>
                        <div className="status-line">
                          New, stable files dropped here import automatically (as referenced files)
                          into {preferences?.watched_folder_library_id === activeLibraryId
                            ? activeLibrary?.name ?? "the active library"
                            : "the library it was set up with"}
                          , checked roughly every 20 seconds by the background worker.
                        </div>
                        <button type="button" className="text-button" onClick={handleChooseWatchedFolder} disabled={!activeLibraryId}>
                          {preferences?.watched_folder_path ? "Change Folder…" : "Choose Folder…"}
                        </button>
                        {preferences?.watched_folder_path ? (
                          <button type="button" className="text-button" onClick={handleClearWatchedFolder}>
                            Stop Watching
                          </button>
                        ) : null}
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
                              <span>{item.original_path.split("/").pop()}</span>
                              <button type="button" className="text-button" onClick={() => handleRestoreFromTrash(item)}>
                                Restore
                              </button>
                              <button type="button" className="text-button" onClick={() => handlePurgeTrashItem(item)}>
                                Purge
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
            {jobProgress.length === 0 && !refreshStatus ? (
              <p className="empty-hint">
                All caught up — nothing analyzing, importing, or scanning right now.
              </p>
            ) : (
              <div className="job-progress-panel" aria-label="Background work">
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
                          </span>
                        </div>
                        <div className="job-progress-track">
                          <div className="job-progress-fill" style={{ width: `${percent}%` }} />
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
            <p className="settings-hint">
              Analysis (metadata, waveform, tempo, key, vocal detection) always runs in the background, one file at a
              time, so browsing and playback stay responsive while it works.
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
                <span>DaVinci Resolve sounds folder (optional)</span>
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
              </label>
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
                <div className="shortcut-row" key={item.command}>
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
    </main>
  );
}
