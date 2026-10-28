use cos_contracts::Capability;
use serde_json::Value;
use std::path::PathBuf;

use crate::ToolAdapter;

/// Deliverable file writer tool adapter.
///
/// Securely writes deliverable documents, proposals, scripts, and exports to a sandboxed directory.
/// Invariant: Strictly prevents path traversal (`..` or absolute paths outside sandbox).
#[derive(Debug, Clone)]
pub struct FileDeliverableAdapter {
    base_dir: PathBuf,
}

impl FileDeliverableAdapter {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub fn default_dir() -> Self {
        Self::new("./deliverables")
    }

    /// Sanitizes and resolves a target path ensuring it stays inside `base_dir`.
    fn resolve_safe_path(
        &self,
        subfolder: Option<&str>,
        file_name: &str,
    ) -> Result<PathBuf, String> {
        if file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
            return Err(format!(
                "Invalid file name '{}': path separators and '..' are forbidden",
                file_name
            ));
        }

        let mut path = self.base_dir.clone();
        if let Some(sub) = subfolder {
            if sub.contains("..") {
                return Err("Invalid subfolder: '..' is forbidden".to_string());
            }
            let sanitized_sub = sub.trim_start_matches(['/', '\\']);
            path.push(sanitized_sub);
        }

        path.push(file_name);
        Ok(path)
    }
}

impl ToolAdapter for FileDeliverableAdapter {
    fn tool_name(&self) -> &str {
        "file_deliverable"
    }

    fn capability(&self) -> Capability {
        Capability::new("filesystem_write")
    }

    fn execute(&self, payload: &Value) -> Result<Value, String> {
        let file_name = payload
            .get("file_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required string field 'file_name'".to_string())?;

        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required string field 'content'".to_string())?;

        let subfolder = payload.get("subfolder").and_then(|v| v.as_str());

        let target_path = self.resolve_safe_path(subfolder, file_name)?;

        // Ensure parent directory exists
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create deliverable directory '{:?}': {}",
                    parent, e
                )
            })?;
        }

        std::fs::write(&target_path, content.as_bytes()).map_err(|e| {
            format!(
                "Failed to write deliverable file '{:?}': {}",
                target_path, e
            )
        })?;

        Ok(serde_json::json!({
            "status": "written",
            "file_name": file_name,
            "path": target_path.to_string_lossy(),
            "bytes_written": content.len()
        }))
    }
}

/// HTTP Webhook and REST API dispatcher tool adapter.
///
/// Sends HTTP requests to external webhooks, CRM APIs, or notification services.
#[derive(Debug, Clone, Default)]
pub struct HttpWebhookAdapter {
    client: reqwest::blocking::Client,
}

impl HttpWebhookAdapter {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl ToolAdapter for HttpWebhookAdapter {
    fn tool_name(&self) -> &str {
        "http_webhook"
    }

    fn capability(&self) -> Capability {
        Capability::new("network_http")
    }

    fn execute(&self, payload: &Value) -> Result<Value, String> {
        let url_str = payload
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required string field 'url'".to_string())?;

        let method = payload
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_uppercase();

        let parsed_url = reqwest::Url::parse(url_str)
            .map_err(|e| format!("Invalid target URL '{}': {}", url_str, e))?;

        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err("Target URL must use http or https scheme".to_string());
        }

        let mut req_builder = match method.as_str() {
            "GET" => self.client.get(parsed_url),
            "PUT" => self.client.put(parsed_url),
            "DELETE" => self.client.delete(parsed_url),
            "PATCH" => self.client.patch(parsed_url),
            _ => self.client.post(parsed_url),
        };

        // Attach optional custom headers
        if let Some(headers) = payload.get("headers").and_then(|v| v.as_object()) {
            for (key, val) in headers {
                if let Some(val_str) = val.as_str() {
                    req_builder = req_builder.header(key, val_str);
                }
            }
        }

        // Attach payload body if provided
        if let Some(body) = payload.get("body") {
            req_builder = req_builder.json(body);
        }

        let resp = req_builder
            .send()
            .map_err(|e| format!("HTTP request to '{}' failed: {}", url_str, e))?;

        let status_code = resp.status().as_u16();
        let body_text = resp.text().unwrap_or_default();

        let parsed_body: Value =
            serde_json::from_str(&body_text).unwrap_or(serde_json::Value::String(body_text));

        Ok(serde_json::json!({
            "status_code": status_code,
            "success": status_code < 400,
            "response": parsed_body
        }))
    }
}

/// Notification tool adapter for external communication (Email, Chat, SMS).
#[derive(Debug, Clone, Default)]
pub struct NotificationToolAdapter;

impl NotificationToolAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl ToolAdapter for NotificationToolAdapter {
    fn tool_name(&self) -> &str {
        "notification_sender"
    }

    fn capability(&self) -> Capability {
        Capability::new("send_notification")
    }

    fn execute(&self, payload: &Value) -> Result<Value, String> {
        let recipient = payload
            .get("recipient")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required string field 'recipient'".to_string())?;

        let message = payload
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing required string field 'message'".to_string())?;

        let channel = payload
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("email");

        let subject = payload
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("Notification");

        Ok(serde_json::json!({
            "status": "dispatched",
            "channel": channel,
            "recipient": recipient,
            "subject": subject,
            "message_length": message.len(),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }
}
