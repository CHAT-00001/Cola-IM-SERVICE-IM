// crates/app_config/src/kits/snow.rs -- 板条箱 - 应用配置 - 工具包 - 雪花ID
// 2026/9/11 07:40 Created.

////////

use snowflake::ProcessUniqueId;

////////

/// 生成进程内唯一ID
pub fn next_id() -> ProcessUniqueId {
    ProcessUniqueId::new()
}

//////// END
