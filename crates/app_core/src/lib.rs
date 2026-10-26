// app_core/src/lib.rs  -- 应用核心模块入口
// 2026-03-24 04:18

////////

pub mod logic; // 逻辑
pub mod models; // 模型
pub mod ports; // RUST trait
pub mod usecase; // 用例

////////

/// # [] -
fn main() {
    println!("APP is running!");
}

//////// END