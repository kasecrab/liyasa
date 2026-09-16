-- Application schema, version 1 (PRD §6.5, §6.13, HOST-07).
-- Timestamps are integer milliseconds since the Unix epoch. Every entity row
-- carries `version` for the optimistic check `Repo::put` documents.

CREATE TABLE organization (
    id          TEXT PRIMARY KEY,
    slug        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE project (
    id          TEXT PRIMARY KEY,
    org_id      TEXT REFERENCES organization(id),
    slug        TEXT NOT NULL,
    name        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX project_slug ON project(slug);

CREATE TABLE build (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    env         TEXT NOT NULL,
    status      TEXT NOT NULL,
    dist        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX build_project_env ON build(project_id, env, created_at);

-- The pointer an environment serves; history keeps every pointer swap so a
-- rollback is a swap back (REST-01).
CREATE TABLE deployment (
    project_id  TEXT NOT NULL REFERENCES project(id),
    env         TEXT NOT NULL,
    build_id    TEXT NOT NULL REFERENCES build(id),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (project_id, env)
);
CREATE TABLE deployment_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id  TEXT NOT NULL,
    env         TEXT NOT NULL,
    build_id    TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE domain (
    host        TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    base_path   TEXT NOT NULL DEFAULT '',
    env         TEXT NOT NULL DEFAULT 'production',
    verified    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- A renamed project keeps its old host redirecting until `expires_at` (HOST-21).
CREATE TABLE domain_redirect (
    old_host    TEXT PRIMARY KEY,
    new_host    TEXT NOT NULL,
    expires_at  INTEGER NOT NULL
);

CREATE TABLE job (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    key          TEXT NOT NULL,
    priority     INTEGER NOT NULL DEFAULT 0,
    state        TEXT NOT NULL,
    project_id   TEXT,
    payload      TEXT NOT NULL DEFAULT '{}',
    attempts     INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    run_at       INTEGER NOT NULL,
    lease_ms     INTEGER NOT NULL DEFAULT 60000,
    lease_until  INTEGER,
    worker       TEXT,
    result       TEXT,
    error        TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1
);
-- De-duplication: one live job per (name, key); done, failed and dead rows
-- do not block a new trigger.
CREATE UNIQUE INDEX job_live_key ON job(name, key) WHERE state IN ('queued', 'leased');
CREATE INDEX job_runnable ON job(state, priority, run_at);

CREATE TABLE feedback (
    id          TEXT PRIMARY KEY,
    project_id  TEXT,
    route       TEXT NOT NULL,
    kind        TEXT NOT NULL,
    rating      INTEGER,
    category    TEXT,
    text        TEXT,
    block_id    TEXT,
    task        TEXT,
    status      TEXT NOT NULL DEFAULT 'open',
    notes       TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX feedback_route ON feedback(route, created_at);
CREATE INDEX feedback_status ON feedback(status, created_at);

CREATE TABLE secret (
    name        TEXT PRIMARY KEY,
    key_id      TEXT NOT NULL,
    nonce       BLOB NOT NULL,
    ciphertext  BLOB NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE webhook_subscription (
    id          TEXT PRIMARY KEY,
    project_id  TEXT,
    url         TEXT NOT NULL,
    secret      TEXT NOT NULL,
    events      TEXT NOT NULL DEFAULT '[]',
    active      INTEGER NOT NULL DEFAULT 1,
    failures    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE webhook_delivery (
    id              TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL REFERENCES webhook_subscription(id),
    event_id        TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    payload         TEXT NOT NULL,
    attempt         INTEGER NOT NULL DEFAULT 0,
    next_at         INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    last_status     INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX webhook_delivery_due ON webhook_delivery(status, next_at);

CREATE TABLE audit_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    at          INTEGER NOT NULL,
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    subject     TEXT NOT NULL,
    detail      TEXT NOT NULL DEFAULT ''
);
