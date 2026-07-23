//! Canary for the Boa 0.21 engine bug the runtime works around: a closure
//! created inside a class constructor capturing a local lexical binding
//! panics the VM (`PutLexicalValue`, empty bindings table).
//! When this test starts failing, Boa fixed the bug — drop the module-level
//! accessor installation in js/src/runtime.ts and this test together.

use boa_engine::{Context, Source};

const REPRO: &str = r#"
class A {
    constructor() {
        const fns = [];
        for (const k of ['a']) {
            fns.push(() => k);
        }
        this.x = fns[0]();
    }
}
new A().x
"#;

// Same family, second shape: an arrow nested inside another arrow loses its
// captured environment (`this` included) once the enclosing call returns —
// the deferred call throws. Template `@event` values must therefore be
// method references (the EventPart host-binding supplies `this`, compiled
// lit's own idiom), never inline closures over render locals.
const NESTED_ARROW: &str = r#"
class A {
    constructor() { this.v = 42; }
    m() { return this.v; }
    r() { return [7].map((x, i) => (() => this.m() + i)); }
}
new A().r()[0]()
"#;

#[test]
fn nested_arrow_capture_still_breaks_boa() {
    let mut context = Context::default();
    let result = context.eval(Source::from_bytes(NESTED_ARROW));
    assert!(
        result.is_err(),
        "Boa now keeps nested-arrow captures alive after the enclosing call \
         returns — inline closures work as template event values again; \
         relax the method-reference guidance and drop this canary"
    );
}

#[test]
fn ctor_loop_capture_still_panics_boa() {
    let result = std::panic::catch_unwind(|| {
        let mut context = Context::default();
        let _ = context.eval(Source::from_bytes(REPRO));
    });
    assert!(
        result.is_err(),
        "Boa no longer panics on constructor loop-capture — remove the \
         installAccessors workaround in js/src/runtime.ts and this canary"
    );
}
