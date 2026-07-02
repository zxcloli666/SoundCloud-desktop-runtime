use rusty_hermes::{Runtime, hermes_op};

#[hermes_op]
fn add(a: f64, b: f64, c: f64) -> f64 {
    a + b + c
}

#[test]
fn evaluates_js_and_calls_back_into_rust() {
    let rt = Runtime::new().expect("failed to create Hermes runtime");
    add::register(&rt).expect("failed to register add()");

    let result = rt.eval("add(10, 20, 30)").expect("eval failed");
    assert_eq!(result.as_number(), Some(60.0));
}

#[test]
fn mounts_a_tree_via_host_functions_and_computes_layout() {
    let rt = Runtime::new().expect("failed to create Hermes runtime");
    super::host::install(&rt).expect("failed to install host functions");

    rt.eval(
        r#"
        const root = __scCreateView();
        __scSetStyle(root, JSON.stringify({ width: 400, height: 200, flexDirection: "row" }));

        const a = __scCreateView();
        __scSetStyle(a, JSON.stringify({ flexGrow: 1, backgroundColor: [0.2, 0.4, 0.9, 1.0] }));
        __scAppendChild(root, a);

        const b = __scCreateView();
        __scSetStyle(b, JSON.stringify({ flexGrow: 1, backgroundColor: [0.9, 0.3, 0.5, 1.0] }));
        __scAppendChild(root, b);

        __scSetRoot(root);
        "#,
    )
    .expect("eval failed");

    super::host::with_scene(|scene| {
        scene.compute_layout(400.0, 200.0);
        let root = scene.root.expect("root should be set");
        assert_eq!(scene.layout_of(root), (0.0, 0.0, 400.0, 200.0));

        // Two equal flex-grow children in a row should split 400pt evenly.
        let children = scene.children_of(root);
        assert_eq!(children.len(), 2);
        assert_eq!(scene.layout_of(children[0]), (0.0, 0.0, 200.0, 200.0));
        assert_eq!(scene.layout_of(children[1]), (200.0, 0.0, 200.0, 200.0));
    });
}
