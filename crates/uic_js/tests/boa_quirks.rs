//! Canary for the Boa 0.21 engine bug the bootstrap works around: a closure
//! created inside a class constructor capturing a local lexical binding
//! panics the VM (`PutLexicalValue`, empty bindings table).
//! When this test starts failing, Boa fixed the bug — drop the
//! `installAccessors` hoisting in js/bootstrap.js and this test together.

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

#[test]
fn ctor_loop_capture_still_panics_boa() {
    let result = std::panic::catch_unwind(|| {
        let mut context = Context::default();
        let _ = context.eval(Source::from_bytes(REPRO));
    });
    assert!(
        result.is_err(),
        "Boa no longer panics on constructor loop-capture — remove the \
         installAccessors workaround in js/bootstrap.js and this canary"
    );
}
