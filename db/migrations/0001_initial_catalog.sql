PRAGMA foreign_keys = ON;

CREATE TABLE libraries (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  media_root TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE assets (
  id TEXT PRIMARY KEY,
  library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  original_filename TEXT NOT NULL,
  display_name TEXT NOT NULL,
  relative_path TEXT,
  referenced_path TEXT,
  storage_mode TEXT NOT NULL CHECK (storage_mode IN ('managed', 'referenced', 'hybrid')),
  content_hash TEXT,
  perceptual_fingerprint TEXT,
  media_type TEXT NOT NULL DEFAULT 'other',
  duration_ms INTEGER,
  sample_rate INTEGER,
  bit_depth INTEGER,
  channels INTEGER,
  file_size INTEGER NOT NULL DEFAULT 0,
  loudness_lufs REAL,
  peak_db REAL,
  bpm REAL,
  bpm_confidence REAL,
  musical_key TEXT,
  key_confidence REAL,
  waveform_version INTEGER NOT NULL DEFAULT 0,
  availability_state TEXT NOT NULL DEFAULT 'unknown',
  review_state TEXT NOT NULL DEFAULT 'unreviewed',
  date_added TEXT NOT NULL,
  last_seen TEXT,
  last_played TEXT,
  play_count INTEGER NOT NULL DEFAULT 0,
  export_count INTEGER NOT NULL DEFAULT 0,
  favorite INTEGER NOT NULL DEFAULT 0,
  notes TEXT
);

CREATE TABLE tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  normalized_name TEXT NOT NULL UNIQUE,
  facet TEXT,
  parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
  preferred_term_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
  is_system INTEGER NOT NULL DEFAULT 0,
  is_hidden INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE asset_tags (
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  origin TEXT NOT NULL CHECK (origin IN ('filename', 'metadata', 'acoustic_model', 'user_rule', 'user_correction', 'manual')),
  confidence REAL NOT NULL DEFAULT 1.0,
  approval_state TEXT NOT NULL CHECK (approval_state IN ('suggested', 'accepted', 'rejected')),
  created_at TEXT NOT NULL,
  PRIMARY KEY (asset_id, tag_id, origin)
);

CREATE TABLE source_records (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  provider TEXT,
  source_url TEXT,
  downloaded_at TEXT,
  license_type TEXT,
  license_status TEXT,
  attribution TEXT,
  restrictions TEXT,
  receipt_path TEXT,
  notes TEXT
);

CREATE TABLE collections (
  id TEXT PRIMARY KEY,
  library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  type TEXT NOT NULL CHECK (type IN ('manual', 'smart', 'project')),
  query_definition TEXT,
  parent_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL,
  archived_at TEXT
);

CREATE TABLE usage_events (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  project_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
  event_type TEXT NOT NULL CHECK (event_type IN ('played', 'exported', 'dragged', 'copied', 'used')),
  destination TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE background_jobs (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 100,
  state TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE library_sync_records (
  entity_id TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  revision INTEGER NOT NULL,
  device_id TEXT NOT NULL,
  changed_at TEXT NOT NULL,
  payload_hash TEXT NOT NULL,
  PRIMARY KEY (entity_id, entity_type)
);

CREATE VIRTUAL TABLE assets_fts USING fts5(
  display_name,
  original_filename,
  notes,
  content='assets',
  content_rowid='rowid'
);

CREATE INDEX idx_assets_library ON assets(library_id);
CREATE INDEX idx_assets_media_type ON assets(media_type);
CREATE INDEX idx_assets_availability ON assets(availability_state);
CREATE INDEX idx_assets_content_hash ON assets(content_hash);
CREATE INDEX idx_usage_events_asset ON usage_events(asset_id);
CREATE UNIQUE INDEX idx_assets_library_hash_size
  ON assets(library_id, content_hash, file_size)
  WHERE content_hash IS NOT NULL;
CREATE INDEX idx_background_jobs_pending ON background_jobs(state, priority, created_at);
