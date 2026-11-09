// Prism grammar for Axiom (.ax) source. The core is vendored in
// assets/vendor/prism-core.min.js so the docs site has no runtime CDN dependency.
(function (global) {
  if (!global || !global.Prism) {
    return;
  }

  global.Prism.languages.axiom = {
    comment: [
      { pattern: /\/\/.*/, greedy: true },
      { pattern: /\/\*[\s\S]*?(?:\*\/|$)/, greedy: true }
    ],
    string: {
      pattern: /"(?:\\.|[^"\\])*"/,
      greedy: true,
      inside: {
        escape: { pattern: /\\./, alias: "escape" }
      }
    },
    namespace: {
      pattern: /\b[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)+\b/,
      alias: "namespace"
    },
    keyword: /\b(?:module|import|export|from|type|enum|fn|agent|workspace|api|async|effects|let|if|else|match|for|in|return|await)\b/,
    boolean: /\b(?:true|false)\b/,
    builtin: /\b(?:string|bool|int|float|bytes|duration|timestamp|Option|Result|Map|UiTree|Some|None|Ok|Err)\b/,
    number: [
      /\b\d+(?:ms|s|m|h)\b/,
      /\b\d+(?:B|kB|KB|MB|GB|TB|KiB|MiB|GiB|TiB)\b/,
      /\b\d+\.\d+\b/,
      /\b\d+\b/
    ],
    "class-name": /\b[A-Z][A-Za-z0-9_]*\b/,
    function: /\b[a-z_][A-Za-z0-9_]*(?=\s*\()/,
    property: {
      pattern: /\b[a-z_][A-Za-z0-9_]*(?=\s*:)/,
      alias: "property"
    },
    operator: /==|!=|<=|>=|->|=>|&&|\|\||[+\-*/%!<>=]/,
    punctuation: /[{}[\];(),.:]/
  };

  function highlightAxiomBlocks() {
    var blocks = global.document.querySelectorAll("code.language-axiom");
    for (var index = 0; index < blocks.length; index += 1) {
      global.Prism.highlightElement(blocks[index]);
    }
  }

  if (global.document) {
    if (global.document.readyState === "loading") {
      global.document.addEventListener("DOMContentLoaded", highlightAxiomBlocks);
    } else {
      highlightAxiomBlocks();
    }
  }
})(typeof window !== "undefined" ? window : globalThis);
