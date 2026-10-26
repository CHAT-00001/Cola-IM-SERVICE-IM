-- crates/storage_pg/src/migrations/group_memberships.sql -- Group membership relation schema
-- 2026/10/7 21:46 Created.

CREATE SCHEMA IF NOT EXISTS im_2024;

CREATE TABLE IF NOT EXISTS im_2024.group_memberships
(
    group_id   TEXT        NOT NULL,
    account_id BIGINT      NOT NULL,
    status     SMALLINT    NOT NULL DEFAULT 1,
    joined_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_group_memberships_account_status
    ON im_2024.group_memberships (account_id, status, group_id);