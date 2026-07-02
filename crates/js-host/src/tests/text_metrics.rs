//! Guards real Skia-based text measurement (`scene.rs`'s `measure_text`,
//! Yoga's measure-function hook) against regressing back to the old
//! `chars().count() * 8.0` heuristic — every assertion here would pass
//! under a fixed-width-per-character guess too, EXCEPT the font-size one,
//! which only real per-glyph measurement can satisfy.

/// Builds `__scCreateView()` wrapping `__scCreateText(text)`, with
/// `fontSize` set on the *View* (matches `react-native.tsx`'s `Text`
/// shim: `<View style={{fontSize, ...}}>{string}</View>`), and returns
/// the text child's natural (unconstrained) measured width.
///
/// `alignItems: "flex-start"` on both container levels — Yoga's default
/// cross-axis alignment (`stretch`) would otherwise stretch `wrap`, and
/// then the text node itself, to the full 2000pt root width regardless
/// of what `measure_text` reports, defeating the point of this helper.
/// Real `@sc/ui` usage never hits this because its containers always
/// have their own explicit (usually much narrower) width.
fn measured_text_width(rt: &super::Runtime, text: &str, font_size: f32) -> f32 {
    rt.eval(&format!(
        r#"
        const root = __scCreateView();
        __scSetStyle(root, JSON.stringify({{ width: 2000, height: 200, alignItems: "flex-start" }}));
        const wrap = __scCreateView();
        __scSetStyle(wrap, JSON.stringify({{ fontSize: {font_size}, alignItems: "flex-start" }}));
        const text = __scCreateText({text:?});
        __scAppendChild(wrap, text);
        __scAppendChild(root, wrap);
        __scSetRoot(root);
        "#,
    ))
    .expect("eval failed");

    super::host::with_scene(|scene| {
        scene.compute_layout(2000.0, 200.0);
        let root = scene.root.expect("root should be set");
        let wrap = scene.children_of(root)[0];
        let text_child = scene.children_of(wrap)[0];
        scene.layout_of(text_child).2
    })
}

#[test]
fn longer_text_measures_wider_at_the_same_font_size() {
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let short = measured_text_width(&rt, "A", 16.0);
    let long = measured_text_width(&rt, "A much longer string of text", 16.0);
    assert!(long > short * 2.0, "a much longer string should measure a lot wider, got short={short} long={long}");
}

#[test]
fn larger_font_size_measures_wider_for_the_same_text() {
    // Only real per-glyph Skia measurement can distinguish this — a
    // `chars().count() * 8.0` heuristic ignores font size entirely and
    // would report the exact same width for both.
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let small = measured_text_width(&rt, "SoundCloud", 12.0);
    let large = measured_text_width(&rt, "SoundCloud", 32.0);
    assert!(large > small * 1.5, "the same text at a much bigger font size should measure a lot wider, got small={small} large={large}");
}

#[test]
fn text_node_shrinks_below_its_natural_width_in_a_tight_flex_container() {
    // `numberOfLines={1}` truncation (Card/TrackRow) only makes sense if
    // the text node's *final* layout width can actually end up smaller
    // than its natural measured width — real `@sc/ui` usage always wraps
    // Text in a container with its own (usually narrower) explicit
    // width, and Yoga's default cross-axis `stretch` does the rest.
    let rt = super::Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");
    let natural = measured_text_width(&rt, "This text is definitely too long to fit", 16.0);

    rt.eval(&format!(
        r#"
        const root = __scCreateView();
        __scSetStyle(root, JSON.stringify({{ width: 60, height: 200 }}));
        const wrap = __scCreateView();
        __scSetStyle(wrap, JSON.stringify({{ fontSize: 16, width: 60 }}));
        const text = __scCreateText({:?});
        __scAppendChild(wrap, text);
        __scAppendChild(root, wrap);
        __scSetRoot(root);
        "#,
        "This text is definitely too long to fit",
    ))
    .expect("eval failed");
    let shrunk = super::host::with_scene(|scene| {
        scene.compute_layout(60.0, 200.0);
        let root = scene.root.expect("root should be set");
        let wrap = scene.children_of(root)[0];
        let text_child = scene.children_of(wrap)[0];
        scene.layout_of(text_child).2
    });

    assert!(shrunk < natural, "a text node inside a 60pt-wide container should shrink well below its {natural}pt natural width, got {shrunk}");
    assert!((shrunk - 60.0).abs() < 1.0, "should shrink down to (approximately) the container's own width, got {shrunk}");
}
