// nu_plugin_cedar -- nushell plugin exposing Cedar policy authorization.
//
// One command: `cedar authorize`. Input is a record with five string fields:
//   principal, action, resource     -- Cedar UID strings, e.g. `User::"alice"`
//   policies                        -- Cedar policy text (one or many `permit` / `forbid`)
//   entities                        -- Cedar entities JSON (string; same shape as
//                                       cedar-wasm and the Workers spike)
//   context                         -- optional JSON string, defaults to `{}`
//
// Output is a record:
//   {decision: "allow"|"deny", reasons: [String], errors: [String]}
//
// Why string fields rather than nu records for entities/context:
//   keeps the plugin tiny -- the middleware module (src/stdlib/cedar/mod.nu) does
//   `to json -r` on nu records before calling. Easy to swap to a typed conversion
//   later without breaking the wire.
//
// PolicySet parsing is cached per policy-text hash so repeat calls don't re-parse.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::str::FromStr;
use std::sync::Mutex;

use nu_plugin::{
    serve_plugin, EngineInterface, EvaluatedCall, MsgPackSerializer, Plugin, PluginCommand,
    SimplePluginCommand,
};
use nu_protocol::{LabeledError, Record, Signature, Span, Type, Value};

use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicySet, Request, Schema,
};

struct CedarPlugin {
    policy_cache: Mutex<HashMap<u64, PolicySet>>,
    schema_cache: Mutex<HashMap<u64, Schema>>,
}

impl Plugin for CedarPlugin {
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }

    fn commands(&self) -> Vec<Box<dyn PluginCommand<Plugin = Self>>> {
        vec![Box::new(AuthorizeCommand)]
    }
}

struct AuthorizeCommand;

impl SimplePluginCommand for AuthorizeCommand {
    type Plugin = CedarPlugin;

    fn name(&self) -> &str {
        "cedar authorize"
    }

    fn signature(&self) -> Signature {
        Signature::build("cedar authorize").input_output_type(Type::Any, Type::Any)
    }

    fn description(&self) -> &str {
        "Evaluate a Cedar authorization request. Input record fields: principal, action, resource, policies (Cedar text), entities (JSON string), context (JSON string, optional)."
    }

    fn run(
        &self,
        plugin: &CedarPlugin,
        _engine: &EngineInterface,
        call: &EvaluatedCall,
        input: &Value,
    ) -> Result<Value, LabeledError> {
        let span = call.head;

        let record = input
            .as_record()
            .map_err(|e| label_err(format!("expected record input: {e}"), span))?;

        let principal_str = require_str(record, "principal", span)?;
        let action_str = require_str(record, "action", span)?;
        let resource_str = require_str(record, "resource", span)?;
        let policies_str = require_str(record, "policies", span)?;
        let entities_str = optional_str(record, "entities").unwrap_or_else(|| "[]".to_owned());
        let context_str = optional_str(record, "context").unwrap_or_else(|| "{}".to_owned());
        let schema_str = optional_str(record, "schema");

        let principal = EntityUid::from_str(&principal_str)
            .map_err(|e| label_err(format!("invalid principal `{principal_str}`: {e}"), span))?;
        let action = EntityUid::from_str(&action_str)
            .map_err(|e| label_err(format!("invalid action `{action_str}`: {e}"), span))?;
        let resource = EntityUid::from_str(&resource_str)
            .map_err(|e| label_err(format!("invalid resource `{resource_str}`: {e}"), span))?;

        // Optional schema: when present, used for entity validation + request validation.
        let schema = match schema_str.as_deref() {
            Some(s) => Some(get_or_parse_schema(plugin, s, span)?),
            None => None,
        };

        let policies = get_or_parse_policies(plugin, &policies_str, span)?;

        let entities = Entities::from_json_str(&entities_str, schema.as_ref())
            .map_err(|e| label_err(format!("entities parse error: {e}"), span))?;

        let context_value: serde_json::Value = serde_json::from_str(&context_str)
            .map_err(|e| label_err(format!("context JSON parse error: {e}"), span))?;
        let context = Context::from_json_value(context_value, None)
            .map_err(|e| label_err(format!("context build error: {e}"), span))?;

        let request = Request::new(principal, action, resource, context, schema.as_ref())
            .map_err(|e| label_err(format!("request build error: {e}"), span))?;

        let response = Authorizer::new().is_authorized(&request, &policies, &entities);

        let decision = match response.decision() {
            Decision::Allow => "allow",
            Decision::Deny => "deny",
        };

        let reasons: Vec<Value> = response
            .diagnostics()
            .reason()
            .map(|p| Value::string(p.to_string(), span))
            .collect();

        let errors: Vec<Value> = response
            .diagnostics()
            .errors()
            .map(|e| Value::string(e.to_string(), span))
            .collect();

        let mut out = Record::new();
        out.push("decision", Value::string(decision, span));
        out.push("reasons", Value::list(reasons, span));
        out.push("errors", Value::list(errors, span));
        Ok(Value::record(out, span))
    }
}

fn get_or_parse_policies(
    plugin: &CedarPlugin,
    policies_str: &str,
    span: Span,
) -> Result<PolicySet, LabeledError> {
    let mut hasher = DefaultHasher::new();
    policies_str.hash(&mut hasher);
    let key = hasher.finish();

    let mut cache = plugin.policy_cache.lock().unwrap();
    if let Some(p) = cache.get(&key) {
        return Ok(p.clone());
    }
    let parsed = PolicySet::from_str(policies_str)
        .map_err(|e| label_err(format!("policy parse error: {e}"), span))?;
    cache.insert(key, parsed.clone());
    Ok(parsed)
}

fn get_or_parse_schema(
    plugin: &CedarPlugin,
    schema_str: &str,
    span: Span,
) -> Result<Schema, LabeledError> {
    let mut hasher = DefaultHasher::new();
    schema_str.hash(&mut hasher);
    let key = hasher.finish();

    let mut cache = plugin.schema_cache.lock().unwrap();
    if let Some(s) = cache.get(&key) {
        return Ok(s.clone());
    }
    let parsed = Schema::from_cedarschema_str(schema_str)
        .map(|(s, _warnings)| s)
        .map_err(|e| label_err(format!("schema parse error: {e}"), span))?;
    cache.insert(key, parsed.clone());
    Ok(parsed)
}

fn require_str(record: &Record, key: &str, span: Span) -> Result<String, LabeledError> {
    record
        .get(key)
        .and_then(|v| v.as_str().ok())
        .map(|s| s.to_owned())
        .ok_or_else(|| label_err(format!("missing required string field: {key}"), span))
}

fn optional_str(record: &Record, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(|v| v.as_str().ok())
        .map(|s| s.to_owned())
}

fn label_err(msg: String, _span: Span) -> LabeledError {
    LabeledError::new(msg)
}

fn main() {
    let plugin = CedarPlugin {
        policy_cache: Mutex::new(HashMap::new()),
        schema_cache: Mutex::new(HashMap::new()),
    };
    serve_plugin(&plugin, MsgPackSerializer)
}
