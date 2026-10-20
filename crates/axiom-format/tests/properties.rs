//! Deterministic generated-input checks: generated programs must parse, and
//! formatting the parsed AST must reach a fixed point.

use axiom_format::format;
use axiom_syntax::parse;

const FUNCTIONS: &[&str] = &[
    "export fn add(a: int, b: int) -> int {\n  return a + b;\n}\n",
    "export fn flag(ok: bool) -> Option<int> {\n  if ok { return Some(1); }\n  return None;\n}\n",
    "export fn pick(xs: string[]) -> string {\n  for x in xs { return x; }\n  return \"\";\n}\n",
    "export enum Kind { A; B; }\nexport fn kind(flag: bool) -> Kind {\n  match flag {\n    true => return Kind.A;\n    false => return Kind.B;\n  }\n}\n",
    "export type Point = { x: int; y: int; };\nexport fn origin() -> Point {\n  return { x: 0, y: 0 };\n}\n",
];

const PADDING: &[&str] = &["\n", "\n\n", "  ", " // note\n", " /* note */ ", "\t"];

#[test]
fn generated_programs_parse_and_format_to_a_fixed_point() {
    let mut state: u64 = 0x5eed_1234_9abc_def0;
    for _ in 0..500 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let count = 1 + (state % 4) as usize;
        let mut source = String::from("module generated;\n");
        for _ in 0..count {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let function = FUNCTIONS[(state as usize) % FUNCTIONS.len()];
            source.push_str(PADDING[(state as usize) % PADDING.len()]);
            source.push_str(function);
            source.push_str(PADDING[(state as usize / 7) % PADDING.len()]);
        }

        let parsed = parse(&source);
        assert!(
            !parsed.diagnostics.iter().any(|d| d.is_error()),
            "generated program did not parse:\n{source}\n{:?}",
            parsed.diagnostics
        );
        let formatted = format(&parsed.file);
        let reparsed = parse(&formatted);
        assert!(
            !reparsed.diagnostics.iter().any(|d| d.is_error()),
            "formatted output did not reparse:\n{formatted}\n{:?}",
            reparsed.diagnostics
        );
        assert_eq!(
            formatted,
            format(&reparsed.file),
            "formatting is not idempotent for:\n{source}"
        );
    }
}
