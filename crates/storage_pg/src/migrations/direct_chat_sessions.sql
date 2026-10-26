-- crates/storage_pg/src/migrations/direct_chat_sessions.sql -- Direct chat sessions and member preferences
-- 2026/10/11 17:17 Created.

CREATE SCHEMA IF NOT EXISTS im_2024;

CREATE TABLE IF NOT EXISTS im_2024.chat_sessions
(
    session_id UUID PRIMARY KEY,
    channel SMALLINT NOT NULL DEFAULT 1,
    status SMALLINT NOT NULL DEFAULT 1,
    user_low BIGINT NOT NULL,
    user_high BIGINT NOT NULL,
    last_message_id TEXT,
    last_message_summary TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Session state: 1 = normal; other values are inactive/reserved.
    CONSTRAINT chat_sessions_direct_channel CHECK (channel = 1),
    CONSTRAINT chat_sessions_valid_participants CHECK (user_low > 0 AND user_high > user_low),
    CONSTRAINT chat_sessions_unique_pair UNIQUE (user_low, user_high)
);

CREATE TABLE IF NOT EXISTS im_2024.chat_session_members
(
    session_id UUID NOT NULL REFERENCES im_2024.chat_sessions(session_id) ON DELETE CASCADE,
    account_id BIGINT NOT NULL,
    -- Member state: 1 = active; other values are inactive/reserved.
    member_state SMALLINT NOT NULL DEFAULT 1,
    is_pinned BOOLEAN NOT NULL DEFAULT FALSE,
    notification_settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    unread_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (session_id, account_id)
);

CREATE INDEX IF NOT EXISTS idx_chat_session_members_account_state
    ON im_2024.chat_session_members (account_id, member_state, session_id);

CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated_at
    ON im_2024.chat_sessions (updated_at DESC);