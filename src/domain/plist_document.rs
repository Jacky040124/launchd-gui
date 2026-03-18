use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorMode {
    Standard,
    Expert,
    Xml,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchdKeyDef {
    pub key: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandardPlistDocument {
    pub label: String,
    pub program: String,
    pub program_arguments: Vec<String>,
    pub run_at_load: bool,
    pub keep_alive: bool,
    pub start_interval: Option<u64>,
    pub working_directory: Option<String>,
    pub environment_variables: BTreeMap<String, String>,
    pub extra_string_keys: BTreeMap<String, String>,
}

impl StandardPlistDocument {
    pub fn to_program_arguments_line(&self) -> String {
        self.program_arguments.join(" ")
    }
}

pub const STANDARD_KEY_DEFS: &[LaunchdKeyDef] = &[
    LaunchdKeyDef {
        key: "Label",
        note: "任务唯一标识，通常使用反向域名",
    },
    LaunchdKeyDef {
        key: "Program",
        note: "可执行文件绝对路径",
    },
    LaunchdKeyDef {
        key: "ProgramArguments",
        note: "程序参数数组（第一个通常是可执行文件）",
    },
    LaunchdKeyDef {
        key: "RunAtLoad",
        note: "加载后立即运行",
    },
    LaunchdKeyDef {
        key: "KeepAlive",
        note: "保持常驻或按策略重启",
    },
    LaunchdKeyDef {
        key: "StartInterval",
        note: "按秒间隔执行",
    },
    LaunchdKeyDef {
        key: "StartCalendarInterval",
        note: "按日历时间执行",
    },
    LaunchdKeyDef {
        key: "WorkingDirectory",
        note: "工作目录",
    },
    LaunchdKeyDef {
        key: "EnvironmentVariables",
        note: "环境变量字典",
    },
    LaunchdKeyDef {
        key: "StandardInPath",
        note: "stdin 重定向路径",
    },
    LaunchdKeyDef {
        key: "StandardOutPath",
        note: "stdout 重定向路径",
    },
    LaunchdKeyDef {
        key: "StandardErrorPath",
        note: "stderr 重定向路径",
    },
    LaunchdKeyDef {
        key: "UserName",
        note: "以指定用户运行（daemon 常用）",
    },
    LaunchdKeyDef {
        key: "GroupName",
        note: "以指定组运行",
    },
    LaunchdKeyDef {
        key: "RootDirectory",
        note: "chroot 根目录",
    },
    LaunchdKeyDef {
        key: "Umask",
        note: "文件创建掩码",
    },
    LaunchdKeyDef {
        key: "ProcessType",
        note: "进程类型（Background/Adaptive 等）",
    },
    LaunchdKeyDef {
        key: "AbandonProcessGroup",
        note: "退出时不清理进程组",
    },
    LaunchdKeyDef {
        key: "ThrottleInterval",
        note: "重启节流时间",
    },
    LaunchdKeyDef {
        key: "TimeOut",
        note: "超时时间",
    },
    LaunchdKeyDef {
        key: "ExitTimeOut",
        note: "退出等待超时",
    },
    LaunchdKeyDef {
        key: "WatchPaths",
        note: "监控路径变化触发",
    },
    LaunchdKeyDef {
        key: "QueueDirectories",
        note: "目录有内容时触发",
    },
    LaunchdKeyDef {
        key: "PathState",
        note: "路径状态触发条件",
    },
    LaunchdKeyDef {
        key: "MachServices",
        note: "Mach 服务注册",
    },
    LaunchdKeyDef {
        key: "Sockets",
        note: "socket 激活配置",
    },
    LaunchdKeyDef {
        key: "inetdCompatibility",
        note: "兼容 inetd 行为",
    },
    LaunchdKeyDef {
        key: "LaunchOnlyOnce",
        note: "仅运行一次",
    },
    LaunchdKeyDef {
        key: "LegacyTimers",
        note: "兼容旧定时器行为",
    },
    LaunchdKeyDef {
        key: "LowPriorityIO",
        note: "低优先级 IO",
    },
    LaunchdKeyDef {
        key: "Nice",
        note: "调度优先级",
    },
    LaunchdKeyDef {
        key: "SoftResourceLimits",
        note: "软资源限制",
    },
    LaunchdKeyDef {
        key: "HardResourceLimits",
        note: "硬资源限制",
    },
    LaunchdKeyDef {
        key: "SessionCreate",
        note: "为任务创建会话",
    },
    LaunchdKeyDef {
        key: "AssociatedBundleIdentifiers",
        note: "关联 app bundle 标识",
    },
    LaunchdKeyDef {
        key: "Disabled",
        note: "默认禁用标志",
    },
];

pub fn search_key_defs(keyword: &str) -> Vec<&'static LaunchdKeyDef> {
    let normalized = keyword.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return STANDARD_KEY_DEFS.iter().collect();
    }

    STANDARD_KEY_DEFS
        .iter()
        .filter(|def| {
            def.key.to_ascii_lowercase().contains(&normalized)
                || def.note.to_ascii_lowercase().contains(&normalized)
        })
        .collect()
}

pub fn is_standard_managed_key(key: &str) -> bool {
    matches!(
        key,
        "Label"
            | "Program"
            | "ProgramArguments"
            | "RunAtLoad"
            | "KeepAlive"
            | "StartInterval"
            | "WorkingDirectory"
            | "EnvironmentVariables"
    )
}

#[cfg(test)]
mod tests {
    use super::search_key_defs;

    #[test]
    fn key_defs_support_search() {
        let defs = search_key_defs("interval");
        assert!(!defs.is_empty());
        assert!(defs.iter().any(|item| item.key == "StartInterval"));
    }
}
