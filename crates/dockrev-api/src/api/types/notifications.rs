use super::*;

#[derive(Clone, Debug)]
pub struct NotificationSettings {
    pub email_enabled: bool,
    pub email_smtp_url: Option<String>,
    pub webhook_enabled: bool,
    pub webhook_url: Option<String>,
    pub telegram_enabled: bool,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub webpush_enabled: bool,
    pub webpush_vapid_public_key: Option<String>,
    pub webpush_vapid_private_key: Option<String>,
    pub webpush_vapid_subject: Option<String>,
    pub event_update_enabled: bool,
    pub event_new_version_enabled: bool,
    pub event_ghcr_webhook_anomaly_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    pub email: EmailNotification,
    pub webhook: WebhookNotification,
    pub telegram: TelegramNotification,
    pub web_push: WebPushNotification,
    #[serde(default)]
    pub events: Option<NotificationEventsConfig>,
}

impl NotificationConfig {
    pub fn from_db(db: NotificationSettings) -> Self {
        Self {
            email: EmailNotification {
                enabled: db.email_enabled,
                smtp_url: mask_if_some(db.email_smtp_url),
            },
            webhook: WebhookNotification {
                enabled: db.webhook_enabled,
                url: mask_if_some(db.webhook_url),
            },
            telegram: TelegramNotification {
                enabled: db.telegram_enabled,
                bot_token: None,
                bot_token_configured: is_non_empty(db.telegram_bot_token.as_deref()),
                chat_id: db.telegram_chat_id,
            },
            web_push: WebPushNotification {
                enabled: db.webpush_enabled,
                vapid_public_key: db.webpush_vapid_public_key,
                vapid_private_key: mask_if_some(db.webpush_vapid_private_key),
                vapid_subject: db.webpush_vapid_subject,
            },
            events: Some(NotificationEventsConfig {
                update: db.event_update_enabled,
                new_version: db.event_new_version_enabled,
                ghcr_webhook_anomaly: db.event_ghcr_webhook_anomaly_enabled,
            }),
        }
    }

    pub fn into_db(self) -> NotificationSettings {
        let events = self.events.unwrap_or_default();
        NotificationSettings {
            email_enabled: self.email.enabled,
            email_smtp_url: self.email.smtp_url,
            webhook_enabled: self.webhook.enabled,
            webhook_url: self.webhook.url,
            telegram_enabled: self.telegram.enabled,
            telegram_bot_token: self.telegram.bot_token,
            telegram_chat_id: self.telegram.chat_id,
            webpush_enabled: self.web_push.enabled,
            webpush_vapid_public_key: self.web_push.vapid_public_key,
            webpush_vapid_private_key: self.web_push.vapid_private_key,
            webpush_vapid_subject: self.web_push.vapid_subject,
            event_update_enabled: events.update,
            event_new_version_enabled: events.new_version,
            event_ghcr_webhook_anomaly_enabled: events.ghcr_webhook_anomaly,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationEventsConfig {
    #[serde(default = "notification_event_default_true")]
    pub update: bool,
    #[serde(default = "notification_event_default_true")]
    pub new_version: bool,
    #[serde(default = "notification_event_default_true")]
    pub ghcr_webhook_anomaly: bool,
}

impl Default for NotificationEventsConfig {
    fn default() -> Self {
        Self {
            update: true,
            new_version: true,
            ghcr_webhook_anomaly: true,
        }
    }
}

fn notification_event_default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smtp_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub bot_token_configured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushNotification {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_private_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vapid_subject: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutNotificationsResponse {
    pub ok: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationsRequest {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub channel: Option<NotificationTestChannel>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestNotificationsResponse {
    pub ok: bool,
    pub results: Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationTestChannel {
    Email,
    Webhook,
    Telegram,
    WebPush,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscriptionRequest {
    pub endpoint: String,
    pub keys: WebPushKeys,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushKeys {
    pub p256dh: String,
    pub auth: String,
}
