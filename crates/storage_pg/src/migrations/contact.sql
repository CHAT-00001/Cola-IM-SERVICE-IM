-- 创建独立模式，保持数据库整洁
CREATE SCHEMA IF NOT EXISTS im_2024;

-- 1. 联系人档案主表
CREATE TABLE im_2024.contact_profile
(
    id           BIGSERIAL PRIMARY KEY,                           -- 内部自增关联 ID
    show_id      UUID         NOT NULL DEFAULT gen_random_uuid(), -- 对外暴露 ID
    status       SMALLINT     NOT NULL DEFAULT 1,                 -- 0.失效 1.正常 2.异常 3.封禁
    name         VARCHAR(128) NOT NULL,                           -- 基础名称
    name_zh      VARCHAR(128),                                    -- 中文名
    name_en      VARCHAR(128),                                    -- 英文名
    first_name   VARCHAR(64),                                     -- 姓
    last_name    VARCHAR(64),                                     -- 名
    avatar_url   TEXT,                                            -- 头像
    gender       SMALLINT,                                        -- 性别
    birthday     TIMESTAMPTZ,                                     -- 生日
    telephone    VARCHAR(32),                                     -- 电话
    email        VARCHAR(128),                                    -- 邮箱
    wechat       VARCHAR(64),                                     -- 微信
    whatsapp     VARCHAR(64),                                     -- WhatsApp
    telegram     VARCHAR(64),                                     -- 电报
    session_id   BIGINT       NOT NULL DEFAULT 0,                 -- 会话关联
    hometown     VARCHAR(255),                                    -- 家乡
    occupation   VARCHAR(128),                                    -- 职业
    description  TEXT,                                            -- 描述
    location_id  BIGINT,                                          -- 现居地 ID
    contact_type SMALLINT              DEFAULT 0,                 -- 类型
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),             -- 创建时间
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()              -- 更新时间
);

-- 2. 联系人设置/状态表
CREATE TABLE im_2024.contact_status
(
    contact_id  BIGINT PRIMARY KEY REFERENCES im_2024.contact_profile (id) ON DELETE CASCADE,
    status      SMALLINT    NOT NULL DEFAULT 1,
    is_discover BOOLEAN              DEFAULT TRUE, -- 默认允许发现
    is_apply    BOOLEAN              DEFAULT TRUE, -- 默认需要申请
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. 通讯录关系表
CREATE TABLE im_2024.contact_list
(
    id           BIGSERIAL PRIMARY KEY,
    status       SMALLINT    NOT NULL DEFAULT 1,
    user_id      BIGINT      NOT NULL,               -- 发起者（对应 profile.id）
    target_id    BIGINT      NOT NULL,               -- 目标（对应 profile.id）
    alias        VARCHAR(128),                       -- 备注名
    remark       TEXT,                               -- 详细备注
    is_starred   BOOLEAN     NOT NULL DEFAULT FALSE, -- 默认非星标
    is_deleted   BOOLEAN     NOT NULL DEFAULT FALSE, -- 默认未删除
    contact_type SMALLINT,
    apply_at     TIMESTAMPTZ,
    added_at     TIMESTAMPTZ,
    sync_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at   TIMESTAMPTZ
);

-- 自动更新 updated_at 的触发器函数
CREATE
OR REPLACE FUNCTION update_timestamp_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at
= NOW();
RETURN NEW;
END;
$$
language 'plpgsql';

-- 为三张表绑定触发器
CREATE TRIGGER update_profile_modtime
    BEFORE UPDATE
    ON im_2024.contact_profile
    FOR EACH ROW EXECUTE PROCEDURE update_timestamp_column();
CREATE TRIGGER update_status_modtime
    BEFORE UPDATE
    ON im_2024.contact_status
    FOR EACH ROW EXECUTE PROCEDURE update_timestamp_column();
CREATE TRIGGER update_list_modtime
    BEFORE UPDATE
    ON im_2024.contact_list
    FOR EACH ROW EXECUTE PROCEDURE update_timestamp_column();