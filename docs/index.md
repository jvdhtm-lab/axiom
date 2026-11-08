---
layout: default
title: Axiom
description: A small, strongly typed language and runtime for full-stack applications with bounded AI agents.
---

<section class="hero">
  <p class="hero-eyebrow">Axiom 1.0 · language + runtime</p>
  <h1>A small language for full-stack apps with <span class="gradient-text">bounded AI agents</span>.</h1>
  <p class="hero-lede">
    One source language and one shared type/effect system for a trusted Rust
    server and a WebAssembly browser frontend. Agents propose changes; the
    runtime decides what is accepted.
  </p>
  <div class="hero-actions">
    <a class="button button-primary" href="{{ '/quickstart.html' | relative_url }}">Quickstart</a>
    <a class="button button-ghost" href="{{ '/language-tour.html' | relative_url }}">Learn the language</a>
    <a class="button button-ghost" href="https://github.com/jvdhtm-lab/axiom">GitHub</a>
  </div>
  <div class="hero-meta">
    <span class="badge"><span class="badge-dot"></span> v0.1.0 development snapshot</span>
    <span class="badge">targets: server + wasm_browser</span>
    <span class="badge">effects checked</span>
    <span class="badge">SQLite reference driver</span>
  </div>
</section>

<div class="hero-code">
{% highlight typescript %}
module demo.dashboard;

export type DashboardInput = { team_id: string; };
export type DashboardSummary = { team_id: string; completed: int; };
export enum AppError { NotFound; DatabaseUnavailable; }

export api async fn get_dashboard(
  input: DashboardInput
) -> Result<DashboardSummary, AppError>
  effects [db.read] {
  match await db.query_one<DashboardSummary>(
    database: "app",
    sql: "SELECT team_id, completed FROM v_team_performance
          WHERE team_id = :team_id LIMIT 1",
    params: { team_id: input.team_id },
    timeout: 5s
  ) {
    Ok(summary) => return Ok(summary);
    Err(_) => return Err(AppError.DatabaseUnavailable);
  }
}
{% endhighlight %}
</div>

<section class="section">
  <div class="section-head">
    <p class="eyebrow">Get started</p>
    <h2>From zero to a running project</h2>
    <p>Install the CLI, scaffold a two-target app, and check it in a few commands.</p>
  </div>
  <div class="link-grid">
    <a class="link-card" href="{{ '/quickstart.html' | relative_url }}">
      <span class="kicker">5 minutes</span>
      <strong>Quickstart</strong>
      <span>Install the CLI, run <code>axiom createapp</code>, and use check, fmt, lint, run, and build.</span>
    </a>
    <a class="link-card" href="{{ '/language-tour.html' | relative_url }}">
      <span class="kicker">Language</span>
      <strong>Language tour</strong>
      <span>Types, enums, functions, control flow, Option/Result, modules, and effects.</span>
    </a>
    <a class="link-card" href="{{ '/examples.html' | relative_url }}">
      <span class="kicker">Cookbook</span>
      <strong>Examples</strong>
      <span>Server APIs, databases, frontend state, agents, workspaces, and validation.</span>
    </a>
    <a class="link-card" href="{{ '/cli.html' | relative_url }}">
      <span class="kicker">Reference</span>
      <strong>CLI</strong>
      <span>Every command, flag, input, and output the <code>axiom</code> binary supports.</span>
    </a>
  </div>
</section>

<section class="section">
  <div class="section-head">
    <p class="eyebrow">Design</p>
    <h2>Small language, explicit authority</h2>
    <p>The surface stays small; runtime concepts are typed library calls and configuration.</p>
  </div>
  <div class="card-grid">
    <div class="card">
      <h3>Small grammar</h3>
      <p>Only <code>module</code>, <code>import</code>, <code>export</code>, <code>type</code>, <code>enum</code>, <code>fn</code>, <code>agent</code>, and <code>workspace</code> are declarations.</p>
    </div>
    <div class="card">
      <h3>Effects are checked</h3>
      <p>Functions declare what they may do. A call cannot exceed the caller's effects, and browser code cannot claim database or filesystem authority.</p>
    </div>
    <div class="card">
      <h3>Two targets, one checker</h3>
      <p>The same parser, module model, type checks, and typed IR feed the server evaluator and the browser WASM artifact.</p>
    </div>
    <div class="card">
      <h3>Agents inside boundaries</h3>
      <p>Agents have a model alias, tools, a workspace, and hard budgets. Runtime-owned validation decides acceptance, not model prose.</p>
    </div>
    <div class="card">
      <h3>Typed state</h3>
      <p>Frontend state is a normal record. It is validated, versioned, migrated or rejected, and partitioned by tenant and principal.</p>
    </div>
    <div class="card">
      <h3>Driver-independent data</h3>
      <p>Database access is parameterized SQL over named connections. SQLite ships as the reference driver; more can be registered by the host.</p>
    </div>
  </div>
</section>

<section class="section">
  <div class="section-head">
    <p class="eyebrow">Documentation</p>
    <h2>Read the details</h2>
  </div>
  <div class="link-grid">
    <a class="link-card" href="{{ '/language-spec.html' | relative_url }}">
      <strong>Language specification</strong>
      <span>The human-readable specification; the EBNF is normative.</span>
    </a>
    <a class="link-card" href="{{ '/grammar.ebnf' | relative_url }}">
      <strong>Grammar (EBNF)</strong>
      <span>The complete syntax in one plain-text file.</span>
    </a>
    <a class="link-card" href="{{ '/configuration.html' | relative_url }}">
      <strong>Configuration</strong>
      <span><code>axiom.yaml</code>, model aliases, database connections, tools, commands, and budgets.</span>
    </a>
    <a class="link-card" href="{{ '/architecture.html' | relative_url }}">
      <strong>Architecture</strong>
      <span>The compiler pipeline, crates, IR, runtime hosts, and artifact flow.</span>
    </a>
    <a class="link-card" href="{{ '/security-model.html' | relative_url }}">
      <strong>Security model</strong>
      <span>Threat model, trust boundaries, least privilege, and residual risks.</span>
    </a>
    <a class="link-card" href="{{ '/release-readiness.html' | relative_url }}">
      <strong>Release readiness</strong>
      <span>What is proven, what is partial, and what blocks Axiom 1.0.</span>
    </a>
    <a class="link-card" href="{{ '/implementation-ledger.html' | relative_url }}">
      <strong>Implementation ledger</strong>
      <span>Requirement-by-requirement status with the test that proves it.</span>
    </a>
    <a class="link-card" href="{{ '/conformance.html' | relative_url }}">
      <strong>Conformance matrix</strong>
      <span>Maps specification requirements to positive and negative tests.</span>
    </a>
    <a class="link-card" href="{{ '/providers.html' | relative_url }}">
      <strong>Model providers</strong>
      <span>Provider adapters, model aliases, offline mocks, and current gaps.</span>
    </a>
    <a class="link-card" href="{{ '/development.html' | relative_url }}">
      <strong>Development</strong>
      <span>Toolchain, gate commands, Windows notes, and editor setup.</span>
    </a>
    <a class="link-card" href="{{ '/github-pages.html' | relative_url }}">
      <strong>GitHub Pages</strong>
      <span>Publish this site and the deployment contract for Axiom frontends.</span>
    </a>
  </div>
</section>

<section class="section">
  <div class="callout">
    <p><strong>Axiom 1.0 is not complete.</strong> The implementation ledger is the source of truth: a feature is not claimed unless a passing test is cited there.</p>
    <p>Server execution, checks, SQLite, artifacts, workspaces, and agent iteration are implemented for the tested subsets. The browser target emits a verifiable WASM artifact that embeds checked IR, but the DOM renderer and trusted browser host shell are not implemented yet.</p>
  </div>
</section>

<section class="section">
  <div class="section-head">
    <p class="eyebrow">Repository</p>
    <h2>Files outside this site</h2>
    <p>Some references point to files kept in the repository but outside <code>docs/</code>. Those links work on GitHub, not on the rendered Pages site.</p>
  </div>
  <div class="link-grid">
    <a class="link-card" href="https://github.com/jvdhtm-lab/axiom">
      <strong>Repository</strong>
      <span>Source, issues, and the full Git history.</span>
    </a>
    <a class="link-card" href="https://github.com/jvdhtm-lab/axiom/blob/master/agents.md">
      <strong>agents.md</strong>
      <span>The project specification and phase plan.</span>
    </a>
    <a class="link-card" href="https://github.com/jvdhtm-lab/axiom/tree/master/examples">
      <strong>examples/</strong>
      <span>The reference TeamBoard application and runnable source files.</span>
    </a>
    <a class="link-card" href="https://github.com/jvdhtm-lab/axiom/tree/master/editors/vscode">
      <strong>VS Code extension</strong>
      <span>Syntax highlighting for <code>.ax</code> files.</span>
    </a>
  </div>
</section>
