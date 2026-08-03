//! The custom-element registry: the `customElements` analog, backed by
//! `inventory` so component crates register their definitions at link time.

use crate::meta::ComponentDef;

/// One registered custom element; `#[derive(CustomElement)]` submits this via
/// `inventory::submit!`, the `customElements.define` moment.
pub struct Registration(pub fn() -> &'static ComponentDef);

inventory::collect!(Registration);

/// Read access to all registered custom elements, mirroring `customElements`.
pub struct CustomElementRegistry;

impl CustomElementRegistry {
    /// `customElements.get`: looks a definition up by tag name.
    pub fn get(tag: &str) -> Option<&'static ComponentDef> {
        Self::iter().find(|def| def.tag_name == tag)
    }

    pub fn iter() -> impl Iterator<Item = &'static ComponentDef> {
        inventory::iter::<Registration>.into_iter().map(|r| (r.0)())
    }

    /// Cross-checks the whole registry: at least one component is linked, tags
    /// are unique, and every custom-element tag used in a template resolves.
    pub fn assert_valid() -> Result<(), RegistryError> {
        let defs: Vec<_> = Self::iter().collect();
        if defs.is_empty() {
            return Err(RegistryError::Empty);
        }
        for (i, def) in defs.iter().enumerate() {
            if let Some(other) = defs[..i].iter().find(|d| d.tag_name == def.tag_name) {
                return Err(RegistryError::DuplicateTag {
                    tag: def.tag_name,
                    first: other.module_path,
                    second: def.module_path,
                });
            }
        }
        for def in &defs {
            for tag in def.template().custom_tags() {
                if tag != def.tag_name && Self::get(tag).is_none() {
                    return Err(RegistryError::UnknownTag {
                        tag: tag.to_string(),
                        referenced_by: def.tag_name,
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(
        "no custom elements are registered; call the component crate's `link()` \
         (e.g. `ui_components::link()`) so the linker keeps its registrations"
    )]
    Empty,
    #[error("duplicate custom-element tag <{tag}>: defined in {first} and {second}")]
    DuplicateTag {
        tag: &'static str,
        first: &'static str,
        second: &'static str,
    },
    #[error("unknown custom element <{tag}> referenced by the template of <{referenced_by}>")]
    UnknownTag {
        tag: String,
        referenced_by: &'static str,
    },
}
