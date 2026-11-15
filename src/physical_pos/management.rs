//! `client.physical_pos_management()`: stores, registers, logical
//! terminals, devices, device identity, the end-of-day policy, the catalog
//! and the overview.

use super::{
    request::{
        build_body, build_query, keyed_write, path_id, record_of, row_version, send, to_page,
        unwrap,
    },
    results::{PhysicalPosPage, PhysicalPosWriteResult},
};
use crate::{Client, Method, Response, Result};
use serde_json::Value;

/// Physical POS management — stores, registers, logical terminals with their
/// bank rows, payment-provider rows and device links, devices with their
/// identity keys, the end-of-day policy, the catalog and the overview
/// (`/api/v1/physical-pos`). Reads need `physical_pos:view`, writes
/// `physical_pos:manage`, device identity writes (key revocation, enrollment
/// revert, activation-code revocation) `physical_pos:identity`; a TEST key
/// can only read. Activation codes are issued in the panel only.
///
/// Updates and status changes take the `rowVersion` you last read (409
/// `physical_pos.version_conflict` when the record changed). Reason codes
/// are closed sets checked by the gateway (400
/// `physical_pos.input.reason_code_invalid`). Writes that take an
/// `Option<&str>` key get a new UUID for `None`; a key you pass is sent
/// exactly as given (never trimmed). The key sent comes back as
/// `idempotency_key`. A key belongs to one operation: reusing it for another
/// body or route is 409 `physical_pos.idempotency_mismatch`.
///
/// Requests and filters are JSON objects checked against
/// [`PHYSICAL_POS_REQUEST_FIELDS`](crate::PHYSICAL_POS_REQUEST_FIELDS) and
/// [`PHYSICAL_POS_FILTERS`](crate::PHYSICAL_POS_FILTERS) before sending
/// (`Value::Null` for no filters). Pages carry `total_count` only with
/// `includeTotal: true` on entity lists.
pub struct PhysicalPosManagement<'a> {
    client: &'a Client,
}
impl Client {
    /// Returns the physical POS management facade; no independent HTTP client is constructed.
    pub fn physical_pos_management(&self) -> PhysicalPosManagement<'_> {
        PhysicalPosManagement { client: self }
    }
}

fn store(store_id: &str) -> Result<String> {
    Ok(format!("/stores/{}", path_id(store_id)?))
}
fn register(register_id: &str) -> Result<String> {
    Ok(format!("/registers/{}", path_id(register_id)?))
}
fn terminal(terminal_id: &str) -> Result<String> {
    Ok(format!("/terminals/{}", path_id(terminal_id)?))
}
fn device(device_id: &str) -> Result<String> {
    Ok(format!("/devices/{}", path_id(device_id)?))
}
fn device_key(device_id: &str, key_id: &str) -> Result<String> {
    Ok(format!("{}/keys/{}", device(device_id)?, path_id(key_id)?))
}

impl PhysicalPosManagement<'_> {
    /// Store, register, terminal and device counts (`GET /overview`).
    pub async fn get_overview(&self) -> Result<Value> {
        Ok(record_of(
            &self.read("/overview", vec![]).await?,
            "overview",
        ))
    }

    // ── Catalog ─────────────────────────────────────────────────────────────

    /// Device vendors (`GET /catalog/vendors`).
    pub async fn list_vendors(&self) -> Result<Value> {
        Ok(unwrap(&self.read("/catalog/vendors", vec![]).await?))
    }

    /// Device models; filters `vendorCode`, `deviceType`, `status` (`GET /catalog/device-models`).
    pub async fn list_device_models(&self, filters: &Value) -> Result<Value> {
        let query = build_query(filters, "listDeviceModels")?;
        Ok(unwrap(&self.read("/catalog/device-models", query).await?))
    }

    /// Banks a terminal row can belong to (`GET /catalog/acquirers`).
    pub async fn list_acquirers(&self) -> Result<Value> {
        Ok(unwrap(&self.read("/catalog/acquirers", vec![]).await?))
    }

    // ── Stores ──────────────────────────────────────────────────────────────

    /// Stores (`GET /stores`).
    pub async fn list_stores(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/stores".into(), filters, "listStores").await
    }

    /// Creates a store: `data.store`, 201 new / 200 replay.
    pub async fn create_store(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let body = build_body(request, "createStore")?;
        keyed_write(self.client, "/stores", body, idempotency_key).await
    }

    /// `{store, registerSummary: {activeCount, archivedCount}, assignedDeviceCount}`.
    pub async fn get_store(&self, store_id: &str) -> Result<Value> {
        Ok(unwrap(&self.read(&store(store_id)?, vec![]).await?))
    }

    /// `null` clears `city`, `district`, `addressLine`, `externalRef`; a field
    /// left out is unchanged. Returns the store.
    pub async fn update_store(&self, store_id: &str, request: &Value) -> Result<Value> {
        let path = store(store_id)?;
        self.record(Method::PATCH, &path, request, "updateStore", "store")
            .await
    }

    /// `{store, archivedRegisterCount}`; the active registers are archived with it.
    pub async fn archive_store(&self, store_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/archive", store(store_id)?);
        self.post(&path, request, "statusChange").await
    }

    /// Restores an archived store. Returns the store.
    pub async fn restore_store(&self, store_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/restore", store(store_id)?);
        self.record(Method::POST, &path, request, "statusChange", "store")
            .await
    }

    /// The store's audit trail.
    pub async fn list_store_audit(
        &self,
        store_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/audit", store(store_id)?);
        self.page(path, filters, "history").await
    }

    // ── Registers ───────────────────────────────────────────────────────────

    /// Registers of a store.
    pub async fn list_registers(&self, store_id: &str, filters: &Value) -> Result<PhysicalPosPage> {
        let path = format!("{}/registers", store(store_id)?);
        self.page(path, filters, "listRegisters").await
    }

    /// Creates a register: `data.register`, 201 new / 200 replay.
    pub async fn create_register(
        &self,
        store_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/registers", store(store_id)?);
        let body = build_body(request, "createRegister")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// Renames a register. Returns the register.
    pub async fn update_register(&self, register_id: &str, request: &Value) -> Result<Value> {
        let path = register(register_id)?;
        self.record(Method::PATCH, &path, request, "updateRegister", "register")
            .await
    }

    /// Archives a register. Returns the register.
    pub async fn archive_register(&self, register_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/archive", register(register_id)?);
        self.record(Method::POST, &path, request, "statusChange", "register")
            .await
    }

    /// Restores an archived register. Returns the register.
    pub async fn restore_register(&self, register_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/restore", register(register_id)?);
        self.record(Method::POST, &path, request, "statusChange", "register")
            .await
    }

    /// The register's audit trail.
    pub async fn list_register_audit(
        &self,
        register_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/audit", register(register_id)?);
        self.page(path, filters, "history").await
    }

    // ── Logical terminals ───────────────────────────────────────────────────

    /// Logical terminals (`GET /terminals`).
    pub async fn list_terminals(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/terminals".into(), filters, "listTerminals")
            .await
    }

    /// Creates a terminal: `data.terminal`, 201 new / 200 replay.
    pub async fn create_terminal(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let body = build_body(request, "createTerminal")?;
        keyed_write(self.client, "/terminals", body, idempotency_key).await
    }

    /// `{terminal, acquirers: {items, pageInfo}}`.
    pub async fn get_terminal(&self, terminal_id: &str) -> Result<Value> {
        Ok(unwrap(&self.read(&terminal(terminal_id)?, vec![]).await?))
    }

    /// Changes a terminal. Returns the terminal.
    pub async fn update_terminal(&self, terminal_id: &str, request: &Value) -> Result<Value> {
        let path = terminal(terminal_id)?;
        self.record(Method::PATCH, &path, request, "updateTerminal", "terminal")
            .await
    }

    /// Closes a terminal. Returns the terminal.
    pub async fn close_terminal(&self, terminal_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/close", terminal(terminal_id)?);
        self.record(Method::POST, &path, request, "statusChange", "terminal")
            .await
    }

    /// Bank rows of a terminal; a row's `id` is the payment's `terminalAcquirerId`.
    pub async fn list_terminal_acquirers(
        &self,
        terminal_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/acquirers", terminal(terminal_id)?);
        self.page(path, filters, "listTerminalAcquirers").await
    }

    /// Adds a bank row: `data.acquirer`, 201 new / 200 replay.
    pub async fn add_terminal_acquirer(
        &self,
        terminal_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/acquirers", terminal(terminal_id)?);
        let body = build_body(request, "addTerminalAcquirer")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// Changes a bank row (`null` label clears it). Returns the row.
    pub async fn update_terminal_acquirer(
        &self,
        terminal_acquirer_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("/terminal-acquirers/{}", path_id(terminal_acquirer_id)?);
        self.record(
            Method::PATCH,
            &path,
            request,
            "updateTerminalAcquirer",
            "acquirer",
        )
        .await
    }

    /// Payment-provider rows of a terminal (e.g. a meal card provider's
    /// merchant / terminal / product code). A row's `id` is the payment's
    /// `terminalProviderId`.
    pub async fn list_terminal_providers(
        &self,
        terminal_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/providers", terminal(terminal_id)?);
        self.page(path, filters, "listTerminalProviders").await
    }

    /// Adds a provider row `{vendorAccountId, merchantCode, terminalCode,
    /// productCode?, label?}`; the provider comes from the vendor account.
    /// `data.provider`, 201 new / 200 replay.
    pub async fn add_terminal_provider(
        &self,
        terminal_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/providers", terminal(terminal_id)?);
        let body = build_body(request, "addTerminalProvider")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// `{rowVersion, label?, status?}`; `null` label clears it, `status`
    /// ACTIVE or DISABLED. Returns the row.
    pub async fn update_terminal_provider(
        &self,
        terminal_provider_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("/terminal-providers/{}", path_id(terminal_provider_id)?);
        self.record(
            Method::PATCH,
            &path,
            request,
            "updateTerminalProvider",
            "provider",
        )
        .await
    }

    /// Links the terminal to a device (closing the previous link): `{terminal, link}`.
    pub async fn change_terminal_device(
        &self,
        terminal_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("{}/device-link", terminal(terminal_id)?);
        self.post(&path, request, "changeTerminalDevice").await
    }

    /// Closes the terminal's device link: `data.terminal`, `data.link` (the
    /// closed link or null).
    pub async fn close_terminal_device_link(
        &self,
        terminal_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/device-link/close", terminal(terminal_id)?);
        let body = build_body(request, "statusChange")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// The terminal's device links.
    pub async fn list_terminal_links(
        &self,
        terminal_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/links", terminal(terminal_id)?);
        self.page(path, filters, "history").await
    }

    /// The terminal's audit trail.
    pub async fn list_terminal_audit(
        &self,
        terminal_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/audit", terminal(terminal_id)?);
        self.page(path, filters, "history").await
    }

    // ── Devices ─────────────────────────────────────────────────────────────

    /// Devices (`GET /devices`).
    pub async fn list_devices(&self, filters: &Value) -> Result<PhysicalPosPage> {
        self.page("/devices".into(), filters, "listDevices").await
    }

    /// Registers a device: `data.device`, 201 new / 200 replay.
    pub async fn create_device(
        &self,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let body = build_body(request, "createDevice")?;
        keyed_write(self.client, "/devices", body, idempotency_key).await
    }

    /// One device.
    pub async fn get_device(&self, device_id: &str) -> Result<Value> {
        Ok(record_of(
            &self.read(&device(device_id)?, vec![]).await?,
            "device",
        ))
    }

    /// Changes a device. Returns the device.
    pub async fn update_device(&self, device_id: &str, request: &Value) -> Result<Value> {
        let path = device(device_id)?;
        self.record(Method::PATCH, &path, request, "updateDevice", "device")
            .await
    }

    /// Assigns the device to a store (and register): `{device, assignment, previousAssignmentId}`.
    pub async fn assign_device(&self, device_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/assignment", device(device_id)?);
        self.post(&path, request, "assignDevice").await
    }

    /// Moves the device to storage: `data.device`, `data.closedAssignment`.
    pub async fn close_device_assignment(
        &self,
        device_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/assignment/close", device(device_id)?);
        let body = build_body(request, "statusChange")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// `target`: ACTIVE, SUSPENDED or MAINTENANCE. Returns the device.
    pub async fn change_device_lifecycle(&self, device_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/lifecycle", device(device_id)?);
        self.record(
            Method::POST,
            &path,
            request,
            "changeDeviceLifecycle",
            "device",
        )
        .await
    }

    /// Retires the device (final): `data.device`.
    pub async fn retire_device(
        &self,
        device_id: &str,
        request: &Value,
        idempotency_key: Option<&str>,
    ) -> Result<PhysicalPosWriteResult> {
        let path = format!("{}/retire", device(device_id)?);
        let body = build_body(request, "statusChange")?;
        keyed_write(self.client, &path, body, idempotency_key).await
    }

    /// The device's store assignments.
    pub async fn list_device_assignments(
        &self,
        device_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/assignments", device(device_id)?);
        self.page(path, filters, "history").await
    }

    /// The device's audit trail.
    pub async fn list_device_audit(
        &self,
        device_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/audit", device(device_id)?);
        self.page(path, filters, "history").await
    }

    // ── Device identity (`physical_pos:identity` for writes) ───────────────

    /// The device's identity keys, fingerprints only
    /// (`GET /devices/{deviceId}/keys`): `keyId`, `algorithm`, `keyStatus`
    /// (ACTIVE | SUPERSEDED | REVOKED), `enrolledVia` (PAIRING |
    /// ACTIVATION_CODE | ROTATION), `fingerprint`, `rowVersion` …
    pub async fn list_device_keys(
        &self,
        device_id: &str,
        filters: &Value,
    ) -> Result<PhysicalPosPage> {
        let path = format!("{}/keys", device(device_id)?);
        self.page(path, filters, "history").await
    }

    /// Revokes a device key: `{rowVersion (the key's), reasonCode}`;
    /// reasonCode DEVICE_LOST, DEVICE_STOLEN, KEY_COMPROMISED,
    /// DEVICE_REPLACED, MERCHANT_REQUEST or OTHER. `{key, lifecycleStatus, replayed}`.
    pub async fn revoke_device_key(
        &self,
        device_id: &str,
        key_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("{}/revoke", device_key(device_id, key_id)?);
        self.post(&path, request, "revokeDeviceKey").await
    }

    /// Reverts an activation-code enrollment (revokes the key, suspends the
    /// device): `{reasonCode}`; NOT_MY_DEVICE, SUSPECTED_PHISHING,
    /// WRONG_DEVICE or OTHER. `{key, lifecycleStatus, replayed}`.
    pub async fn revert_device_enrollment(
        &self,
        device_id: &str,
        key_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("{}/revert-enrollment", device_key(device_id, key_id)?);
        self.post(&path, request, "revertDeviceEnrollment").await
    }

    /// Revokes the device's open activation code: `{reasonCode}`;
    /// MERCHANT_REVOKED, DEVICE_LOST, DEVICE_STOLEN or OTHER.
    /// `{deviceId, revokedAt, revokeReason}`.
    pub async fn revoke_activation_code(&self, device_id: &str, request: &Value) -> Result<Value> {
        let path = format!("{}/activation-code/revoke", device(device_id)?);
        self.post(&path, request, "revokeActivationCode").await
    }

    // ── End-of-day policy (account → store → device) ────────────────────────
    //
    // A level's policy replaces the level above it entirely. `set_…` sends
    // the full setting: `{mode (MANUAL | SCHEDULED | DEVICE_AUTO),
    // scheduleTime?, timeZone?, scheduleDays?, retryWindowMinutes?,
    // retryIntervalMinutes?, offlineBehavior?, overdueAlertHours?,
    // expectedRowVersion?}`. `clear_…` resets the account to MANUAL, or
    // removes a store / device override (it then inherits).
    // `expectedRowVersion` is the policy's `rowVersion` you read (409
    // `physical_pos.version_conflict` when it changed); leave it out for last
    // write wins. Each call returns the `policy`.

    /// The account's end-of-day policy.
    pub async fn get_account_settlement_policy(&self) -> Result<Value> {
        self.policy(Method::GET, "/settlement-policy".into(), None, vec![])
            .await
    }

    /// Sets the account's end-of-day policy.
    pub async fn set_account_settlement_policy(&self, request: &Value) -> Result<Value> {
        let body = build_body(request, "setSettlementPolicy")?;
        self.policy(Method::PUT, "/settlement-policy".into(), Some(body), vec![])
            .await
    }

    /// Resets the account's policy to MANUAL.
    pub async fn clear_account_settlement_policy(
        &self,
        expected_row_version: Option<i64>,
    ) -> Result<Value> {
        let query = expected_row_version_query(expected_row_version)?;
        self.policy(Method::DELETE, "/settlement-policy".into(), None, query)
            .await
    }

    /// A store's end-of-day policy override.
    pub async fn get_store_settlement_policy(&self, store_id: &str) -> Result<Value> {
        let path = format!("{}/settlement-policy", store(store_id)?);
        self.policy(Method::GET, path, None, vec![]).await
    }

    /// Sets a store's end-of-day policy override.
    pub async fn set_store_settlement_policy(
        &self,
        store_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("{}/settlement-policy", store(store_id)?);
        let body = build_body(request, "setSettlementPolicy")?;
        self.policy(Method::PUT, path, Some(body), vec![]).await
    }

    /// Removes a store's override (it then inherits the account's policy).
    pub async fn clear_store_settlement_policy(
        &self,
        store_id: &str,
        expected_row_version: Option<i64>,
    ) -> Result<Value> {
        let path = format!("{}/settlement-policy", store(store_id)?);
        let query = expected_row_version_query(expected_row_version)?;
        self.policy(Method::DELETE, path, None, query).await
    }

    /// A device's end-of-day policy override.
    pub async fn get_device_settlement_policy(&self, device_id: &str) -> Result<Value> {
        let path = format!("{}/settlement-policy", device(device_id)?);
        self.policy(Method::GET, path, None, vec![]).await
    }

    /// Sets a device's override. DEVICE_AUTO only on devices that report
    /// their end-of-day results.
    pub async fn set_device_settlement_policy(
        &self,
        device_id: &str,
        request: &Value,
    ) -> Result<Value> {
        let path = format!("{}/settlement-policy", device(device_id)?);
        let body = build_body(request, "setSettlementPolicy")?;
        self.policy(Method::PUT, path, Some(body), vec![]).await
    }

    /// Removes a device's override (it then inherits).
    pub async fn clear_device_settlement_policy(
        &self,
        device_id: &str,
        expected_row_version: Option<i64>,
    ) -> Result<Value> {
        let path = format!("{}/settlement-policy", device(device_id)?);
        let query = expected_row_version_query(expected_row_version)?;
        self.policy(Method::DELETE, path, None, query).await
    }

    /// The policy a device follows
    /// (`GET /devices/{deviceId}/settlement-policy/effective`): `{deviceId,
    /// sourceScopeType (ACCOUNT | STORE | DEVICE), policy,
    /// serverSettlementSupported, deviceAutoSupported, nextScheduledAt,
    /// lastSettlementAt, overdue, scheduleWarning}`.
    pub async fn get_effective_settlement_policy(&self, device_id: &str) -> Result<Value> {
        let path = format!("{}/settlement-policy/effective", device(device_id)?);
        Ok(record_of(&self.read(&path, vec![]).await?, "effective"))
    }

    // ── Transport ───────────────────────────────────────────────────────────

    async fn read(&self, path: &str, query: Vec<(String, String)>) -> Result<Response> {
        send(self.client, Method::GET, path, None, query, None).await
    }

    async fn page(&self, path: String, filters: &Value, route: &str) -> Result<PhysicalPosPage> {
        let query = build_query(filters, route)?;
        Ok(to_page(&self.read(&path, query).await?))
    }

    async fn post(&self, path: &str, request: &Value, operation: &str) -> Result<Value> {
        let body = build_body(request, operation)?;
        let response = send(self.client, Method::POST, path, Some(body), vec![], None).await?;
        Ok(unwrap(&response))
    }

    async fn record(
        &self,
        method: Method,
        path: &str,
        request: &Value,
        operation: &str,
        name: &str,
    ) -> Result<Value> {
        let body = build_body(request, operation)?;
        let response = send(self.client, method, path, Some(body), vec![], None).await?;
        Ok(record_of(&response, name))
    }

    async fn policy(
        &self,
        method: Method,
        path: String,
        body: Option<Value>,
        query: Vec<(String, String)>,
    ) -> Result<Value> {
        let response = send(self.client, method, &path, body, query, None).await?;
        Ok(record_of(&response, "policy"))
    }
}

/// `DELETE …/settlement-policy?expectedRowVersion=` — left out for `None`.
fn expected_row_version_query(expected_row_version: Option<i64>) -> Result<Vec<(String, String)>> {
    match expected_row_version {
        None => Ok(vec![]),
        Some(version) => Ok(vec![(
            "expectedRowVersion".into(),
            row_version(version)?.to_string(),
        )]),
    }
}
