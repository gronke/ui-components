//! Custom Elements Manifest (v1) emission — the interchange/validation
//! artifact describing every generated component.

use serde_json::{json, Value as Json};
use uic_core::{ComponentDef, DefaultValue, JsType, PropertyMeta};

pub fn custom_elements_json(defs: &[&'static ComponentDef]) -> String {
    let modules: Vec<Json> = defs.iter().map(|def| module_json(def)).collect();
    let manifest = json!({
        "schemaVersion": "1.0.0",
        "modules": modules,
    });
    let mut out = serde_json::to_string_pretty(&manifest).expect("manifest serializes");
    out.push('\n');
    out
}

fn module_json(def: &'static ComponentDef) -> Json {
    let path = format!("components/{}.ts", def.tag_name);
    let members: Vec<Json> = def.properties.iter().map(member_json).collect();
    let attributes: Vec<Json> = def
        .properties
        .iter()
        .filter_map(|p| {
            p.attribute
                .map(|attribute| json!({ "name": attribute, "fieldName": p.js_name }))
        })
        .collect();
    let events: Vec<Json> = def
        .properties
        .iter()
        .filter_map(|p| {
            p.notify_event_name().map(|name| {
                json!({
                    "name": name.as_ref(),
                    "type": { "text": "CustomEvent" },
                    "description":
                        format!("Fired when `{}` changes; detail: {{ property, value, oldValue }}.", p.js_name),
                })
            })
        })
        .collect();

    json!({
        "kind": "javascript-module",
        "path": path,
        "declarations": [{
            "kind": "class",
            "name": def.class_name,
            "customElement": true,
            "tagName": def.tag_name,
            "superclass": { "name": "LitElement", "package": "lit" },
            "members": members,
            "attributes": attributes,
            "events": events,
        }],
        "exports": [
            {
                "kind": "js",
                "name": def.class_name,
                "declaration": { "name": def.class_name, "module": path },
            },
            {
                "kind": "custom-element-definition",
                "name": def.tag_name,
                "declaration": { "name": def.class_name, "module": path },
            },
        ],
    })
}

fn member_json(prop: &PropertyMeta) -> Json {
    let ts_type = match prop.js_type {
        JsType::String => "string",
        JsType::Number => "number",
        JsType::Boolean => "boolean",
        JsType::Zoned => "Temporal.ZonedDateTime | null",
        JsType::Options => "SelectOption[]",
        JsType::Object => "Record<string, unknown>",
    };
    let mut member = json!({
        "kind": "field",
        "name": prop.js_name,
        "type": { "text": ts_type },
    });
    let default = match prop.default {
        DefaultValue::Undefined => None,
        DefaultValue::Str(s) => Some(format!("'{s}'")),
        DefaultValue::Num(n) => Some(n.to_string()),
        DefaultValue::Bool(b) => Some(b.to_string()),
        DefaultValue::EmptyOptions => Some("[]".to_string()),
        DefaultValue::EmptyObject => Some("{}".to_string()),
    };
    if let Some(default) = default {
        member["default"] = json!(default);
    }
    if !prop.doc.is_empty() {
        member["description"] = json!(prop.doc);
    }
    member
}
