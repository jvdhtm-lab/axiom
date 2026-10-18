# Architecture decision records

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-workspace-and-toolchain.md) | Cargo workspace layout and pinned GNU toolchain | accepted |
| [0002](0002-hand-written-lexer-parser.md) | Hand-written lexer and recursive-descent parser | accepted |
| [0003](0003-config-precedence.md) | Configuration precedence | accepted |
| [0004](0004-provider-boundary.md) | Adapter / integration / model-profile separation | accepted |
| [0005](0005-concept-separation.md) | Five concepts are never collapsed | accepted |
| [0006](0006-newline-significant-bodies.md) | Line breaks are entry separators in bodies (superseded by 0011) | superseded |
| [0007](0007-diagnostics-and-recovery.md) | Spanned diagnostics with error recovery | accepted |
| [0008](0008-source-vs-configuration.md) | `.ax` source versus `axiom.yaml` configuration | accepted |
| [0009](0009-yaml-dependency.md) | YAML parsing dependency (`yaml_serde`) | accepted |
| [0010](0010-rebaseline-axiom-1-0.md) | Re-baseline to Axiom 1.0 | accepted |
| [0011](0011-semicolons-and-member-blocks.md) | Semicolon-terminated statements and member blocks | accepted |
| [0012](0012-semantic-analysis-scope.md) | Semantic analysis scope for Phase 2 | accepted (decision 6 superseded by 0013) |
| [0013](0013-strict-resolution-and-multi-target.md) | Strict name resolution and multi-target/frontend scope | accepted |

ADRs are immutable once accepted; a later ADR supersedes an earlier one rather
than editing it.
