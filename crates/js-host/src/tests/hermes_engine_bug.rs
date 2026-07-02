//! Documents a genuine Hermes engine bug found while wiring up real `@sc/ui`
//! (spike 7): a `for (let key of ...)` loop whose body defines a closure via
//! `Object.defineProperty` doesn't get a fresh `key` binding per iteration —
//! every getter ends up seeing the *last* key. This is exactly the shape of
//! esbuild's own CJS→ESM interop helper (`__copyProps`), which `js/build.mjs`
//! patches post-build (swaps the loop for `.forEach`, where `key` is a
//! function parameter instead of a loop-scoped `let`). If Hermes ever fixes
//! this, both `createContextType` assertions below would need to flip to
//! "function" — that's the signal to remove the build.mjs patch too.
//!
//! `rt.eval(js)` below runs a small, hardcoded inline repro string owned by
//! this test, never external input — Hermes' ordinary script-execution
//! entry point, not a code-injection risk.

#[test]
fn reproduces_in_isolation() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    let js = r#"
    var __getOwnPropNames = Object.getOwnPropertyNames;
    var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
    var __hasOwnProp = Object.prototype.hasOwnProperty;
    var __defProp = Object.defineProperty;
    var copyPropsForOf = (to, from, except, desc) => {
      if (from && typeof from === "object" || typeof from === "function") {
        for (let key of __getOwnPropNames(from))
          if (!__hasOwnProp.call(to, key) && key !== except)
            __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
      }
      return to;
    };

    var fakeModule = { firstFn: function () { return "ok"; }, version: "1.2.3" };
    var wrapped = copyPropsForOf({}, fakeModule);
    JSON.stringify({ firstFnType: typeof wrapped.firstFn, firstFnValue: String(wrapped.firstFn) });
    "#;
    let result = rt.eval(js).expect("eval failed").into_string().expect("result should be a string").to_rust_string().expect("valid utf8");
    assert_eq!(
        result,
        r#"{"firstFnType":"string","firstFnValue":"1.2.3"}"#,
        "if this ever reads back as a function, the Hermes bug is fixed — go remove the build.mjs __copyProps patch",
    );
}
