//! Async data connectors (ADR 0014): the query side of `input-suggestion`.
//!
//! A [`QuerySource`] answers a typed text with matching option rows through a
//! delivery callback — the push shape both targets share. The browser twin
//! (`connect.impl.ts`, emitted as `components/uic-connectors.ts`) spells the
//! same interface as `query(text): Promise<SelectOption[]>`; the
//! `InMemorySource` matching rules are pinned across targets by the parity
//! fixtures.
//!
//! Sources deliver on their own schedule: an in-memory source immediately —
//! inside the TUI that is within the current update cycle, so the popup
//! repaints in the same frame — while a remote browser source resolves later
//! and lands through a property write. The terminal runtime has no executor,
//! so a Rust source that cannot answer synchronously needs a host-driven
//! pump (the ADR 0013 deliver-return-apply pattern).

use uic_core::SelectOption;

/// The browser twin, emitted by the web codegen as
/// `components/uic-connectors.ts` — keep it in sync with this module.
pub const WEB_TS: &str = include_str!("connect.impl.ts");

/// Receives the rows a [`QuerySource`] resolved for one query.
///
/// The borrow allows capturing the caller's context for in-cycle delivery;
/// a source that outlives the call must be driven by the host instead.
pub type Deliver<'a> = Box<dyn FnOnce(Vec<SelectOption>) + 'a>;

/// An async suggestion source: resolves matching rows for the typed text.
pub trait QuerySource {
    fn query(&self, text: &str, deliver: Deliver<'_>);
}

/// The pool: a fixed option list matched case-insensitively by value prefix,
/// in pool order, capped at `limit` rows. The empty query yields no rows.
pub struct InMemorySource {
    options: Vec<SelectOption>,
    limit: usize,
}

impl InMemorySource {
    pub fn new(options: Vec<SelectOption>) -> Self {
        InMemorySource { options, limit: 8 }
    }

    /// A pool of plain words, each becoming a label-free option.
    pub fn from_words<I>(words: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        InMemorySource::new(words.into_iter().map(SelectOption::new).collect())
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl QuerySource for InMemorySource {
    fn query(&self, text: &str, deliver: Deliver<'_>) {
        let needle = text.to_lowercase();
        if needle.is_empty() {
            return deliver(Vec::new());
        }
        deliver(
            self.options
                .iter()
                .filter(|option| option.value.to_lowercase().starts_with(&needle))
                .take(self.limit)
                .cloned()
                .collect(),
        );
    }
}

/// The simplest source: a provided method answers each query.
pub struct MethodSource<F: Fn(&str) -> Vec<SelectOption>> {
    method: F,
}

impl<F: Fn(&str) -> Vec<SelectOption>> MethodSource<F> {
    pub fn new(method: F) -> Self {
        MethodSource { method }
    }
}

impl<F: Fn(&str) -> Vec<SelectOption>> QuerySource for MethodSource<F> {
    fn query(&self, text: &str, deliver: Deliver<'_>) {
        deliver((self.method)(text));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn rows(source: &impl QuerySource, text: &str) -> Vec<String> {
        let mut rows = Vec::new();
        source.query(text, Box::new(|options| rows = options));
        rows.into_iter().map(|option| option.value).collect()
    }

    fn pool() -> InMemorySource {
        InMemorySource::from_words(["apple", "apricot", "Avocado", "banana"])
    }

    #[test]
    fn prefix_matches_case_insensitively_in_pool_order() {
        assert_eq!(rows(&pool(), "ap"), ["apple", "apricot"]);
        assert_eq!(rows(&pool(), "AP"), ["apple", "apricot"]);
        assert_eq!(rows(&pool(), "avo"), ["Avocado"]);
        assert_eq!(rows(&pool(), "apple"), ["apple"]);
        assert_eq!(rows(&pool(), "zzz"), Vec::<String>::new());
    }

    #[test]
    fn the_empty_query_yields_no_rows() {
        assert_eq!(rows(&pool(), ""), Vec::<String>::new());
    }

    #[test]
    fn the_limit_caps_the_rows() {
        let source = InMemorySource::from_words(["aa", "ab", "ac"]).with_limit(2);
        assert_eq!(rows(&source, "a"), ["aa", "ab"]);
    }

    #[test]
    fn a_method_source_passes_its_answer_through() {
        let source = MethodSource::new(|text: &str| vec![SelectOption::new(text.to_uppercase())]);
        assert_eq!(rows(&source, "hi"), ["HI"]);
    }

    #[test]
    fn deliver_runs_exactly_once_per_query() {
        let calls = Cell::new(0);
        pool().query("ap", Box::new(|_| calls.set(calls.get() + 1)));
        assert_eq!(calls.get(), 1);
        pool().query("", Box::new(|_| calls.set(calls.get() + 1)));
        assert_eq!(calls.get(), 2);
    }
}
