import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Bell,
  Command,
  Contrast,
  Gauge,
  Import,
  ListFilter,
  MonitorCheck,
  Pause,
  Play,
  RotateCcw,
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
import { useCallback, useEffect, useState } from "react";

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
};

type ImportFailure = {
  filename: string;
  reason: string;
};

type ImportFolderResult = {
  imported: AssetRecord[];
  failed: ImportFailure[];
};

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
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

const shortcutItems = [
  { command: "Play/Pause", binding: "Space" },
  { command: "Next sound", binding: "Arrow Down" },
  { command: "Previous sound", binding: "Arrow Up" },
  { command: "Favorite", binding: "F" },
  { command: "Command palette", binding: "Mod K" },
  { command: "Import", binding: "Mod I" }
];

const dragTargets = ["Tag", "Collection", "Project", "Favorite", "Trash", "External Export"];

const paletteCommands = ["Import Folder", "Focus Search", "Apply Tag", "Export Selected", "Open Settings"];

const maintenanceItems = [
  { label: "Missing media", value: "0" },
  { label: "License review", value: "0" },
  { label: "Waveform cache", value: "0" },
  { label: "Duplicates", value: "Review" }
];

const duplicateActions = ["Keep", "Link", "Merge", "Replace", "Trash"];

export function App() {
  const [releaseItems, setReleaseItems] = useState(fallbackReleaseItems);
  const updateChannelState = releaseItems.find((item) => item.blocker === "update_system")?.state ?? "Planned";

  const [librariesLoaded, setLibrariesLoaded] = useState(false);
  const [libraries, setLibraries] = useState<LibraryRecord[]>([]);
  const [activeLibraryId, setActiveLibraryId] = useState<string | null>(null);
  const [assets, setAssets] = useState<AssetRecord[]>([]);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [libraryName, setLibraryName] = useState("");
  const [libraryRoot, setLibraryRoot] = useState("");
  const [importStatus, setImportStatus] = useState<string | null>(null);

  const selectedAsset = assets.find((asset) => asset.id === selectedAssetId) ?? null;

  const refreshAssets = useCallback((libraryId: string, query: string) => {
    const request = query.trim().length > 0
      ? invoke<AssetRecord[]>("search_assets", { libraryId, query })
      : invoke<AssetRecord[]>("list_assets", { libraryId });

    request.then(setAssets).catch(() => setAssets([]));
  }, []);

  useEffect(() => {
    invoke<ReleaseReadinessItem[]>("release_readiness_items")
      .then(setReleaseItems)
      .catch(() => setReleaseItems(fallbackReleaseItems));
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
    if (!activeLibraryId) return;
    const timeout = setTimeout(() => refreshAssets(activeLibraryId, searchQuery), 200);
    return () => clearTimeout(timeout);
  }, [activeLibraryId, searchQuery, refreshAssets]);

  useEffect(() => {
    if (selectedAssetId && !assets.some((asset) => asset.id === selectedAssetId)) {
      setSelectedAssetId(null);
    }
  }, [assets, selectedAssetId]);

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

  const handleImportFolder = async () => {
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
      refreshAssets(activeLibraryId, searchQuery);
    } catch (error) {
      setImportStatus(`Import failed: ${String(error)}`);
    }
  };

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
      <aside className="sidebar" aria-label="Library">
        <div className="brand">Darkwave</div>
        {["All Sounds", "Inbox", "Recently Added", "Favorites", "Unreviewed", "Missing Files", "Music", "Sound Effects", "Ambience", "Projects"].map((item) => (
          <button className={item === "All Sounds" ? "nav-item active" : "nav-item"} key={item}>
            {item}
          </button>
        ))}
      </aside>
      <section className="workspace">
        <header className="topbar">
          <label className="search">
            <Search size={16} />
            <input
              placeholder="Search sounds, tags, source, license"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
            />
          </label>
          <button className="icon-button" aria-label="Filter">
            <ListFilter size={17} />
          </button>
          <button className="icon-button" aria-label="Command palette">
            <Command size={17} />
          </button>
          <button className="primary-action" type="button" onClick={handleImportFolder}>
            <Import size={16} />
            Import
          </button>
        </header>
        <section className="onboarding-strip" aria-label="Library setup">
          <button type="button" onClick={handleImportFolder}>
            <Import size={16} />
            Import Folder
          </button>
          <span>{importStatus ?? "Referenced import — files stay where they are"}</span>
          <button disabled title="Not wired up yet">
            <Zap size={16} />
            Open NAS Library
          </button>
          <span>NAS probe and offline controls ready</span>
          <button disabled title="Not wired up yet">
            <RotateCcw size={16} />
            Restore Session
          </button>
        </section>
        <section className="command-strip" aria-label="Command palette preview">
          <Command size={15} />
          {paletteCommands.map((command) => (
            <button key={command}>{command}</button>
          ))}
        </section>
        <section className="browser" aria-label="Sound browser">
          <div className="selection-bar" aria-label="Selection actions">
            <strong>{selectedAsset ? "1 selected" : "0 selected"}</strong>
            <span>Shift range</span>
            <span>Mod additive</span>
            <span>Drag to classify or export</span>
          </div>
          <div className="virtualization-bar" aria-label="Browser performance">
            <span>{assets.length} row{assets.length === 1 ? "" : "s"}</span>
            <span>Not yet virtualized</span>
          </div>
          <div className="browser-header">
            <span>Name</span>
            <span>Type</span>
            <span>Storage</span>
            <span>Size</span>
            <span>Status</span>
          </div>
          {assets.length === 0 ? (
            <p className="empty-browser">No sounds yet. Use Import Folder to add some.</p>
          ) : (
            assets.map((asset) => (
              <article
                className={asset.id === selectedAssetId ? "asset-row selected" : "asset-row"}
                key={asset.id}
                onClick={() => setSelectedAssetId(asset.id)}
              >
                <button className="play-cell" aria-label={`Preview ${asset.display_name}`} disabled title="Playback not wired up yet">
                  <Play size={15} />
                </button>
                <div className="waveform" aria-hidden="true">
                  {Array.from({ length: 32 }).map((_, i) => (
                    <span key={i} style={{ height: `${22 + (i * 17) % 42}%` }} />
                  ))}
                </div>
                <strong>{asset.display_name}</strong>
                <span>{asset.media_type}</span>
                <span>{asset.storage_mode}</span>
                <span>{formatFileSize(asset.file_size)}</span>
                <span>{asset.availability_state}</span>
                <button className="favorite" aria-label={`Favorite ${asset.display_name}`} disabled title="Favoriting not wired up yet">
                  <Star size={15} fill={asset.favorite ? "currentColor" : "none"} />
                </button>
              </article>
            ))
          )}
        </section>
      </section>
      <aside className="inspector" aria-label="Inspector">
        <div className="inspector-head">
          <h1>{selectedAsset?.display_name ?? "No sound selected"}</h1>
          <button className="icon-button" aria-label="Settings">
            <Settings size={17} />
          </button>
        </div>
        <section>
          <h2>Suggested Tags</h2>
          <div className="tag-grid">
            {["Impact", "Metal", "Cinematic", "Dark", "High Energy"].map((tag) => (
              <button key={tag}>{tag}</button>
            ))}
          </div>
        </section>
        <section>
          <h2>Drop Targets</h2>
          <div className="drop-target-grid">
            {dragTargets.map((target) => (
              <button key={target}>{target}</button>
            ))}
          </div>
        </section>
        <section>
          <h2>Source & License</h2>
          <dl>
            <dt>Status</dt>
            <dd>Needs review</dd>
            <dt>Storage</dt>
            <dd>Referenced NAS path</dd>
            <dt>Preview</dt>
            <dd>Cached WAV decode ready</dd>
            <dt>Receipt</dt>
            <dd>Attached to report row</dd>
          </dl>
          <div className="warning-line">Export allowed with license warning</div>
          <div className="status-line">Decode cancellation active</div>
          <div className="status-line">Original copy export executable</div>
          <div className="status-line">Ranged WAV render ready from PCM</div>
          <div className="status-line">External drag payload ready after copy</div>
        </section>
        <section>
          <h2>Settings</h2>
          <div className="settings-stack">
            <div className="setting-row">
              <SlidersHorizontal size={15} />
              <span>Browser density</span>
              <strong>Compact</strong>
            </div>
            <div className="setting-row">
              <Volume2 size={15} />
              <span>Output route</span>
              <strong>Handle bound</strong>
            </div>
            <div className="setting-row">
              <Gauge size={15} />
              <span>Preview cache</span>
              <strong>16 GB</strong>
            </div>
            <div className="setting-row">
              <Save size={15} />
              <span>Settings file</span>
              <strong>JSON</strong>
            </div>
          </div>
        </section>
        <section>
          <h2>Shortcuts</h2>
          <div className="shortcut-list">
            {shortcutItems.map((item) => (
              <div className="shortcut-row" key={item.command}>
                <span>{item.command}</span>
                <kbd>{item.binding}</kbd>
              </div>
            ))}
          </div>
        </section>
        <section>
          <h2>Accessibility</h2>
          <div className="toggle-list">
            <label>
              <input type="checkbox" />
              <Contrast size={15} />
              Reduced transparency
            </label>
            <label>
              <input type="checkbox" />
              <Zap size={15} />
              Reduced motion
            </label>
          </div>
        </section>
        <section>
          <h2>Recovery</h2>
          <div className="status-line">
            <RotateCcw size={15} />
            Autosave revision 42 available
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
            {maintenanceItems.map((item) => (
              <div className="maintenance-row" key={item.label}>
                <MonitorCheck size={15} />
                <span>{item.label}</span>
                <strong>{item.value}</strong>
              </div>
            ))}
          </div>
        </section>
        <section>
          <h2>Duplicate Review</h2>
          <div className="duplicate-actions">
            {duplicateActions.map((action) => (
              <button key={action}>{action}</button>
            ))}
          </div>
        </section>
        <section>
          <h2>Trash</h2>
          <div className="status-line">30 day retention before explicit purge</div>
          <div className="status-line">Restore keeps original asset identity</div>
        </section>
        <section>
          <h2>Backup</h2>
          <div className="status-line">Catalog snapshot required</div>
          <div className="status-line">Portable manifest required</div>
          <div className="status-line">Media root verified on restore</div>
        </section>
      </aside>
      <footer className="transport" aria-label="Transport">
        <button className="icon-button" aria-label="Previous">
          <SkipBack size={17} />
        </button>
        <button className="transport-play" aria-label="Play or pause">
          <Pause size={18} />
        </button>
        <button className="icon-button" aria-label="Next">
          <SkipForward size={17} />
        </button>
        <div className="transport-waveform" aria-hidden="true">
          {Array.from({ length: 96 }).map((_, i) => (
            <span key={i} style={{ height: `${18 + ((i * 13) % 66)}%` }} />
          ))}
        </div>
        <span className="time">0:01 / 0:02</span>
        <span className="time">1.00x</span>
        <span className="time">80 ms start</span>
      </footer>
    </main>
  );
}
