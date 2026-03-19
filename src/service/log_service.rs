use std::sync::Arc;

use crate::adapter::log_stream::LogStreamClient;
use crate::error::AppResult;

pub struct LogService {
    client: Arc<dyn LogStreamClient>,
}

impl LogService {
    pub fn new(client: Arc<dyn LogStreamClient>) -> Self {
        Self { client }
    }

    pub fn recent_logs(&self, label: &str, minutes: u32, max_lines: usize) -> AppResult<String> {
        self.client.recent_logs_for_label(label, minutes, max_lines)
    }

    pub fn live_logs(&self, label: &str, seconds: u32, max_lines: usize) -> AppResult<String> {
        self.client.live_logs_for_label(label, seconds, max_lines)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::adapter::log_stream::LogStreamClient;
    use crate::error::AppResult;

    use super::LogService;

    #[derive(Debug)]
    struct MockLogClient;

    impl LogStreamClient for MockLogClient {
        fn recent_logs_for_label(
            &self,
            label: &str,
            _minutes: u32,
            _max_lines: usize,
        ) -> AppResult<String> {
            Ok(format!("{label}: line-1\n{label}: line-2"))
        }

        fn live_logs_for_label(
            &self,
            label: &str,
            _seconds: u32,
            _max_lines: usize,
        ) -> AppResult<String> {
            Ok(format!("{label}: live-1\n{label}: live-2"))
        }
    }

    #[test]
    fn recent_logs_returns_client_output() {
        let service = LogService::new(Arc::new(MockLogClient));
        let logs = service
            .recent_logs("com.demo.service", 5, 200)
            .expect("logs");
        assert!(logs.contains("line-1"));
        assert!(logs.contains("com.demo.service"));
    }

    #[test]
    fn live_logs_returns_client_output() {
        let service = LogService::new(Arc::new(MockLogClient));
        let logs = service.live_logs("com.demo.service", 5, 200).expect("logs");
        assert!(logs.contains("live-1"));
    }
}
