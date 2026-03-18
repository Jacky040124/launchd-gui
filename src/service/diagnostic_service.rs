use crate::domain::diagnostic::{DiagnosticIssue, DiagnosticSeverity};
use crate::domain::plist_document::StandardPlistDocument;

#[derive(Debug, Default)]
pub struct DiagnosticService;

impl DiagnosticService {
    pub fn analyze(&self, document: &StandardPlistDocument) -> Vec<DiagnosticIssue> {
        let mut issues = Vec::new();

        if document.label.trim().is_empty() {
            issues.push(issue(
                DiagnosticSeverity::Error,
                "Label",
                "缺少 Label，launchd 无法识别任务标识",
                "填写唯一 Label（例如 com.example.worker）",
            ));
        } else {
            if document.label.contains(' ') {
                issues.push(issue(
                    DiagnosticSeverity::Error,
                    "Label",
                    "Label 不能包含空格",
                    "使用点号或连字符分隔，例如 com.example.worker",
                ));
            }
            if !document.label.starts_with("com.") {
                issues.push(issue(
                    DiagnosticSeverity::Info,
                    "Label",
                    "Label 建议使用反向域名前缀",
                    "将 Label 改为 com.<team>.<service> 风格",
                ));
            }
        }

        if document.program.trim().is_empty() {
            issues.push(issue(
                DiagnosticSeverity::Error,
                "Program",
                "缺少 Program 可执行路径",
                "填写可执行文件绝对路径",
            ));
        } else if !document.program.starts_with('/') {
            issues.push(issue(
                DiagnosticSeverity::Warning,
                "Program",
                "Program 不是绝对路径",
                "改为绝对路径，例如 /usr/local/bin/task",
            ));
        } else if document.program.ends_with(".app") {
            issues.push(issue(
                DiagnosticSeverity::Warning,
                "Program",
                "Program 指向 .app 包，通常不可直接执行",
                "指向 app 内具体可执行文件",
            ));
        }

        if document.program_arguments.is_empty() {
            issues.push(issue(
                DiagnosticSeverity::Info,
                "ProgramArguments",
                "未配置 ProgramArguments",
                "按需添加参数，第一项通常为程序路径",
            ));
        } else if let Some(first) = document.program_arguments.first() {
            if first != &document.program {
                issues.push(issue(
                    DiagnosticSeverity::Warning,
                    "ProgramArguments",
                    "ProgramArguments 首项与 Program 不一致",
                    "若无需特殊处理，建议首项与 Program 保持一致",
                ));
            }
        }

        if document.start_interval == Some(0) {
            issues.push(issue(
                DiagnosticSeverity::Error,
                "StartInterval",
                "StartInterval 不能为 0",
                "设置为大于 0 的秒数，或删除该字段",
            ));
        }

        if document.start_interval.is_some() && document.keep_alive {
            issues.push(issue(
                DiagnosticSeverity::Warning,
                "KeepAlive/StartInterval",
                "KeepAlive 与 StartInterval 同时开启可能产生意外重启行为",
                "二者通常二选一，按任务语义保留一个",
            ));
        }

        if !document.run_at_load && !document.keep_alive && document.start_interval.is_none() {
            issues.push(issue(
                DiagnosticSeverity::Warning,
                "RunAtLoad/KeepAlive/StartInterval",
                "当前配置可能不会被自动触发",
                "开启 RunAtLoad，或配置 KeepAlive/StartInterval",
            ));
        }

        if let Some(working_directory) = &document.working_directory {
            if !working_directory.is_empty() && !working_directory.starts_with('/') {
                issues.push(issue(
                    DiagnosticSeverity::Warning,
                    "WorkingDirectory",
                    "WorkingDirectory 不是绝对路径",
                    "改为绝对路径（例如 /Users/you/project）",
                ));
            }
        }

        if document.environment_variables.is_empty() {
            issues.push(issue(
                DiagnosticSeverity::Info,
                "EnvironmentVariables",
                "未配置环境变量",
                "如任务依赖 PATH/TZ/代理等可在此补充",
            ));
        }

        for key in document.environment_variables.keys() {
            if key.trim().is_empty() {
                issues.push(issue(
                    DiagnosticSeverity::Error,
                    "EnvironmentVariables",
                    "存在空的环境变量键",
                    "删除空键并使用 KEY=VALUE 形式",
                ));
            }
            if key.contains(' ') {
                issues.push(issue(
                    DiagnosticSeverity::Warning,
                    "EnvironmentVariables",
                    "环境变量键包含空格",
                    "改为不含空格的键名（例如 APP_ENV）",
                ));
            }
            if key != &key.to_ascii_uppercase() {
                issues.push(issue(
                    DiagnosticSeverity::Info,
                    "EnvironmentVariables",
                    "环境变量键建议统一大写",
                    "将键名改为大写以提高可读性",
                ));
            }
        }

        if !document.environment_variables.contains_key("PATH") {
            issues.push(issue(
                DiagnosticSeverity::Info,
                "EnvironmentVariables",
                "未显式设置 PATH",
                "若依赖系统命令，建议显式设置 PATH",
            ));
        }

        issues
    }
}

fn issue(
    severity: DiagnosticSeverity,
    key: &str,
    message: &str,
    suggestion: &str,
) -> DiagnosticIssue {
    DiagnosticIssue {
        severity,
        key: key.to_string(),
        message: message.to_string(),
        suggestion: suggestion.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::domain::diagnostic::DiagnosticSeverity;
    use crate::domain::plist_document::StandardPlistDocument;

    use super::DiagnosticService;

    #[test]
    fn analysis_reports_multiple_findings() {
        let service = DiagnosticService;
        let document = StandardPlistDocument {
            label: "bad label".to_string(),
            program: "relative/path".to_string(),
            program_arguments: vec![],
            run_at_load: false,
            keep_alive: false,
            start_interval: Some(0),
            working_directory: Some("tmp".to_string()),
            environment_variables: BTreeMap::from([("my key".to_string(), "1".to_string())]),
        };

        let issues = service.analyze(&document);
        assert!(issues.len() >= 8);
        assert!(issues
            .iter()
            .any(|issue| issue.severity == DiagnosticSeverity::Error));
        assert!(issues.iter().any(|issue| issue.key == "WorkingDirectory"));
    }
}
