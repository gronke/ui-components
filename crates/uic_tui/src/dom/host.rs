//! LitElement semantics on the retained DOM (ADR 0008): registered custom
//! elements mount as element nodes, their templates instantiate through the
//! parts engine, holes resolve against the property store, and every update
//! cycle patches only the parts that changed.
//!
//! Data flows down the tree: attribute parts commit onto child custom-tag
//! nodes and the child mount syncs its observed attributes from its own
//! node, `.prop` writes apply directly to child stores — and `.value`/
//! `.options` writes onto widget-bearing elements sync the terminal widget
//! living in the node payload. Events flow up: a child's notify events
//! route into the parent's `@event` template bindings AND dispatch as
//! bubbling DOM events, and a widget commit routes into the `@change`
//! binding its template declares. Reflected properties land on the host
//! element as attributes, ReactiveElement's reflection during update.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use uic_core::{
    attribute_to_value, notify_events, Behavior, Changed, ComponentDef, Ctx, CustomElementRegistry,
    NotifyEvent, PropertyStore, Value,
};
use uic_dom::parts::{CompiledTemplate, EventBinding, PartValue, TemplateInstance};
use uic_dom::{Event as DomEvent, NodeId};

use super::resolve;
use super::widget;
use super::DomDocument;
use crate::Error;

thread_local! {
    /// One compiled template per component definition; the prototype is
    /// immutable and every instance clones from it.
    static TEMPLATES: RefCell<HashMap<&'static str, Rc<CompiledTemplate>>> =
        RefCell::new(HashMap::new());
}

fn compiled(def: &'static ComponentDef) -> Rc<CompiledTemplate> {
    TEMPLATES.with(|cache| {
        cache
            .borrow_mut()
            .entry(def.tag_name)
            .or_insert_with(|| Rc::new(CompiledTemplate::from_template(def.template())))
            .clone()
    })
}

pub(super) type Listener = Box<dyn FnMut(&NotifyEvent)>;

/// Delivers notify events to every subscribed listener whose key matches —
/// the one listener loop behind [`DomHost`] and [`super::App`].
pub(super) fn deliver<K>(
    listeners: &mut [(K, Listener)],
    events: &[NotifyEvent],
    matches: impl Fn(&K, &NotifyEvent) -> bool,
) {
    for event in events {
        for (key, listener) in listeners.iter_mut() {
            if matches(key, event) {
                listener(event);
            }
        }
    }
}

/// A document hosting one mounted component tree.
pub struct DomHost {
    doc: DomDocument,
    root: Mount,
    listeners: Vec<(String, Listener)>,
}

/// One mounted component: its state beside its element node, its template
/// instance, and the child mounts living on custom-tag nodes below it.
pub(crate) struct Mount {
    def: &'static ComponentDef,
    store: PropertyStore,
    behavior: Box<dyn Behavior>,
    pub(crate) host: NodeId,
    template: Rc<CompiledTemplate>,
    instance: TemplateInstance,
    /// `@event=${handler}` bindings, template-declared; grows when
    /// conditional branches instantiate.
    pub(super) bindings: Vec<EventBinding>,
    /// Child component mounts, keyed by their custom-tag node.
    pub(super) children: HashMap<NodeId, Mount>,
    /// The attribute set last synced from the host node, per child — the
    /// diff drives `attribute_changed` on the way down.
    pub(super) synced_attrs: HashMap<NodeId, HashMap<String, String>>,
    cascade: u8,
}

impl DomHost {
    /// Mounts a registered element into a fresh document — the
    /// `document.createElement` + append moment, `connected` included.
    pub fn mount(tag: &str) -> Result<DomHost, Error> {
        let mut doc = DomDocument::new();
        let root_node = doc.root();
        let mut root = Mount::create(&mut doc, root_node, tag)?;
        let events = root.update_cycle(&mut doc, |behavior, ctx| behavior.connected(ctx));
        let mut host = DomHost {
            doc,
            root,
            listeners: Vec::new(),
        };
        host.publish(events);
        Ok(host)
    }

    /// The document, for assertions and DOM-level listeners.
    pub fn doc(&self) -> &DomDocument {
        &self.doc
    }

    pub fn doc_mut(&mut self) -> &mut DomDocument {
        &mut self.doc
    }

    /// The mounted component's element node.
    pub fn root_node(&self) -> NodeId {
        self.root.host
    }

    /// The rendered tree as HTML.
    pub fn outer_html(&self) -> String {
        self.doc.outer_html(self.root.host)
    }

    /// First descendant element with the tag, for tests and probes.
    pub fn find(&self, tag: &str) -> Option<NodeId> {
        self.doc
            .descendants(self.root.host)
            .find(|&node| self.doc.tag_name(node).map(|t| &**t == tag) == Some(true))
    }

    /// Subscribes to the root component's notify events.
    pub fn on(&mut self, event: &str, listener: impl FnMut(&NotifyEvent) + 'static) {
        self.listeners.push((event.to_string(), Box::new(listener)));
    }

    /// Sets an observed attribute on the root component.
    pub fn set_attr(&mut self, name: &str, value: &str) {
        let events = self.root.set_attr(&mut self.doc, name, Some(value));
        self.publish(events);
    }

    /// Sets a property on the root component, `el.prop = …`.
    pub fn set_prop(&mut self, name: &str, value: impl Into<Value>) {
        let events = self.root.set_prop(&mut self.doc, name, value.into());
        self.publish(events);
    }

    /// Sets a property on the component mounted at `node` — the way a test
    /// stands in for a widget commit inside a child. Notify events route up
    /// through the template bindings and bubble through the document.
    pub fn set_prop_at(&mut self, node: NodeId, name: &str, value: impl Into<Value>) {
        let value = value.into();
        let events = self
            .root
            .with_mount_at(node, &mut self.doc, &mut |mount, doc| {
                mount.set_prop(doc, name, value.clone())
            });
        self.publish(events);
    }

    fn publish(&mut self, events: Vec<NotifyEvent>) {
        deliver(&mut self.listeners, &events, |name, event| {
            name == &event.event_name
        });
    }
}

impl Mount {
    pub(crate) fn create(doc: &mut DomDocument, parent: NodeId, tag: &str) -> Result<Mount, Error> {
        let def =
            CustomElementRegistry::get(tag).ok_or_else(|| Error::UnknownTag(tag.to_string()))?;
        let host = doc.create_element_named(tag);
        doc.append_child(parent, host);
        Mount::bind(doc, host, def)
    }

    pub(super) fn create_at(doc: &mut DomDocument, host: NodeId) -> Result<Mount, Error> {
        let tag = doc
            .tag_name(host)
            .map(|t| t.to_string())
            .ok_or_else(|| Error::UnknownTag(String::new()))?;
        let def = CustomElementRegistry::get(&tag).ok_or_else(|| Error::UnknownTag(tag.clone()))?;
        Mount::bind(doc, host, def)
    }

    fn bind(
        doc: &mut DomDocument,
        host: NodeId,
        def: &'static ComponentDef,
    ) -> Result<Mount, Error> {
        let template = compiled(def);
        let (instance, bindings) = template.instantiate(doc, host);
        let mut mount = Mount {
            def,
            store: PropertyStore::new(def.properties),
            behavior: (def.new_behavior)(),
            host,
            template,
            instance,
            bindings,
            children: HashMap::new(),
            synced_attrs: HashMap::new(),
            cascade: 0,
        };
        widget::mount_widgets(doc, mount.host);
        mount.adopt_new_children(doc);
        Ok(mount)
    }

    /// ReactiveElement's cycle on the DOM: the trigger collects the batch,
    /// `will_update` joins it, notify events emit (and bubble), reflected
    /// properties land on the host attributes, the commit patches the parts
    /// and syncs widgets and children, `updated` runs on the committed
    /// state and its writes drive a converging follow-up.
    pub(crate) fn update_cycle(
        &mut self,
        doc: &mut DomDocument,
        f: impl FnOnce(&mut dyn Behavior, &mut Ctx),
    ) -> Vec<NotifyEvent> {
        self.cascade += 1;
        debug_assert!(
            self.cascade < 16,
            "runaway update cascade in <{}>",
            self.def.tag_name
        );
        let mut changed = Changed::default();
        {
            let mut ctx = Ctx::new(&mut self.store, &mut changed);
            f(self.behavior.as_mut(), &mut ctx);
        }
        let mut events = Vec::new();
        let mut cycles: u8 = 0;
        loop {
            cycles += 1;
            debug_assert!(
                cycles < 16,
                "updated() keeps requesting follow-up cycles in <{}>",
                self.def.tag_name
            );
            let snapshot = changed.clone();
            {
                let mut ctx = Ctx::new(&mut self.store, &mut changed);
                self.behavior.will_update(&mut ctx, &snapshot);
            }
            let cycle_events = notify_events(self.def, &changed, &self.store);
            for event in &cycle_events {
                let mut dom_event = DomEvent::new(&event.event_name)
                    .with_bubbles(true)
                    .with_detail(event.value.clone());
                doc.dispatch_event(self.host, &mut dom_event);
            }
            events.extend(cycle_events);
            self.reflect(doc, &changed);
            events.extend(self.commit(doc));
            let mut follow_up = Changed::default();
            {
                let mut ctx = Ctx::new(&mut self.store, &mut follow_up);
                self.behavior.updated(&mut ctx, &changed);
            }
            if follow_up.is_empty() {
                break;
            }
            changed = follow_up;
        }
        self.cascade -= 1;
        events
    }

    /// ReactiveElement reflection: changed properties declared `reflect`
    /// land on the host element as attributes — booleans as presence.
    fn reflect(&mut self, doc: &mut DomDocument, changed: &Changed) {
        for (rust_name, _) in changed.iter() {
            let Some(meta) = self.def.property(rust_name) else {
                continue;
            };
            if !meta.reflect {
                continue;
            }
            let Some(attribute) = meta.attribute else {
                continue;
            };
            match self.store.get(rust_name) {
                Value::Bool(true) => doc.set_attribute(self.host, attribute, ""),
                Value::Bool(false) | Value::Null | Value::Undefined => {
                    doc.remove_attribute(self.host, attribute)
                }
                value => {
                    let text = value.display_text();
                    doc.set_attribute(self.host, attribute, &text);
                }
            }
        }
    }

    /// Resolves every hole and patches the instance, then carries the
    /// effects into the tree: newly rendered branches may mount widgets and
    /// child components, attribute commits sync down as
    /// `attribute_changed`, `.prop` writes apply to child stores, and
    /// `.value`/`.options` writes sync the widgets in the node payloads.
    ///
    /// Cost model: resolution is O(all holes) per commit — every hole
    /// re-evaluates (computed getters included) and the per-part `PartialEq`
    /// dedupe in the parts engine keeps the tree writes minimal. A
    /// changed-properties filter over the holes would cut the resolution
    /// work; it becomes worthwhile when repeated template instances multiply
    /// the hole count.
    fn commit(&mut self, doc: &mut DomDocument) -> Vec<NotifyEvent> {
        let template = self.template.clone();
        let mut values: Vec<PartValue> = template
            .holes()
            .iter()
            .map(|expr| resolve::resolve_hole(expr, &self.store, self.behavior.as_ref()))
            .collect();
        // Each repeat tree resolves per row against its scope stack, nesting
        // lists for repeats inside repeat bodies (ADR 0001).
        for repeat in template.repeats() {
            let each = match &values[repeat.each_hole] {
                PartValue::Value(value) => value.clone(),
                _ => uic_core::Value::Undefined,
            };
            values[repeat.each_hole] =
                resolve::resolve_repeat(&repeat, &each, &[], &self.store, self.behavior.as_ref());
        }
        let effects = template.commit(&mut self.instance, doc, &values);
        self.bindings.extend(effects.added_events);
        widget::mount_widgets(doc, self.host);
        self.adopt_new_children(doc);

        let mut routed = Vec::new();
        // Attribute diffs flow down first, then property writes.
        let child_nodes: Vec<NodeId> = self.children.keys().copied().collect();
        for node in child_nodes {
            let events = self.sync_child_attrs(doc, node);
            routed.extend(self.route_child_events(doc, node, events));
        }
        for write in effects.property_writes {
            let value = resolve::part_value_to_value(&write.value);
            if self.children.contains_key(&write.node) {
                let events = self
                    .children
                    .get_mut(&write.node)
                    .expect("checked above")
                    .set_prop(doc, &write.name, value);
                routed.extend(self.route_child_events(doc, write.node, events));
                continue;
            }
            // Writes onto plain widget-bearing elements feed the widget living
            // in the node payload.
            resolve::apply_widget_write(doc, write.node, &write.name, value);
        }
        routed
    }

    pub(crate) fn set_attr(
        &mut self,
        doc: &mut DomDocument,
        name: &str,
        value: Option<&str>,
    ) -> Vec<NotifyEvent> {
        let Some(meta) = self.def.property_by_attribute(name) else {
            return Vec::new();
        };
        let new_value = attribute_to_value(meta.js_type, value);
        let rust_name = meta.rust_name;
        let owned = value.map(str::to_string);
        self.update_cycle(doc, |behavior, ctx| {
            ctx.set(rust_name, new_value);
            behavior.attribute_changed(ctx, name, None, owned.as_deref());
        })
    }

    pub(crate) fn set_prop(
        &mut self,
        doc: &mut DomDocument,
        name: &str,
        value: Value,
    ) -> Vec<NotifyEvent> {
        let Some(meta) = self.def.property(name) else {
            return Vec::new();
        };
        let rust_name = meta.rust_name;
        self.update_cycle(doc, |_, ctx| {
            ctx.set(rust_name, value);
        })
    }
}
