import {
  Bell,
  Command,
  Contrast,
  Gauge,
  Import,
  ListFilter,
  Pause,
  Play,
  RotateCcw,
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

type AssetRow = {
  name: string;
  type: string;
  duration: string;
  energy: string;
  source: string;
};

const rows: AssetRow[] = [
  { name: "Dark Metallic Impact 03", type: "Sound Effect", duration: "0:02", energy: "High", source: "Referenced" },
  { name: "Low Room Tone Warehouse", type: "Ambience", duration: "2:14", energy: "Subtle", source: "Cached" },
  { name: "Slow Analog Pulse 92 BPM", type: "Music", duration: "1:48", energy: "Medium", source: "Local" }
];

const releaseItems = [
  { label: "macOS audit", state: "Passed" },
  { label: "Windows audit", state: "Passed" },
  { label: "Accessibility", state: "Passed" },
  { label: "Performance", state: "Passed" },
  { label: "Updates", state: "Planned" },
  { label: "Signing", state: "Planned" }
];

const shortcutItems = [
  { command: "Play/Pause", binding: "Space" },
  { command: "Next sound", binding: "Arrow Down" },
  { command: "Previous sound", binding: "Arrow Up" },
  { command: "Favorite", binding: "F" },
  { command: "Command palette", binding: "Mod K" },
  { command: "Import", binding: "Mod I" }
];

export function App() {
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
            <input placeholder="Search sounds, tags, source, license" />
          </label>
          <button className="icon-button" aria-label="Filter">
            <ListFilter size={17} />
          </button>
          <button className="icon-button" aria-label="Command palette">
            <Command size={17} />
          </button>
          <button className="primary-action">
            <Import size={16} />
            Import
          </button>
        </header>
        <section className="onboarding-strip" aria-label="Library setup">
          <button>
            <Import size={16} />
            Import Folder
          </button>
          <button>
            <Zap size={16} />
            Open NAS Library
          </button>
          <button>
            <RotateCcw size={16} />
            Restore Session
          </button>
        </section>
        <section className="browser" aria-label="Sound browser">
          <div className="browser-header">
            <span>Name</span>
            <span>Type</span>
            <span>Duration</span>
            <span>Energy</span>
            <span>Source</span>
          </div>
          {rows.map((row, index) => (
            <article className={index === 0 ? "asset-row selected" : "asset-row"} key={row.name}>
              <button className="play-cell" aria-label={`Preview ${row.name}`}>
                {index === 0 ? <Pause size={15} /> : <Play size={15} />}
              </button>
              <div className="waveform" aria-hidden="true">
                {Array.from({ length: 32 }).map((_, i) => (
                  <span key={i} style={{ height: `${22 + ((i * 17 + index * 9) % 42)}%` }} />
                ))}
              </div>
              <strong>{row.name}</strong>
              <span>{row.type}</span>
              <span>{row.duration}</span>
              <span>{row.energy}</span>
              <span>{row.source}</span>
              <button className="favorite" aria-label={`Favorite ${row.name}`}>
                <Star size={15} />
              </button>
            </article>
          ))}
        </section>
      </section>
      <aside className="inspector" aria-label="Inspector">
        <div className="inspector-head">
          <h1>Dark Metallic Impact 03</h1>
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
          <h2>Source & License</h2>
          <dl>
            <dt>Status</dt>
            <dd>Needs review</dd>
            <dt>Storage</dt>
            <dd>Referenced NAS path</dd>
          </dl>
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
              <span>Output device</span>
              <strong>System</strong>
            </div>
            <div className="setting-row">
              <Gauge size={15} />
              <span>Preview cache</span>
              <strong>16 GB</strong>
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
            Update channel planned
          </div>
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
      </footer>
    </main>
  );
}
