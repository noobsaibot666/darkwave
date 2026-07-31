import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AnimatePresence, motion } from "motion/react";
import { open as openDialog, save as saveDialog, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import {
  Bell,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clapperboard,
  Contrast,
  Gauge,
  Import,
  Link2,
  ListFilter,
  Music,
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
  Volume2,
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
};

type ImportFailure = {
  filename: string;
  reason: string;
};

type ImportFolderResult = {
  imported: AssetRecord[];
  failed: ImportFailure[];
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
  watched_folder_path: string | null;
  watched_folder_library_id: string | null;
};

type ActiveFilter =
  | "all"
  | "favorites"
  | "unreviewed"
  | "missing"
  | "needs_review"
  | "music"
  | "sound_effect"
  | "ambience"
  | { project: string; smart?: boolean }
  | { tag: string };

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

const playerMoodTheme: Record<PlayerMood, { from: string; to: string; glow: string }> = {
  soundtrack: { from: "#4ade9c", to: "#0ea968", glow: "rgba(14, 169, 104, 0.45)" },
  "soundtrack-voice": { from: "#c4a6fa", to: "#8b5cf6", glow: "rgba(139, 92, 246, 0.45)" },
  "voice-over": { from: "#7c90f5", to: "#4c5fe0", glow: "rgba(76, 95, 224, 0.45)" },
  sfx: { from: "#ff8a73", to: "#f2543a", glow: "rgba(242, 84, 58, 0.45)" }
};

// Derives a playback "mood" from the sound's applied tags (falling back to
// media_type) — there's no dedicated speech/music classifier yet, so this
// reuses the app's existing tagging system as the classification signal.
// Below this fraction of the clip detected as speech, treat it as noise in
// the Silero VAD signal rather than a real vocal presence (a few misfired
// frames on a transient shouldn't flip a whole SFX into "has voice").
const VOCAL_RATIO_THRESHOLD = 0.15;

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

function CollapsibleSection({
  id,
  title,
  collapsed,
  onToggle,
  children,
  ...rest
}: {
  id: string;
  title: string;
  collapsed: boolean;
  onToggle: (id: string) => void;
  children: ReactNode;
} & Record<string, unknown>) {
  return (
    <section {...rest}>
      <div className="section-header" onClick={() => onToggle(id)}>
        <h2>{title}</h2>
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
  const [libraryRoot, setLibraryRoot] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>("all");
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const focusSearch = useCallback(() => {
    const input = searchInputRef.current;
    if (!input) return;
    input.scrollIntoView({ behavior: "smooth", inline: "nearest", block: "nearest" });
    input.focus();
    input.select();
  }, []);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);
  const [sfxSubcategoriesOpen, setSfxSubcategoriesOpen] = useState(false);
  const [collapsedSections, setCollapsedSections] = useState<Set<string>>(
    () => new Set(["projects", "embedded", "detected", "source", "release", "maintenance", "nas", "backup"])
  );
  const [refreshStatus, setRefreshStatus] = useState<string | null>(null);
  const [newProjectModalOpen, setNewProjectModalOpen] = useState(false);
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
  const [vocalRatio, setVocalRatio] = useState<number | null>(null);
  const [newTagName, setNewTagName] = useState("");
  const [newTagFacet, setNewTagFacet] = useState("action");

  const [collections, setCollections] = useState<CollectionRecord[]>([]);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectExportPath, setNewProjectExportPath] = useState("");
  const [lastExportProjectId, setLastExportProjectId] = useState<string | null>(null);
  const [drExportStatus, setDrExportStatus] = useState<string | null>(null);

  const [undoStack, setUndoStack] = useState<{ id: string; label: string }[]>([]);
  const [redoStack, setRedoStack] = useState<{ id: string; label: string }[]>([]);

  const [sourceDraft, setSourceDraft] = useState<SourceRecordDraft | null>(null);
  const [maintenanceReport, setMaintenanceReport] = useState<MaintenanceReport | null>(null);
  const [mediaRootStatus, setMediaRootStatus] = useState<{ status: string; reconnectRequired: boolean } | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [exportFormat, setExportFormat] = useState<"original" | "wav24">("original");
  const [similarStatus, setSimilarStatus] = useState<string | null>(null);
  const [jobProgress, setJobProgress] = useState<JobProgress[]>([]);
  const [offlineControl, setOfflineControl] = useState<OfflineControlState | null>(null);
  const [reconnectStatus, setReconnectStatus] = useState<string | null>(null);
  const [trashItems, setTrashItems] = useState<TrashItem[]>([]);
  const [backupStatus, setBackupStatus] = useState<string | null>(null);
  const [cacheStatus, setCacheStatus] = useState<string | null>(null);

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
  const vocalRatioRequestId = useRef(0);

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
    if (activeFilter === "favorites") return assets.filter((asset) => asset.favorite);
    if (activeFilter === "unreviewed") return assets.filter((asset) => asset.review_state === "Unreviewed");
    if (activeFilter === "missing") return assets.filter((asset) => asset.availability_state === "Missing");
    if (
      activeFilter === "needs_review" ||
      activeFilter === "music" ||
      activeFilter === "sound_effect" ||
      activeFilter === "ambience"
    ) {
      return assets.filter((asset) => asset.media_type === activeFilter);
    }
    return assets;
  }, [assets, activeFilter]);

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
            if (startPending === 0) return;

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
    const row = document.querySelector(`[data-asset-id="${selectedAssetId}"]`);
    row?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [selectedAssetId]);

  useEffect(() => {
    if (!selectedAssetId) {
      setAppliedTags([]);
      setSuggestedTags([]);
      setSourceDraft(null);
      setVocalRatio(null);
      return;
    }
    refreshAssetTags(selectedAssetId);
    const requestId = ++vocalRatioRequestId.current;
    invoke<number | null>("asset_vocal_ratio", { assetId: selectedAssetId })
      .then((ratio) => {
        if (vocalRatioRequestId.current === requestId) setVocalRatio(ratio);
      })
      .catch(() => {
        if (vocalRatioRequestId.current === requestId) setVocalRatio(null);
      });
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

  const handleChooseMediaRoot = async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: "Choose media location" });
    if (typeof selected === "string") setLibraryRoot(selected);
  };

  const handleCreateLibrary = async () => {
    if (!libraryName.trim() || !libraryRoot.trim()) return;

    const library = await invoke<LibraryRecord>("create_library", {
      name: libraryName.trim(),
      mediaRoot: libraryRoot.trim()
    });
    setLibraries((previous) => [...previous, library]);
    setActiveLibraryId(library.id);
    setLibraryName("");
    setLibraryRoot("");
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
      refreshAssets(activeLibraryId, searchQuery, activeFilter);
      refreshMaintenance(activeLibraryId);
      runJobDrain(activeLibraryId);
    } catch (error) {
      setImportStatus(`Import failed: ${String(error)}`);
    }
  }, [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance, runJobDrain]);

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

  const handleExportSelected = useCallback(async () => {
    if (!selectedAssetId) return;
    const destination = await openDialog({ directory: true, multiple: false, title: "Choose export destination" });
    if (typeof destination !== "string") return;

    try {
      const destinationPath = await invoke<string>("export_selected_asset", {
        assetId: selectedAssetId,
        destinationFolder: destination,
        format: exportFormat === "wav24" ? "wav24" : null
      });
      setExportStatus(`Exported to ${destinationPath}`);
    } catch (error) {
      setExportStatus(`Export failed: ${String(error)}`);
    }
  }, [selectedAssetId, exportFormat]);

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
            format: exportFormat === "wav24" ? "wav24" : null
          })
        )
      );
      setExportStatus(`Exported ${bulkAssetIds.length} sound${bulkAssetIds.length === 1 ? "" : "s"}`);
    } catch (error) {
      setExportStatus(`Export failed: ${String(error)}`);
    }
  }, [bulkAssetIds, exportFormat]);

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
    function isTypingTarget(target: EventTarget | null) {
      return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
    }

    function acceleratorFor(event: KeyboardEvent): string {
      const mod = event.metaKey || event.ctrlKey;
      if (event.key === " ") return mod ? "Mod+Space" : "Space";
      const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
      return mod ? `Mod+${key}` : key;
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
        case "FocusSearch":
          event.preventDefault();
          focusSearch();
          break;
        case "Import":
          event.preventDefault();
          handleImportFolder();
          break;
        case "ExportSelected":
          event.preventDefault();
          handleExportSelected();
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
    focusSearch,
    handleImportFolder,
    handleExportSelected,
    browserState
  ]);

  if (librariesLoaded && libraries.length === 0) {
    return (
      <main className="shell setup-shell">
        <section className="setup-card" aria-label="Create library">
          <div className="brand">Darkwave</div>
          <h1>Create your library</h1>
          <p>Choose a name and a media location. This can be a local disk, external drive, or NAS folder.</p>
          <label className="setup-field">
            <span>Library name</span>
            <input
              value={libraryName}
              onChange={(event) => setLibraryName(event.target.value)}
              placeholder="Home Studio"
            />
          </label>
          <label className="setup-field">
            <span>Media location</span>
            <div className="setup-field-row">
              <input
                value={libraryRoot}
                onChange={(event) => setLibraryRoot(event.target.value)}
                placeholder="/Volumes/TrueNAS/SFX"
              />
              <button type="button" onClick={handleChooseMediaRoot}>
                Browse
              </button>
            </div>
          </label>
          <button
            className="primary-action"
            type="button"
            onClick={handleCreateLibrary}
            disabled={!libraryName.trim() || !libraryRoot.trim()}
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
  const playerMood = classifyPlayerMood(selectedAsset, appliedTags, vocalRatio);
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
          ) : null}
          {smartFilters.map((filter) => (
            <div key={filter.label}>
              <button
                className={activeFilter === filter.id ? "nav-item active" : "nav-item"}
                onClick={() => setActiveFilter(filter.id)}
              >
                {filter.label}
              </button>
              {filter.id === "sound_effect" ? (
                <>
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
                </>
              ) : null}
            </div>
          ))}
          <div className="nav-heading-row">
            <span className="nav-heading">Projects</span>
            <button
              type="button"
              className="nav-heading-add"
              aria-label="New project"
              title="New project"
              onClick={() => setNewProjectModalOpen(true)}
            >
              <Plus size={16} />
            </button>
          </div>
          {collections.map((project) => (
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
          <CollapsibleSection id="release" title="Release Readiness" collapsed={collapsedSections.has("release")} onToggle={toggleSection}>
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
          </CollapsibleSection>
          <div className="virtualization-bar" aria-label="Browser performance">
            <span>{visibleAssets.length} row{visibleAssets.length === 1 ? "" : "s"}</span>
            <span>Not yet virtualized</span>
          </div>
        </div>
      </aside>
      <section className="workspace">
        <header className="topbar">
          <button className="text-button" onClick={focusSearch}>
            Focus Search
          </button>
          <button className="text-button" onClick={() => document.getElementById("tags-section")?.scrollIntoView({ behavior: "smooth" })}>
            Apply Tag
          </button>
          <select
            aria-label="Export format"
            value={exportFormat}
            onChange={(event) => setExportFormat(event.target.value === "wav24" ? "wav24" : "original")}
          >
            <option value="original">Original</option>
            <option value="wav24">WAV (24-bit)</option>
          </select>
          <button className="primary-action" onClick={handleExportSelected} disabled={!selectedAssetId}>
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
          <button type="button" className="icon-button" aria-label="Refresh library" onClick={() => handleRefreshLibrary()} title="Scan the media root for new files">
            <RefreshCw size={16} />
          </button>
          <button className="icon-button" aria-label="Open settings" onClick={() => setSettingsOpen(true)}>
            <Settings size={17} />
          </button>
        </header>
        {queryFilters.length > 0 || refreshStatus || importStatus ? (
          <div className="tag-grid status-strip" aria-label="Status">
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
        {jobProgress.length > 0 ? (
          <div className="job-progress-panel" aria-label="Background work">
            {jobProgress.map((job) => {
              const percent = job.total > 0 ? Math.round(((job.total - job.pending) / job.total) * 100) : 0;
              return (
                <div className="job-progress-row" key={job.kind}>
                  <span className="job-progress-label">{job.label}</span>
                  <div className="job-progress-track">
                    <div className="job-progress-fill" style={{ width: `${percent}%` }} />
                  </div>
                  <span className="job-progress-count">
                    {job.total - job.pending}/{job.total}
                  </span>
                </div>
              );
            })}
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
          {hasActiveRangeFilters(rangeFilters) ? (
            <>
              <button type="button" className="text-button" onClick={() => setRangeFilters({})}>
                Clear
              </button>
              <button type="button" className="text-button" onClick={() => setSmartCollectionModalOpen(true)}>
                <Zap size={13} />
                Save as Smart Collection
              </button>
            </>
          ) : null}
        </section>
        <section className="browser" aria-label="Sound browser" data-density={preferences?.browser_density ?? "Comfortable"}>
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
            visibleAssets.map((asset, index) => {
              const isSelected = browserState
                ? browserState.selected_indices.includes(index)
                : asset.id === selectedAssetId;
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
                <div className="waveform" aria-hidden="true">
                  <Music size={16} />
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
            })
          )}
        </section>
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
          <div className="tag-grid">
            {suggestedTags.length === 0 ? (
              <span className="empty-hint">No pending suggestions</span>
            ) : (
              suggestedTags.map((tag) => (
                <span className="suggestion-chip" key={tag.id}>
                  {tag.name}
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
          <div className="tag-grid">
            {appliedTags.length === 0 ? (
              <span className="empty-hint">No tags applied</span>
            ) : (
              appliedTags.map((tag) => (
                <span key={tag.id} className="suggestion-chip">
                  {tag.name}
                  <button onClick={() => handleRemoveTag(tag)} aria-label={`Remove ${tag.name}`}>
                    ✗
                  </button>
                </span>
              ))
            )}
          </div>
          <h2>Add Tag</h2>
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
          {offlineControl ? (
            <>
              <div className="status-line">
                {offlineControl.media_root} — {mediaRootStatus?.status ?? "unknown"}
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
            const seekStepSeconds = 5;
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
                  <strong className="settings-value-path" title={activeLibrary?.media_root}>
                    {activeLibrary?.media_root ?? "—"}
                  </strong>
                </div>
                <div className="settings-row">
                  <Gauge size={14} />
                  <span>Status</span>
                  <strong>{mediaRootStatus?.status ?? "unknown"}</strong>
                </div>
              </div>
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
