import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog, confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import {
  Bell,
  Command,
  Contrast,
  Gauge,
  Import,
  ListFilter,
  Music,
  Pause,
  Play,
  Save,
  Search,
  Settings,
  ShieldCheck,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Star,
  Volume2,
  Zap
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent } from "react";

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
};

type ActiveFilter =
  | "all"
  | "favorites"
  | "unreviewed"
  | "missing"
  | "music"
  | "sound_effect"
  | "ambience"
  | { project: string };

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

async function computePeaks(path: string, bucketCount = 96): Promise<number[] | null> {
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
  { id: "music", label: "Music" },
  { id: "sound_effect", label: "Sound Effects" },
  { id: "ambience", label: "Ambience" }
];

const maintenanceLabels: Record<string, string> = {
  MissingMedia: "Missing media",
  LicenseReviewRequired: "License review",
  StaleWaveformCache: "Waveform cache",
  DuplicateContent: "Duplicates"
};

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
  const [queryFilters, setQueryFilters] = useState<VisibleFilter[]>([]);
  const [libraryName, setLibraryName] = useState("");
  const [libraryRoot, setLibraryRoot] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);
  const [activeFilter, setActiveFilter] = useState<ActiveFilter>("all");
  const searchInputRef = useRef<HTMLInputElement | null>(null);

  const [tags, setTags] = useState<TagRecord[]>([]);
  const [appliedTags, setAppliedTags] = useState<TagRecord[]>([]);
  const [suggestedTags, setSuggestedTags] = useState<TagRecord[]>([]);
  const [newTagName, setNewTagName] = useState("");
  const [newTagFacet, setNewTagFacet] = useState("action");

  const [collections, setCollections] = useState<CollectionRecord[]>([]);
  const [newProjectName, setNewProjectName] = useState("");

  const [undoStack, setUndoStack] = useState<{ id: string; label: string }[]>([]);
  const [redoStack, setRedoStack] = useState<{ id: string; label: string }[]>([]);

  const [sourceDraft, setSourceDraft] = useState<SourceRecordDraft | null>(null);
  const [maintenanceReport, setMaintenanceReport] = useState<MaintenanceReport | null>(null);
  const [mediaRootStatus, setMediaRootStatus] = useState<{ status: string; reconnectRequired: boolean } | null>(null);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const [offlineControl, setOfflineControl] = useState<OfflineControlState | null>(null);
  const [reconnectStatus, setReconnectStatus] = useState<string | null>(null);
  const [trashItems, setTrashItems] = useState<TrashItem[]>([]);
  const [backupStatus, setBackupStatus] = useState<string | null>(null);

  const [preferences, setPreferences] = useState<AppPreferences | null>(null);
  const [trashRetentionDays, setTrashRetentionDays] = useState(30);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playingAssetId, setPlayingAssetId] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
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
    if (activeFilter === "favorites") return assets.filter((asset) => asset.favorite);
    if (activeFilter === "unreviewed") return assets.filter((asset) => asset.review_state === "Unreviewed");
    if (activeFilter === "missing") return assets.filter((asset) => asset.availability_state === "Missing");
    if (activeFilter === "music" || activeFilter === "sound_effect" || activeFilter === "ambience") {
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

  const refreshAssets = useCallback((libraryId: string, query: string, filter: ActiveFilter) => {
    if (typeof filter === "object") {
      invoke<AssetRecord[]>("assets_in_collection", { collectionId: filter.project })
        .then(setAssets)
        .catch(() => setAssets([]));
      return;
    }
    const request = query.trim().length > 0
      ? invoke<AssetRecord[]>("search_assets", { libraryId, query })
      : invoke<AssetRecord[]>("list_assets", { libraryId });

    request.then(setAssets).catch(() => setAssets([]));
  }, []);

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
  }, [activeLibraryId, refreshCollections, refreshMaintenance, refreshTrashItems]);

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
    },
    [visibleAssets, playingAssetId, selectedAssetId, loadAssetForPlayback]
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

  const handleCreateProject = useCallback(() => {
    if (!activeLibraryId || !newProjectName.trim()) return;
    invoke<CollectionRecord>("create_project", { libraryId: activeLibraryId, name: newProjectName.trim() })
      .then((project) => {
        setCollections((previous) => [...previous, project]);
        setNewProjectName("");
      })
      .catch(() => {});
  }, [activeLibraryId, newProjectName]);

  const handleRowClick = useCallback(
    (asset: AssetRecord, index: number, event: MouseEvent) => {
      setSelectedAssetId(asset.id);
      if (!browserState) return;

      const mode: SelectionMode = event.shiftKey ? "Range" : event.metaKey || event.ctrlKey ? "Toggle" : "Replace";
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
    [browserState]
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
      invoke<number>("process_pending_jobs")
        .then(() => refreshAssets(activeLibraryId, searchQuery, activeFilter))
        .catch(() => {});
    } catch (error) {
      setImportStatus(`Import failed: ${String(error)}`);
    }
  }, [activeLibraryId, searchQuery, activeFilter, refreshAssets, refreshMaintenance]);

  const handleExportSelected = useCallback(async () => {
    if (!selectedAssetId) return;
    const destination = await openDialog({ directory: true, multiple: false, title: "Choose export destination" });
    if (typeof destination !== "string") return;

    try {
      const destinationPath = await invoke<string>("export_selected_asset", {
        assetId: selectedAssetId,
        destinationFolder: destination
      });
      setExportStatus(`Exported to ${destinationPath}`);
    } catch (error) {
      setExportStatus(`Export failed: ${String(error)}`);
    }
  }, [selectedAssetId]);

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
          invoke<string>("export_selected_asset", { assetId, destinationFolder: destination })
        )
      );
      setExportStatus(`Exported ${bulkAssetIds.length} sound${bulkAssetIds.length === 1 ? "" : "s"}`);
    } catch (error) {
      setExportStatus(`Export failed: ${String(error)}`);
    }
  }, [bulkAssetIds]);

  const handleExportLicenseReport = useCallback(async () => {
    if (typeof activeFilter !== "object") return;
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
          searchInputRef.current?.focus();
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

  return (
    <main className="shell">
      <audio
        ref={audioRef}
        onPlay={() => setIsPlaying(true)}
        onPause={() => setIsPlaying(false)}
        onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => setDuration(event.currentTarget.duration)}
        onEnded={() => playRelative(1)}
      />
      <aside className="sidebar" aria-label="Library">
        <div className="brand">Darkwave</div>
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
          <button
            className={activeFilter === filter.id ? "nav-item active" : "nav-item"}
            key={filter.label}
            onClick={() => setActiveFilter(filter.id)}
          >
            {filter.label}
          </button>
        ))}
        <div className="nav-heading">Projects</div>
        {collections.map((project) => (
          <button
            className={
              typeof activeFilter === "object" && activeFilter.project === project.id ? "nav-item active" : "nav-item"
            }
            key={project.id}
            onClick={() => setActiveFilter({ project: project.id })}
          >
            {project.name}
          </button>
        ))}
        <div className="new-project-row">
          <input
            placeholder="New project"
            value={newProjectName}
            onChange={(event) => setNewProjectName(event.target.value)}
          />
          <button type="button" onClick={handleCreateProject} disabled={!newProjectName.trim()}>
            +
          </button>
        </div>
      </aside>
      <section className="workspace">
        <header className="topbar">
          <label className="search">
            <Search size={16} />
            <input
              ref={searchInputRef}
              placeholder="Search sounds, tags, source, license"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
            />
          </label>
          <button className="icon-button" aria-label="Filter" disabled title="Use the sidebar filters">
            <ListFilter size={17} />
          </button>
          <button className="text-button" aria-label="Undo" onClick={handleUndo} disabled={undoStack.length === 0}>
            Undo
          </button>
          <button className="text-button" aria-label="Redo" onClick={handleRedo} disabled={redoStack.length === 0}>
            Redo
          </button>
          <button
            className="text-button"
            type="button"
            onClick={handleExportLicenseReport}
            disabled={typeof activeFilter !== "object"}
            title="Export a CSV license report for the active project"
          >
            License Report
          </button>
          <button className="primary-action" type="button" onClick={handleImportFolder}>
            <Import size={16} />
            Import
          </button>
        </header>
        {queryFilters.length > 0 ? (
          <div className="tag-grid" aria-label="Parsed search filters">
            {queryFilters.map((filter, index) => (
              <span key={index} className="suggestion-chip">
                {filter.field} {filter.operator} {filter.value}
              </span>
            ))}
          </div>
        ) : null}
        <section className="onboarding-strip" aria-label="Library setup">
          <button type="button" onClick={handleImportFolder}>
            <Import size={16} />
            Import Folder
          </button>
          <span>{importStatus ?? "Referenced import — files stay where they are"}</span>
          <span>
            {activeLibrary ? activeLibrary.media_root : ""}{" "}
            {mediaRootStatus ? `(${mediaRootStatus.status})` : ""}
          </span>
        </section>
        <section className="command-strip" aria-label="Command palette preview">
          <Command size={15} />
          <button onClick={handleImportFolder}>Import Folder</button>
          <button onClick={() => searchInputRef.current?.focus()}>Focus Search</button>
          <button onClick={() => document.getElementById("tags-section")?.scrollIntoView({ behavior: "smooth" })}>
            Apply Tag
          </button>
          <button onClick={handleExportSelected} disabled={!selectedAssetId}>
            Export Selected
          </button>
          <button onClick={() => document.getElementById("settings-section")?.scrollIntoView({ behavior: "smooth" })}>
            Open Settings
          </button>
        </section>
        <section className="browser" aria-label="Sound browser">
          <div className="selection-bar" aria-label="Selection actions">
            <strong>{selectedAsset ? "1 selected" : "0 selected"}</strong>
            <span>Click a row to select</span>
            <span>Click a tag or project to apply it</span>
          </div>
          <div className="virtualization-bar" aria-label="Browser performance">
            <span>{visibleAssets.length} row{visibleAssets.length === 1 ? "" : "s"}</span>
            <span>Not yet virtualized</span>
          </div>
          <div className="browser-header">
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
              </article>
              );
            })
          )}
        </section>
      </section>
      <aside className="inspector" aria-label="Inspector">
        <div className="inspector-head">
          <h1>{selectedAsset?.display_name ?? "No sound selected"}</h1>
          <button className="icon-button" aria-label="Settings" onClick={() => document.getElementById("settings-section")?.scrollIntoView({ behavior: "smooth" })}>
            <Settings size={17} />
          </button>
        </div>
        {selectedCount > 1 ? (
          <section aria-label="Bulk actions">
            <h2>{selectedCount} Selected</h2>
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
          </section>
        ) : null}
        {selectedAsset ? (
          <section>
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
          </section>
        ) : null}
        <section id="tags-section">
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
        </section>
        <section>
          <h2>Projects</h2>
          <div className="drop-target-grid">
            {collections.map((project) => (
              <button key={project.id} onClick={() => handleAddSelectedToProject(project)} disabled={bulkAssetIds.length === 0}>
                + {project.name}
              </button>
            ))}
          </div>
        </section>
        {selectedAsset && (selectedAsset.embedded_title || selectedAsset.embedded_genre || selectedAsset.embedded_comment) ? (
          <section>
            <h2>Embedded Metadata</h2>
            <div className="settings-stack">
              {selectedAsset.embedded_title ? <div className="status-line">Title: {selectedAsset.embedded_title}</div> : null}
              {selectedAsset.embedded_genre ? <div className="status-line">Genre: {selectedAsset.embedded_genre}</div> : null}
              {selectedAsset.embedded_comment ? <div className="status-line">Comment: {selectedAsset.embedded_comment}</div> : null}
            </div>
          </section>
        ) : null}
        <section>
          <h2>Source &amp; License</h2>
          {sourceDraft ? (
            <div className="settings-stack">
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
              <button type="button" className="primary-action" onClick={handleSaveSource}>
                <Save size={15} />
                Save source
              </button>
            </div>
          ) : (
            <span className="empty-hint">Select a sound to edit source and license</span>
          )}
          {exportStatus ? <div className="status-line">{exportStatus}</div> : null}
        </section>
        <section id="settings-section">
          <h2>Settings</h2>
          <div className="settings-stack">
            <div className="setting-row">
              <SlidersHorizontal size={15} />
              <span>Browser density</span>
              <strong>{preferences?.browser_density ?? "…"}</strong>
            </div>
            <div className="setting-row">
              <Volume2 size={15} />
              <span>Output route</span>
              <strong>{preferences?.output_device === "SystemDefault" ? "System default" : "Custom device"}</strong>
            </div>
            <div className="setting-row">
              <Gauge size={15} />
              <span>Preview cache</span>
              <strong>{preferences ? `${(preferences.preview_cache_limit_mb / 1024).toFixed(0)} GB` : "…"}</strong>
            </div>
            <div className="setting-row">
              <Save size={15} />
              <span>Settings file</span>
              <strong>Saved to preferences.json</strong>
            </div>
          </div>
        </section>
        <section>
          <h2>Shortcuts</h2>
          <div className="shortcut-list">
            {(preferences?.shortcuts.bindings ?? []).map((item) => (
              <div className="shortcut-row" key={item.command}>
                <span>{item.command}</span>
                <kbd>{item.accelerator}</kbd>
              </div>
            ))}
          </div>
        </section>
        <section>
          <h2>Accessibility</h2>
          <div className="toggle-list">
            <label>
              <input
                type="checkbox"
                checked={preferences?.reduced_transparency ?? false}
                onChange={handleToggleReducedTransparency}
              />
              <Contrast size={15} />
              Reduced transparency
            </label>
            <label>
              <input
                type="checkbox"
                checked={preferences?.reduced_motion ?? false}
                onChange={handleToggleReducedMotion}
              />
              <Zap size={15} />
              Reduced motion
            </label>
          </div>
        </section>
        <section>
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
            <ShieldCheck size={15} />
            Distribution gates tracked
          </div>
          <div className="status-line">
            <Bell size={15} />
            {updateChannelState === "Passed" ? "Update channel ready" : "Update channel planned"}
          </div>
        </section>
        <section>
          <h2>Maintenance</h2>
          <div className="maintenance-list">
            {Object.entries(maintenanceLabels).map(([kind, label]) => (
              <div className="maintenance-row" key={kind}>
                <span>{label}</span>
                <strong>{maintenanceReport?.counts_by_kind[kind] ?? 0}</strong>
              </div>
            ))}
          </div>
          {maintenanceReport && maintenanceReport.findings.some((finding) => finding.kind === "DuplicateContent") ? (
            <div className="settings-stack">
              {maintenanceReport.findings
                .filter((finding) => finding.kind === "DuplicateContent")
                .map((finding, index) => (
                  <div className="status-line" key={index}>
                    {finding.asset_ids.length} duplicate files sharing content
                    <button type="button" className="text-button" onClick={() => handleTrashDuplicateGroup(finding.asset_ids)}>
                      Keep oldest, trash rest
                    </button>
                  </div>
                ))}
            </div>
          ) : null}
        </section>
        <section>
          <h2>NAS &amp; Offline</h2>
          {offlineControl ? (
            <>
              <div className="status-line">
                {offlineControl.media_root} — {mediaRootStatus?.status ?? "unknown"}
                {offlineControl.catalog_only ? " (catalog only)" : ""}
              </div>
              <div className="drop-target-grid">
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
        </section>
        <section>
          <h2>Trash</h2>
          <div className="status-line">{trashRetentionDays} day retention before explicit purge</div>
          {trashItems.length === 0 ? (
            <span className="empty-hint">Trash is empty</span>
          ) : (
            <div className="settings-stack">
              {trashItems.map((item) => (
                <div className="maintenance-row" key={item.asset_id}>
                  <span>{item.original_path.split("/").pop()}</span>
                  <button type="button" onClick={() => handleRestoreFromTrash(item)}>
                    Restore
                  </button>
                  <button type="button" onClick={() => handlePurgeTrashItem(item)}>
                    Purge
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
        <section>
          <h2>Backup</h2>
          <button type="button" className="text-button" onClick={handleBackupLibrary} disabled={!activeLibraryId}>
            Back Up Library
          </button>
          <button type="button" className="text-button" onClick={handleRestoreLibrary}>
            Restore From Backup…
          </button>
          <div className="status-line">{backupStatus ?? "Copies the catalog snapshot and manifest to a folder you choose"}</div>
        </section>
      </aside>
      <footer className="transport" aria-label="Transport">
        <button className="icon-button" aria-label="Previous" onClick={() => playRelative(-1)}>
          <SkipBack size={17} />
        </button>
        <button className="transport-play" aria-label="Play or pause" onClick={togglePlayback}>
          {isPlaying ? <Pause size={18} /> : <Play size={18} />}
        </button>
        <button className="icon-button" aria-label="Next" onClick={() => playRelative(1)}>
          <SkipForward size={17} />
        </button>
        <div className="transport-waveform" aria-hidden="true">
          {(peaks ?? []).map((peak, i) => (
            <span key={i} style={{ height: `${4 + peak * 96}%` }} />
          ))}
        </div>
        <span className="time">
          {formatTime(currentTime)} / {formatTime(duration)}
        </span>
      </footer>
    </main>
  );
}
