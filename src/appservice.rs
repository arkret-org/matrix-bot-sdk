use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Context;
use async_trait::async_trait;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use regex::Regex;
use reqwest::Method;
use salvo::http::StatusCode;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex as AsyncMutex, RwLock, broadcast};
use url::{Url, form_urlencoded};
use uuid::Uuid;

use crate::client::{MatrixAuth, MatrixClient};
use crate::models::CreateRoom;
use crate::preprocessors::IPreprocessor;

pub mod http_responses {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct EmptyResponse {}

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ErrorResponse {
        pub error: String,
        pub errcode: String,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AppserviceTransaction {
    pub transaction_id: String,
    #[serde(default)]
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PingHomeserverResponse {
    pub duration_ms: f64,
}

#[async_trait]
pub trait AppserviceHandler: Send + Sync {
    async fn on_transaction(&self, _txn_id: &str, _body: &Value) -> anyhow::Result<()> {
        Ok(())
    }

    async fn query_user(&self, _user_id: &str) -> anyhow::Result<Option<Value>> {
        Ok(None)
    }

    async fn query_room_alias(&self, _room_alias: &str) -> anyhow::Result<Option<Value>> {
        Ok(None)
    }

    async fn query_key_claim(&self, _body: &Value) -> anyhow::Result<Option<Value>> {
        Ok(None)
    }

    async fn query_key(&self, _body: &Value) -> anyhow::Result<Option<Value>> {
        Ok(None)
    }

    async fn thirdparty_protocol(&self, _protocol: &str) -> anyhow::Result<Option<Value>> {
        Ok(None)
    }

    async fn thirdparty_user_remote(
        &self,
        _protocol: &str,
        _fields: &HashMap<String, String>,
    ) -> anyhow::Result<Vec<Value>> {
        Ok(Vec::new())
    }

    async fn thirdparty_user_matrix(&self, _user_id: &str) -> anyhow::Result<Vec<Value>> {
        Ok(Vec::new())
    }

    async fn thirdparty_location_remote(
        &self,
        _protocol: &str,
        _fields: &HashMap<String, String>,
    ) -> anyhow::Result<Vec<Value>> {
        Ok(Vec::new())
    }

    async fn thirdparty_location_matrix(&self, _alias: &str) -> anyhow::Result<Vec<Value>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Default)]
pub struct NoopAppserviceHandler;

#[async_trait]
impl AppserviceHandler for NoopAppserviceHandler {}

#[derive(Clone)]
pub struct Appservice {
    pub homeserver_token: String,
    pub appservice_token: String,
    pub client: MatrixClient,
    appservice_id: Option<String>,
    homeserver_name: Option<String>,
    user_prefix: Option<String>,
    alias_prefix: Option<String>,
    user_namespace_regexes: Vec<Regex>,
    alias_namespace_regexes: Vec<Regex>,
    protocols: Vec<String>,
    handlers: Arc<dyn AppserviceHandler>,
    preprocessors: Arc<RwLock<Vec<Arc<dyn IPreprocessor>>>>,
    intents_cache: Arc<StdMutex<HashMap<String, Intent>>>,
    pending_transactions: Arc<AsyncMutex<HashMap<String, broadcast::Sender<bool>>>>,
    completed_transactions: Arc<RwLock<HashSet<String>>>,
    ping_request: Arc<RwLock<Option<String>>>,
}

impl Default for Appservice {
    fn default() -> Self {
        let client = MatrixClient::new(
            Url::parse("http://localhost").expect("localhost url must be valid"),
            MatrixAuth::new(""),
        );
        Self::new("", "", client)
    }
}

impl Appservice {
    pub fn new(
        homeserver_token: impl Into<String>,
        appservice_token: impl Into<String>,
        client: MatrixClient,
    ) -> Self {
        Self {
            homeserver_token: homeserver_token.into(),
            appservice_token: appservice_token.into(),
            client,
            appservice_id: None,
            homeserver_name: None,
            user_prefix: None,
            alias_prefix: None,
            user_namespace_regexes: Vec::new(),
            alias_namespace_regexes: Vec::new(),
            protocols: Vec::new(),
            handlers: Arc::new(NoopAppserviceHandler),
            preprocessors: Arc::new(RwLock::new(Vec::new())),
            intents_cache: Arc::new(StdMutex::new(HashMap::new())),
            pending_transactions: Arc::new(AsyncMutex::new(HashMap::new())),
            completed_transactions: Arc::new(RwLock::new(HashSet::new())),
            ping_request: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_appservice_id(mut self, appservice_id: impl Into<String>) -> Self {
        self.appservice_id = Some(appservice_id.into());
        self
    }

    pub fn with_homeserver_name(mut self, homeserver_name: impl Into<String>) -> Self {
        self.homeserver_name = Some(homeserver_name.into());
        self
    }

    pub fn with_user_prefix(mut self, user_prefix: impl Into<String>) -> Self {
        self.user_prefix = Some(user_prefix.into());
        self
    }

    pub fn with_alias_prefix(mut self, alias_prefix: impl Into<String>) -> Self {
        self.alias_prefix = Some(alias_prefix.into());
        self
    }

    pub fn with_user_namespace_regexes<I, S>(mut self, regexes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.user_namespace_regexes = regexes
            .into_iter()
            .filter_map(|regex| Regex::new(&regex.into()).ok())
            .collect();
        self
    }

    pub fn with_alias_namespace_regexes<I, S>(mut self, regexes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.alias_namespace_regexes = regexes
            .into_iter()
            .filter_map(|regex| Regex::new(&regex.into()).ok())
            .collect();
        self
    }

    pub fn with_protocols<I, S>(mut self, protocols: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.protocols = protocols.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_handler(mut self, handler: Arc<dyn AppserviceHandler>) -> Self {
        self.handlers = handler;
        self
    }

    fn resolved_homeserver_name(&self) -> String {
        if let Some(name) = &self.homeserver_name {
            return name.clone();
        }
        let host = self.client.homeserver().host_str().unwrap_or("localhost");
        if let Some(port) = self.client.homeserver().port() {
            format!("{host}:{port}")
        } else {
            host.to_owned()
        }
    }

    pub fn get_intent(&self, localpart: &str) -> Intent {
        self.get_intent_for_user_id(&self.get_user_id(localpart))
    }

    pub fn get_user_id(&self, localpart: &str) -> String {
        format!("@{localpart}:{}", self.resolved_homeserver_name())
    }

    pub fn get_intent_for_suffix(&self, suffix: &str) -> anyhow::Result<Intent> {
        Ok(self.get_intent_for_user_id(&self.get_user_id_for_suffix(suffix)?))
    }

    pub fn get_user_id_for_suffix(&self, suffix: &str) -> anyhow::Result<String> {
        let Some(prefix) = &self.user_prefix else {
            anyhow::bail!("Cannot use get_user_id_for_suffix without a configured user prefix")
        };
        Ok(format!(
            "{prefix}{suffix}:{}",
            self.resolved_homeserver_name()
        ))
    }

    pub fn get_intent_for_user_id(&self, user_id: &str) -> Intent {
        if let Some(existing) = self
            .intents_cache
            .lock()
            .expect("intent cache lock poisoned")
            .get(user_id)
            .cloned()
        {
            return existing;
        }

        let intent = Intent::new(user_id.to_owned(), self.client.clone());
        self.intents_cache
            .lock()
            .expect("intent cache lock poisoned")
            .insert(user_id.to_owned(), intent.clone());
        intent
    }

    pub fn get_suffix_for_user_id(&self, user_id: &str) -> anyhow::Result<Option<String>> {
        let Some(prefix) = &self.user_prefix else {
            anyhow::bail!("Cannot use get_suffix_for_user_id without a configured user prefix")
        };
        Ok(self.suffix_for_entity(user_id, prefix))
    }

    pub fn is_namespaced_user(&self, user_id: &str) -> bool {
        if self
            .user_namespace_regexes
            .iter()
            .any(|regex| regex.is_match(user_id))
        {
            return true;
        }
        self.user_prefix
            .as_ref()
            .and_then(|prefix| self.suffix_for_entity(user_id, prefix))
            .is_some()
    }

    pub fn get_alias(&self, localpart: &str) -> String {
        format!("#{localpart}:{}", self.resolved_homeserver_name())
    }

    pub fn get_alias_for_suffix(&self, suffix: &str) -> anyhow::Result<String> {
        let Some(prefix) = &self.alias_prefix else {
            anyhow::bail!("Cannot use get_alias_for_suffix without a configured alias prefix")
        };
        Ok(format!(
            "{prefix}{suffix}:{}",
            self.resolved_homeserver_name()
        ))
    }

    pub fn get_alias_localpart_for_suffix(&self, suffix: &str) -> anyhow::Result<String> {
        let Some(prefix) = &self.alias_prefix else {
            anyhow::bail!(
                "Cannot use get_alias_localpart_for_suffix without a configured alias prefix"
            )
        };
        Ok(format!("{}{suffix}", prefix.trim_start_matches('#')))
    }

    pub fn get_suffix_for_alias(&self, alias: &str) -> anyhow::Result<Option<String>> {
        let Some(prefix) = &self.alias_prefix else {
            anyhow::bail!("Cannot use get_suffix_for_alias without a configured alias prefix")
        };
        Ok(self.suffix_for_entity(alias, prefix))
    }

    pub fn is_namespaced_alias(&self, alias: &str) -> bool {
        if self
            .alias_namespace_regexes
            .iter()
            .any(|regex| regex.is_match(alias))
        {
            return true;
        }
        self.alias_prefix
            .as_ref()
            .and_then(|prefix| self.suffix_for_entity(alias, prefix))
            .is_some()
    }

    pub async fn add_preprocessor(&self, preprocessor: Arc<dyn IPreprocessor>) {
        self.preprocessors.write().await.push(preprocessor);
    }

    pub async fn set_room_directory_visibility(
        &self,
        network_id: &str,
        room_id: &str,
        visibility: &str,
    ) -> anyhow::Result<Value> {
        if visibility != "public" && visibility != "private" {
            anyhow::bail!("visibility must be 'public' or 'private'");
        }
        let room_id = utf8_percent_encode(room_id, NON_ALPHANUMERIC).to_string();
        let network_id = utf8_percent_encode(network_id, NON_ALPHANUMERIC).to_string();
        self.client
            .raw_json(
                Method::PUT,
                &format!("/_matrix/client/v3/directory/list/appservice/{network_id}/{room_id}"),
                Some(json!({ "visibility": visibility })),
            )
            .await
    }

    pub fn router(&self) -> Router {
        build_router_with_appservice(self.clone())
    }

    /// Present for API parity with the TypeScript SDK where the appservice starts
    /// its own web server. In Rust, server startup is controlled by the host
    /// application using `router()`.
    pub async fn begin(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Present for API parity with the TypeScript SDK.
    pub fn stop(&self) {}

    pub async fn expect_ping_transaction(&self, transaction_id: impl Into<String>) {
        *self.ping_request.write().await = Some(transaction_id.into());
    }

    pub async fn expected_ping_transaction(&self) -> Option<String> {
        self.ping_request.read().await.clone()
    }

    pub async fn ping_homeserver(&self) -> anyhow::Result<PingHomeserverResponse> {
        let appservice_id = self
            .appservice_id
            .as_ref()
            .context("No `id` given in registration information. Cannot ping homeserver")?;
        let transaction_id = format!("matrix-bot-sdk_{}", Uuid::new_v4());
        self.expect_ping_transaction(transaction_id.clone()).await;
        let endpoint = format!(
            "/_matrix/client/v1/appservice/{}/ping",
            utf8_percent_encode(appservice_id, NON_ALPHANUMERIC)
        );
        let response = self
            .client
            .raw_json(
                Method::POST,
                &endpoint,
                Some(json!({ "transaction_id": transaction_id })),
            )
            .await?;
        serde_json::from_value(response).context("invalid ping response payload")
    }

    async fn acquire_transaction_slot(&self, txn_id: &str) -> TransactionSlot {
        if self.completed_transactions.read().await.contains(txn_id) {
            return TransactionSlot::Completed;
        }
        let mut pending = self.pending_transactions.lock().await;
        if let Some(sender) = pending.get(txn_id) {
            return TransactionSlot::Pending(sender.subscribe());
        }
        let (sender, _receiver) = broadcast::channel(1);
        pending.insert(txn_id.to_owned(), sender.clone());
        TransactionSlot::Owner(sender)
    }

    async fn mark_transaction_completed(&self, txn_id: &str) {
        self.completed_transactions
            .write()
            .await
            .insert(txn_id.to_owned());
    }

    async fn clear_pending_transaction(&self, txn_id: &str) {
        self.pending_transactions.lock().await.remove(txn_id);
    }

    async fn handle_transaction(&self, txn_id: &str, body: &Value) -> anyhow::Result<()> {
        let mut processed = body.clone();
        let preprocessors = self.preprocessors.read().await.clone();

        if !preprocessors.is_empty()
            && let Some(events) = processed.get_mut("events").and_then(Value::as_array_mut)
        {
            for event in events {
                for preprocessor in &preprocessors {
                    preprocessor.process(event).await?;
                }
            }
        }

        if !preprocessors.is_empty()
            && let Some(events) = processed
                .get_mut("de.sorunome.msc2409.ephemeral")
                .and_then(Value::as_array_mut)
        {
            for event in events {
                for preprocessor in &preprocessors {
                    preprocessor.process(event).await?;
                }
            }
        }

        self.handlers.on_transaction(txn_id, &processed).await
    }

    fn supports_protocol(&self, protocol: &str) -> bool {
        self.protocols.iter().any(|p| p == protocol)
    }

    fn suffix_for_entity(&self, value: &str, prefix: &str) -> Option<String> {
        let homeserver = self.resolved_homeserver_name();
        let suffix = format!(":{homeserver}");
        if !value.starts_with(prefix) || !value.ends_with(&suffix) {
            return None;
        }
        let middle = &value[prefix.len()..value.len().saturating_sub(suffix.len())];
        if middle.is_empty() {
            None
        } else {
            Some(middle.to_owned())
        }
    }
}

pub fn build_router() -> Router {
    build_router_with_appservice(Appservice::default())
}

pub fn build_router_with_appservice(appservice: Appservice) -> Router {
    Router::new()
        .hoop(InjectAppservice::new(appservice))
        .push(Router::with_path("health").get(health_handler))
        .push(Router::with_path("users/{user_id}").get(user_handler))
        .push(Router::with_path("rooms/{room_alias}").get(room_alias_handler))
        .push(Router::with_path("transactions/{txn_id}").put(transaction_handler))
        .push(Router::with_path("_matrix/app/v1/users/{user_id}").get(user_handler))
        .push(Router::with_path("_matrix/app/v1/rooms/{room_alias}").get(room_alias_handler))
        .push(Router::with_path("_matrix/app/v1/transactions/{txn_id}").put(transaction_handler))
        .push(
            Router::with_path("_matrix/app/v1/thirdparty/protocol/{protocol}")
                .get(thirdparty_protocol_handler),
        )
        .push(
            Router::with_path("_matrix/app/v1/thirdparty/user/{protocol}")
                .get(thirdparty_user_handler),
        )
        .push(Router::with_path("_matrix/app/v1/thirdparty/user").get(thirdparty_user_handler))
        .push(
            Router::with_path("_matrix/app/v1/thirdparty/location/{protocol}")
                .get(thirdparty_location_handler),
        )
        .push(
            Router::with_path("_matrix/app/v1/thirdparty/location")
                .get(thirdparty_location_handler),
        )
        .push(
            Router::with_path("_matrix/app/unstable/org.matrix.msc3983/keys/claim")
                .post(keys_claim_handler),
        )
        .push(
            Router::with_path("_matrix/app/unstable/org.matrix.msc3984/keys/query")
                .post(keys_query_handler),
        )
        .push(
            Router::with_path("_matrix/app/v1/unstable/org.matrix.msc3983/keys/claim")
                .post(keys_claim_handler),
        )
        .push(Router::with_path("unstable/org.matrix.msc3983/keys/claim").post(keys_claim_handler))
        .push(
            Router::with_path("_matrix/app/v1/unstable/org.matrix.msc3984/keys/query")
                .post(keys_query_handler),
        )
        .push(Router::with_path("unstable/org.matrix.msc3984/keys/query").post(keys_query_handler))
        .push(Router::with_path("_matrix/app/v1/ping").post(ping_handler))
        .push(Router::with_path("{**rest}").goal(unrecognized_handler))
}

#[derive(Clone)]
struct InjectAppservice {
    appservice: Appservice,
}

impl InjectAppservice {
    fn new(appservice: Appservice) -> Self {
        Self { appservice }
    }
}

#[handler]
impl InjectAppservice {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        depot.insert_typed(self.appservice.clone());
        ctrl.call_next(req, depot, res).await;
    }
}

enum TransactionSlot {
    Completed,
    Pending(broadcast::Receiver<bool>),
    Owner(broadcast::Sender<bool>),
}

#[derive(Copy, Clone)]
enum ThirdpartyObjectKind {
    User,
    Location,
}

fn appservice_from_depot(depot: &Depot) -> Option<Appservice> {
    depot.get_typed::<Appservice>().ok().cloned()
}

fn respond_error(res: &mut Response, status: StatusCode, errcode: &str, error: &str) {
    res.status_code(status);
    res.render(Json(json!({
        "errcode": errcode,
        "error": error,
    })));
}

fn respond_ok_empty(res: &mut Response) {
    res.status_code(StatusCode::OK);
    res.render(Json(json!({})));
}

fn provided_access_token(req: &Request) -> Option<String> {
    if let Some(auth_header) = req.header::<String>("authorization") {
        return auth_header.strip_prefix("Bearer ").map(ToOwned::to_owned);
    }
    req.query::<String>("access_token")
}

fn is_authed(appservice: &Appservice, req: &Request) -> bool {
    provided_access_token(req)
        .as_deref()
        .is_some_and(|token| token == appservice.homeserver_token)
}

fn require_auth(appservice: &Appservice, req: &Request, res: &mut Response) -> bool {
    if is_authed(appservice, req) {
        true
    } else {
        respond_error(
            res,
            StatusCode::UNAUTHORIZED,
            "AUTH_FAILED",
            "Authentication failed",
        );
        false
    }
}

async fn parse_json_object(req: &mut Request, res: &mut Response) -> Option<Value> {
    let body = match req.parse_json::<Value>().await {
        Ok(value) => value,
        Err(_) => {
            respond_error(res, StatusCode::BAD_REQUEST, "BAD_REQUEST", "Expected JSON");
            return None;
        }
    };
    if !body.is_object() {
        respond_error(res, StatusCode::BAD_REQUEST, "BAD_REQUEST", "Expected JSON");
        return None;
    }
    Some(body)
}

fn query_fields_without_access_token(req: &Request) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let Some(raw_query) = req.uri().query() else {
        return fields;
    };
    for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
        if key == "access_token" {
            continue;
        }
        fields.insert(key.into_owned(), value.into_owned());
    }
    fields
}

fn room_alias_localpart(room_alias: &str) -> String {
    room_alias
        .strip_prefix('#')
        .unwrap_or(room_alias)
        .split(':')
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[handler]
async fn health_handler(res: &mut Response) {
    res.render(Json(json!({"ok": true})));
}

#[handler]
async fn unrecognized_handler(res: &mut Response) {
    respond_error(
        res,
        StatusCode::NOT_FOUND,
        "M_UNRECOGNIZED",
        "Endpoint not implemented",
    );
}

#[handler]
async fn transaction_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }

    let Some(body) = parse_json_object(req, res).await else {
        return;
    };
    if body.get("events").and_then(Value::as_array).is_none() {
        respond_error(
            res,
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "Invalid JSON: expected events",
        );
        return;
    }

    let txn_id = req.param::<String>("txn_id").unwrap_or_default();
    match appservice.acquire_transaction_slot(&txn_id).await {
        TransactionSlot::Completed => {
            respond_ok_empty(res);
        }
        TransactionSlot::Pending(mut receiver) => match receiver.recv().await {
            Ok(true) => respond_ok_empty(res),
            _ => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(json!({})));
            }
        },
        TransactionSlot::Owner(sender) => {
            let success = appservice.handle_transaction(&txn_id, &body).await.is_ok();
            if success {
                appservice.mark_transaction_completed(&txn_id).await;
                respond_ok_empty(res);
            } else {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(json!({})));
            }
            let _ = sender.send(success);
            appservice.clear_pending_transaction(&txn_id).await;
        }
    }
}

#[handler]
async fn user_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }

    let user_id = req.param::<String>("user_id").unwrap_or_default();
    match appservice.handlers.query_user(&user_id).await {
        Ok(Some(result)) => {
            res.status_code(StatusCode::OK);
            res.render(Json(result));
        }
        Ok(None) => {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "USER_DOES_NOT_EXIST",
                "User not created",
            );
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Failed to query user",
            );
        }
    }
}

#[handler]
async fn room_alias_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }

    let room_alias = req.param::<String>("room_alias").unwrap_or_default();
    let result = match appservice.handlers.query_room_alias(&room_alias).await {
        Ok(value) => value,
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Failed to query room alias",
            );
            return;
        }
    };
    let Some(value) = result else {
        respond_error(
            res,
            StatusCode::NOT_FOUND,
            "ROOM_DOES_NOT_EXIST",
            "Room not created",
        );
        return;
    };

    let mut payload = match value {
        Value::Object(map) => map,
        _ => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Room query handler must return an object",
            );
            return;
        }
    };
    payload.insert(
        "room_alias_name".to_owned(),
        Value::String(room_alias_localpart(&room_alias)),
    );
    let create_room: CreateRoom = match serde_json::from_value(Value::Object(payload.clone())) {
        Ok(options) => options,
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Invalid room create options",
            );
            return;
        }
    };
    match appservice.client.create_room(&create_room).await {
        Ok(room_id) => {
            payload.insert("__roomId".to_owned(), Value::String(room_id));
            res.status_code(StatusCode::OK);
            res.render(Json(Value::Object(payload)));
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Failed to create room",
            );
        }
    }
}

#[handler]
async fn keys_claim_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }
    let Some(body) = parse_json_object(req, res).await else {
        return;
    };
    match appservice.handlers.query_key_claim(&body).await {
        Ok(Some(result)) => {
            res.status_code(StatusCode::OK);
            res.render(Json(result));
        }
        Ok(None) => {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "M_UNRECOGNIZED",
                "Endpoint not implemented",
            );
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Error handling key claim API",
            );
        }
    }
}

#[handler]
async fn keys_query_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }
    let Some(body) = parse_json_object(req, res).await else {
        return;
    };
    match appservice.handlers.query_key(&body).await {
        Ok(Some(result)) => {
            res.status_code(StatusCode::OK);
            res.render(Json(result));
        }
        Ok(None) => {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "M_UNRECOGNIZED",
                "Endpoint not implemented",
            );
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Error handling key query API",
            );
        }
    }
}

#[handler]
async fn thirdparty_protocol_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }

    let protocol = req.param::<String>("protocol").unwrap_or_default();
    if !appservice.supports_protocol(&protocol) {
        respond_error(
            res,
            StatusCode::NOT_FOUND,
            "PROTOCOL_NOT_HANDLED",
            "Protocol is not handled by this appservice",
        );
        return;
    }
    match appservice.handlers.thirdparty_protocol(&protocol).await {
        Ok(Some(result)) => {
            res.status_code(StatusCode::OK);
            res.render(Json(result));
        }
        Ok(None) => {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "M_UNRECOGNIZED",
                "Endpoint not implemented",
            );
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Failed to query third-party protocol",
            );
        }
    }
}

async fn handle_thirdparty_object(
    req: &Request,
    res: &mut Response,
    appservice: &Appservice,
    kind: ThirdpartyObjectKind,
) {
    let protocol = req.param::<String>("protocol");
    let matrix_id = match kind {
        ThirdpartyObjectKind::User => req.query::<String>("userid"),
        ThirdpartyObjectKind::Location => req.query::<String>("alias"),
    };
    let response_items = if let Some(protocol) = protocol {
        if !appservice.supports_protocol(&protocol) {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "PROTOCOL_NOT_HANDLED",
                "Protocol is not handled by this appservice",
            );
            return;
        }
        let fields = query_fields_without_access_token(req);
        match kind {
            ThirdpartyObjectKind::User => {
                appservice
                    .handlers
                    .thirdparty_user_remote(&protocol, &fields)
                    .await
            }
            ThirdpartyObjectKind::Location => {
                appservice
                    .handlers
                    .thirdparty_location_remote(&protocol, &fields)
                    .await
            }
        }
    } else if let Some(matrix_id) = matrix_id {
        match kind {
            ThirdpartyObjectKind::User => {
                appservice.handlers.thirdparty_user_matrix(&matrix_id).await
            }
            ThirdpartyObjectKind::Location => {
                appservice
                    .handlers
                    .thirdparty_location_matrix(&matrix_id)
                    .await
            }
        }
    } else {
        respond_error(
            res,
            StatusCode::BAD_REQUEST,
            "INVALID_PARAMETERS",
            "Invalid parameters given",
        );
        return;
    };

    match response_items {
        Ok(items) if items.is_empty() => {
            respond_error(
                res,
                StatusCode::NOT_FOUND,
                "NO_MAPPING_FOUND",
                "No mappings found",
            );
        }
        Ok(items) => {
            res.status_code(StatusCode::OK);
            res.render(Json(Value::Array(items)));
        }
        Err(_) => {
            respond_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_UNKNOWN",
                "Failed to process third-party lookup",
            );
        }
    }
}

#[handler]
async fn thirdparty_user_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }
    handle_thirdparty_object(req, res, &appservice, ThirdpartyObjectKind::User).await;
}

#[handler]
async fn thirdparty_location_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }
    handle_thirdparty_object(req, res, &appservice, ThirdpartyObjectKind::Location).await;
}

#[handler]
async fn ping_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(appservice) = appservice_from_depot(depot) else {
        respond_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "M_UNKNOWN",
            "Appservice state unavailable",
        );
        return;
    };
    if !require_auth(&appservice, req, res) {
        return;
    }
    let Some(body) = parse_json_object(req, res).await else {
        return;
    };

    let Some(expected) = appservice.expected_ping_transaction().await else {
        respond_error(
            res,
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "No ping request expected",
        );
        return;
    };
    let transaction_id = body
        .get("transaction_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if transaction_id != expected {
        respond_error(
            res,
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "transaction_id did not match",
        );
        return;
    }
    *appservice.ping_request.write().await = None;
    respond_ok_empty(res);
}

#[derive(Clone)]
pub struct Intent {
    user_id: String,
    client: MatrixClient,
    storage: Option<Arc<dyn crate::storage::IAppserviceStorageProvider>>,
}

impl Intent {
    pub fn new(user_id: impl Into<String>, client: MatrixClient) -> Self {
        Self {
            user_id: user_id.into(),
            client,
            storage: None,
        }
    }

    pub fn with_storage(
        mut self,
        storage: Arc<dyn crate::storage::IAppserviceStorageProvider>,
    ) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn underlying_client(&self) -> &MatrixClient {
        &self.client
    }

    pub async fn ensure_registered(&self) -> anyhow::Result<()> {
        if let Some(storage) = &self.storage
            && storage.is_user_registered(&self.user_id).await?
        {
            return Ok(());
        }

        let localpart = self
            .user_id
            .strip_prefix('@')
            .and_then(|s| s.split(':').next())
            .unwrap_or(&self.user_id);

        let result = self
            .client
            .raw_json(
                Method::POST,
                "/_matrix/client/v3/register",
                Some(json!({
                    "type": "m.login.application_service",
                    "username": localpart,
                    "inhibit_login": true,
                })),
            )
            .await;

        match result {
            Ok(_) => {
                if let Some(storage) = &self.storage {
                    storage.register_user(&self.user_id).await?;
                }
                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("M_USER_IN_USE") {
                    if let Some(storage) = &self.storage {
                        storage.register_user(&self.user_id).await?;
                    }
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    pub async fn ensure_joined(&self, room_id_or_alias: &str) -> anyhow::Result<String> {
        self.ensure_registered().await?;
        self.client.join_room(room_id_or_alias).await
    }

    pub async fn ensure_registered_and_joined(
        &self,
        room_id_or_alias: &str,
    ) -> anyhow::Result<String> {
        self.ensure_registered().await?;
        self.ensure_joined(room_id_or_alias).await
    }

    pub async fn get_joined_rooms(&self) -> anyhow::Result<Vec<String>> {
        self.ensure_registered().await?;
        self.client.get_joined_rooms().await
    }

    pub async fn refresh_joined_rooms(&self) -> anyhow::Result<Vec<String>> {
        self.get_joined_rooms().await
    }

    pub async fn join_room(&self, room_id_or_alias: &str) -> anyhow::Result<String> {
        self.ensure_registered().await?;
        self.client.join_room(room_id_or_alias).await
    }

    pub async fn leave_room(&self, room_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
        self.ensure_registered().await?;
        self.client.leave_room(room_id, reason).await
    }

    pub async fn send_text(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.ensure_registered_and_joined(room_id).await?;
        self.client.send_text(room_id, body).await
    }

    pub async fn send_notice(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.ensure_registered_and_joined(room_id).await?;
        self.client.send_notice(room_id, body).await
    }

    pub async fn send_event(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        self.ensure_registered_and_joined(room_id).await?;
        self.client.send_event(room_id, event_type, content).await
    }

    pub async fn send_state_event(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: &Value,
    ) -> anyhow::Result<String> {
        self.ensure_registered_and_joined(room_id).await?;
        self.client
            .send_state_event(room_id, event_type, state_key, content)
            .await
    }

    pub async fn send_message(&self, room_id: &str, body: &str) -> anyhow::Result<String> {
        self.send_text(room_id, body).await
    }

    pub async fn invite_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        self.ensure_registered().await?;
        self.client.invite_user(user_id, room_id).await
    }

    pub async fn kick_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        self.ensure_registered().await?;
        self.client.kick_user(user_id, room_id, reason).await
    }

    pub async fn ban_user(
        &self,
        user_id: &str,
        room_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        self.ensure_registered().await?;
        self.client.ban_user(user_id, room_id, reason).await
    }

    pub async fn unban_user(&self, user_id: &str, room_id: &str) -> anyhow::Result<()> {
        self.ensure_registered().await?;
        self.client.unban_user(user_id, room_id).await
    }

    pub async fn redact_event(
        &self,
        room_id: &str,
        event_id: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<String> {
        self.ensure_registered().await?;
        self.client.redact_event(room_id, event_id, reason).await
    }

    pub async fn get_room_members(
        &self,
        room_id: &str,
    ) -> anyhow::Result<Vec<crate::models::events::MembershipEvent>> {
        self.ensure_registered().await?;
        self.client.get_room_members(room_id, None, None).await
    }

    pub async fn enable_encryption(&self, room_id: &str) -> anyhow::Result<()> {
        self.ensure_registered_and_joined(room_id).await?;
        let content = json!({
            "algorithm": "m.megolm.v1.aes-sha2"
        });
        self.client
            .send_state_event(room_id, "m.room.encryption", "", &content)
            .await?;
        Ok(())
    }

    pub fn unstable_apis(&self) -> UnstableIntentApis {
        UnstableIntentApis {
            intent: self.clone(),
        }
    }
}

#[derive(Clone)]
pub struct UnstableIntentApis {
    intent: Intent,
}

impl UnstableIntentApis {
    pub async fn knock_room(
        &self,
        room_id_or_alias: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<String> {
        self.intent.ensure_registered().await?;
        let encoded = utf8_percent_encode(room_id_or_alias, NON_ALPHANUMERIC).to_string();
        let body = reason.map(|r| json!({ "reason": r })).unwrap_or_default();
        let response = self
            .intent
            .client
            .raw_json(
                Method::POST,
                &format!("/_matrix/client/unstable/knock/{encoded}"),
                Some(body),
            )
            .await?;
        response
            .get("room_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("missing room_id in knock response"))
    }

    pub async fn get_room_hierarchy(&self, room_id: &str) -> anyhow::Result<Value> {
        self.intent.ensure_registered().await?;
        let encoded = utf8_percent_encode(room_id, NON_ALPHANUMERIC).to_string();
        self.intent
            .client
            .raw_json(
                Method::GET,
                &format!("/_matrix/client/unstable/org.matrix.msc3266/rooms/{encoded}/hierarchy"),
                None,
            )
            .await
    }
}

pub const REMOTE_USER_INFO_ACCOUNT_DATA_EVENT_TYPE: &str = "io.t2bot.sdk.bot.remote_user_info";
pub const REMOTE_ROOM_INFO_ACCOUNT_DATA_EVENT_TYPE: &str = "io.t2bot.sdk.bot.remote_room_info";
pub const REMOTE_USER_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX: &str = "io.t2bot.sdk.bot.remote_user_map";
pub const REMOTE_ROOM_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX: &str = "io.t2bot.sdk.bot.remote_room_map";

#[derive(Clone)]
pub struct MatrixBridge {
    bot_intent: Intent,
    namespace_to_user: Arc<RwLock<HashMap<String, String>>>,
}

impl MatrixBridge {
    pub fn new(bot_intent: Intent) -> Self {
        Self {
            bot_intent,
            namespace_to_user: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn map_virtual_user(
        &self,
        namespace: impl Into<String>,
        user_id: impl Into<String>,
    ) -> anyhow::Result<()> {
        self.namespace_to_user
            .write()
            .await
            .insert(namespace.into(), user_id.into());
        Ok(())
    }

    pub async fn resolve_virtual_user(&self, namespace: &str) -> Option<String> {
        self.namespace_to_user.read().await.get(namespace).cloned()
    }

    /// Gets information about a remote user.
    pub async fn get_remote_user_info<T: serde::de::DeserializeOwned>(
        &self,
        user_intent: &Intent,
    ) -> anyhow::Result<T> {
        user_intent.ensure_registered().await?;
        let value = self
            .bot_intent
            .underlying_client()
            .get_account_data(REMOTE_USER_INFO_ACCOUNT_DATA_EVENT_TYPE)
            .await?;
        serde_json::from_value(value).context("Failed to parse remote user info")
    }

    /// Sets information about a remote user. Calling this function will map the
    /// provided remote user ID to the intent's owner.
    pub async fn set_remote_user_info<T: serde::Serialize>(
        &self,
        user_intent: &Intent,
        remote_info: &T,
    ) -> anyhow::Result<()> {
        user_intent.ensure_registered().await?;
        let value = serde_json::to_value(remote_info)?;
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("remote_info must have an 'id' field"))?;
        self.bot_intent
            .underlying_client()
            .set_account_data(REMOTE_USER_INFO_ACCOUNT_DATA_EVENT_TYPE, &value)
            .await?;
        self.update_remote_user_mapping(user_intent.user_id(), id)
            .await
    }

    /// Gets information about a remote room.
    pub async fn get_remote_room_info<T: serde::de::DeserializeOwned>(
        &self,
        matrix_room_id: &str,
    ) -> anyhow::Result<T> {
        self.bot_intent.ensure_registered().await?;
        let value = self
            .bot_intent
            .underlying_client()
            .get_room_account_data(matrix_room_id, REMOTE_ROOM_INFO_ACCOUNT_DATA_EVENT_TYPE)
            .await?;
        serde_json::from_value(value).context("Failed to parse remote room info")
    }

    /// Sets information about a remote room. Calling this function will map the
    /// provided remote room ID to the matrix room ID.
    pub async fn set_remote_room_info<T: serde::Serialize>(
        &self,
        matrix_room_id: &str,
        remote_info: &T,
    ) -> anyhow::Result<()> {
        self.bot_intent.ensure_registered().await?;
        let value = serde_json::to_value(remote_info)?;
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("remote_info must have an 'id' field"))?;
        self.bot_intent
            .underlying_client()
            .set_room_account_data(
                matrix_room_id,
                REMOTE_ROOM_INFO_ACCOUNT_DATA_EVENT_TYPE,
                &value,
            )
            .await?;
        self.update_remote_room_mapping(matrix_room_id, id).await
    }

    /// Gets the Matrix room ID for the provided remote room ID.
    pub async fn get_matrix_room_id_for_remote(
        &self,
        remote_room_id: &str,
    ) -> anyhow::Result<String> {
        self.bot_intent.ensure_registered().await?;
        let event_type =
            format!("{REMOTE_ROOM_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX}.{remote_room_id}");
        let value = self
            .bot_intent
            .underlying_client()
            .get_account_data(&event_type)
            .await?;
        value
            .get("id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' in remote room mapping"))
    }

    /// Gets a Matrix user intent for the provided remote user ID.
    pub async fn get_intent_for_remote(&self, remote_user_id: &str) -> anyhow::Result<Intent> {
        self.bot_intent.ensure_registered().await?;
        let event_type =
            format!("{REMOTE_USER_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX}.{remote_user_id}");
        let value = self
            .bot_intent
            .underlying_client()
            .get_account_data(&event_type)
            .await?;
        let user_id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'id' in remote user mapping"))?;
        // Return a new Intent with the same client as bot_intent
        Ok(Intent::new(
            user_id,
            self.bot_intent.underlying_client().clone(),
        ))
    }

    async fn update_remote_user_mapping(
        &self,
        matrix_user_id: &str,
        remote_user_id: &str,
    ) -> anyhow::Result<()> {
        let event_type =
            format!("{REMOTE_USER_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX}.{remote_user_id}");
        self.bot_intent
            .underlying_client()
            .set_account_data(&event_type, &json!({ "id": matrix_user_id }))
            .await
    }

    async fn update_remote_room_mapping(
        &self,
        matrix_room_id: &str,
        remote_room_id: &str,
    ) -> anyhow::Result<()> {
        let event_type =
            format!("{REMOTE_ROOM_MAP_ACCOUNT_DATA_EVENT_TYPE_PREFIX}.{remote_room_id}");
        self.bot_intent
            .underlying_client()
            .set_account_data(&event_type, &json!({ "id": matrix_room_id }))
            .await
    }
}

#[async_trait]
pub trait UnstableAppserviceApis: Send + Sync {
    async fn unstable_ping(&self) -> anyhow::Result<Value>;

    /// Send several historical events into a room (MSC2716).
    async fn send_historical_event_batch(
        &self,
        room_id: &str,
        prev_event_id: &str,
        events: Vec<Value>,
        state_events_at_start: Vec<Value>,
        chunk_id: Option<String>,
    ) -> anyhow::Result<crate::models::MSC2716BatchSendResponse>;

    /// Sends an event to the given room with a given timestamp.
    async fn send_event_with_timestamp(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
        ts: u64,
    ) -> anyhow::Result<String>;

    /// Sends a state event to the given room with a given timestamp.
    async fn send_state_event_with_timestamp(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: &Value,
        ts: u64,
    ) -> anyhow::Result<String>;
}

#[async_trait]
impl UnstableAppserviceApis for Appservice {
    async fn unstable_ping(&self) -> anyhow::Result<Value> {
        self.client
            .raw_json(Method::GET, "/_matrix/app/unstable/ping", None)
            .await
    }

    async fn send_historical_event_batch(
        &self,
        room_id: &str,
        prev_event_id: &str,
        events: Vec<Value>,
        state_events_at_start: Vec<Value>,
        chunk_id: Option<String>,
    ) -> anyhow::Result<crate::models::MSC2716BatchSendResponse> {
        let room_id = utf8_percent_encode(room_id, NON_ALPHANUMERIC).to_string();
        let body = json!({
            "prev_event": prev_event_id,
            "chunk_id": chunk_id,
            "events": events,
            "state_events_at_start": state_events_at_start,
        });
        let response = self
            .client
            .raw_json(
                Method::POST,
                &format!("/_matrix/client/unstable/org.matrix.msc2716/rooms/{room_id}/batch_send"),
                Some(body),
            )
            .await?;
        serde_json::from_value(response).context("Failed to parse batch send response")
    }

    async fn send_event_with_timestamp(
        &self,
        room_id: &str,
        event_type: &str,
        content: &Value,
        ts: u64,
    ) -> anyhow::Result<String> {
        let room_id = utf8_percent_encode(room_id, NON_ALPHANUMERIC).to_string();
        let event_type = utf8_percent_encode(event_type, NON_ALPHANUMERIC).to_string();
        let txn_id = format!(
            "{}__inc_appts{}",
            chrono::Utc::now().timestamp_millis(),
            Uuid::new_v4()
        );
        let txn_id = utf8_percent_encode(&txn_id, NON_ALPHANUMERIC).to_string();
        let response = self
            .client
            .raw_json(
                Method::PUT,
                &format!(
                    "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}?ts={}",
                    ts
                ),
                Some(content.clone()),
            )
            .await?;
        response
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("Missing event_id in response"))
    }

    async fn send_state_event_with_timestamp(
        &self,
        room_id: &str,
        event_type: &str,
        state_key: &str,
        content: &Value,
        ts: u64,
    ) -> anyhow::Result<String> {
        let room_id = utf8_percent_encode(room_id, NON_ALPHANUMERIC).to_string();
        let event_type = utf8_percent_encode(event_type, NON_ALPHANUMERIC).to_string();
        let state_key = utf8_percent_encode(state_key, NON_ALPHANUMERIC).to_string();
        let response = self
            .client
            .raw_json(
                Method::PUT,
                &format!(
                    "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}?ts={}",
                    ts
                ),
                Some(content.clone()),
            )
            .await?;
        response
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("Missing event_id in response"))
    }
}
