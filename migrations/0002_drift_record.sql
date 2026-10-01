-- Drift records, version 2 (VER-23, VER-77).
--
-- One row per `DriftKey`. The record is stored as the JSON `DriftRecord`
-- serialises to rather than column-wise: `DriftKind` is `#[non_exhaustive]`,
-- so a column mapping would need a catch-all arm, and a catch-all in a
-- persistence layer silently drops any kind added later — a worse failure
-- than not persisting, because it looks like it worked. The derived impl has
-- no catch-all, so a new variant round-trips the moment it compiles.
--
-- `open` is lifted out of the document so the dashboard's and the digest's
-- read — which is only ever the open ones — is an index scan rather than a
-- deserialise-and-filter over every record the site has ever had.
CREATE TABLE drift_record (
    key         TEXT PRIMARY KEY,
    open        INTEGER NOT NULL,
    document    TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX drift_record_open ON drift_record(open);
