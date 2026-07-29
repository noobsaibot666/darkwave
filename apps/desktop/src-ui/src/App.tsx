import { Command, Import, ListFilter, Pause, Play, Search, Settings, SkipBack, SkipForward, Star } from "lucide-react";

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
