//! More tests of the rules of the `references` and `slices` gates, on the fixture. A
//! submodule of `spec_gates.rs`.

use super::*;

/// Verifies REQ-047: a link to an anchor, a URL or a mail address is not checked, a link in a
/// fenced code block or in a Rust file is not checked, and a link after a code block is.
#[test]
fn references_gate_skips_external_links_and_code() {
    fixture_passes();
    let design = "docs/design/demo.md";
    let fixture = Fixture::new();
    fixture.replace(
        design,
        "# Demo design\n",
        "# Demo design\n\n[a](#test-plan), [b](https://x.org/b.md) and [c](mailto:c@x.org).\n\n\
         ```text\n[d](fenced.md)\n```\n\n[e](after.md)\n",
    );
    fixture.write(
        "crates/demo/src/lib.rs",
        "//! See [f](in-code.md).\n\n/// Also [g](in-code.md).\nfn helper() {}\n",
    );
    let found = fixture.run(spec::references);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_reports(&found, 47, "the link `after.md` points to no file");
    assert!(
        found.iter().all(|v| v.line == Some(9)),
        "the line of the link: {found:?}"
    );
}

/// Verifies REQ-057 criterion 2: a requirement in no package without the status
/// `implemented` makes no violation of the status rules.
#[test]
fn unplanned_requirement_without_implemented_status_passes() {
    let fixture = Fixture::new();
    fixture.replace("docs/plan/SLICE-01.md", &format!("[{}]", req(3)), "[]");
    let found = fixture.run(slice_rules);
    assert_reports(&found, 50, "is in no slice");
    let status: Vec<&Violation> = found.iter().filter(|v| v.rule == 57).collect();
    assert!(status.is_empty(), "{status:?}");
}
