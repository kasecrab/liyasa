-- The analytics database, version 1 (ANA-02, ANA-08). Separate file and
-- connection pool so event writes never contend with page-serving reads.

CREATE TABLE event (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    ts            INTEGER NOT NULL,
    site          TEXT NOT NULL,
    env           TEXT NOT NULL,
    route         TEXT NOT NULL,
    type          TEXT NOT NULL,
    variant       TEXT NOT NULL DEFAULT '{}',
    caller        TEXT NOT NULL DEFAULT '{}',
    format        TEXT NOT NULL DEFAULT 'html',
    session_key   TEXT NOT NULL DEFAULT '',
    referrer_host TEXT,
    device        TEXT NOT NULL DEFAULT '{}',
    country       TEXT,
    duration_ms   INTEGER,
    props         TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX event_ts ON event(ts);
CREATE INDEX event_site_route_ts ON event(site, route, ts);

-- Dashboard time series read this, never the raw table (ANA-08).
CREATE TABLE agg_hour (
    hour        INTEGER NOT NULL,
    site        TEXT NOT NULL,
    env         TEXT NOT NULL,
    route       TEXT NOT NULL,
    type        TEXT NOT NULL,
    caller_kind TEXT NOT NULL,
    format      TEXT NOT NULL,
    count       INTEGER NOT NULL,
    PRIMARY KEY (hour, site, env, route, type, caller_kind, format)
);

CREATE TABLE ingest_drops (
    day         INTEGER NOT NULL,
    class       TEXT NOT NULL,
    count       INTEGER NOT NULL,
    PRIMARY KEY (day, class)
);
