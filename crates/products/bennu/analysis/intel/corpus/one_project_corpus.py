#!/usr/bin/env python3
"""Reshape the per-case corpus into ONE Java project — for pointing an IDE at it.

Each case becomes a package of its own (the case name is already a legal Java identifier), which is
what keeps 166 classes called `A` from colliding in a single project. A `package` line is prepended
unless the file already declares one — `misc_package_mismatch` declares a WRONG one on purpose,
which is the whole case.

javac is then re-run PER CASE, from inside the case's own directory, so the goldens land beside the
sources exactly as `javac_diff` expects them. Per case rather than once over the whole project, for
two reasons that each make a single run useless as an oracle:

  * `-XDrawDiagnostics` prints a diagnostic's file by its BASE NAME, so 166 files called `A.java`
    come back indistinguishable;
  * a parse error stops attribution for the WHOLE compilation, so the handful of cases written to be
    syntactically broken would take every other case's semantic errors down with them.

    python3 one_project_corpus.py <per-case-corpus> <out-dir>

## What this was built for, and what came of it

To compare Bennu against IntelliJ on the same files. **It does not work, and the reason is worth
recording so nobody spends the afternoon again**: IntelliJ's offline `inspect.sh` runs INSPECTIONS,
and a Java compile error is not one. The red squiggle under `int x = "s";` comes from the platform's
highlighting pass (`HighlightVisitor`), which the offline runner does not run — measured on this
corpus with IDEA Ultimate 2026.1.4, of 147 javac error lines the offline run reported **2** at ERROR
severity, both syntax-level, from the one inspection (`Annotator`) that does surface parser errors.
The 1940 inspections the IDE registers include `GrUnresolvedAccess` for Groovy and `JSAnnotator` for
JavaScript — languages whose front end IS annotator-based — and nothing equivalent for Java.

So a real IntelliJ comparison needs either a small IDE plugin that drives `DaemonCodeAnalyzer`
headless, or the UI. What the offline run IS good for is the other half: the inspections proper
(dataflow, unused, redundancy), which is a different question from "does it see the error".
"""

import os, re, shutil, subprocess, sys

src_root, out_root = sys.argv[1], sys.argv[2]
src_dir = os.path.join(out_root, "src")
if os.path.isdir(out_root):
    shutil.rmtree(out_root)
os.makedirs(src_dir)

cases = sorted(d for d in os.listdir(src_root) if os.path.isdir(os.path.join(src_root, d)))
files_written = 0
for case in cases:
    for dirpath, _, files in os.walk(os.path.join(src_root, case)):
        for f in files:
            if not f.endswith(".java"):
                continue
            rel = os.path.relpath(os.path.join(dirpath, f), os.path.join(src_root, case))
            text = open(os.path.join(dirpath, f)).read()
            dest = os.path.join(src_dir, case, rel)
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            if not re.match(r"\s*package\s", text):
                sub = os.path.dirname(rel)
                pkg = ".".join([case] + sub.split(os.sep)) if sub else case
                text = f"package {pkg};\n" + text
            open(dest, "w").write(text)
            files_written += 1

total = 0
for case in cases:
    d = os.path.join(src_dir, case)
    rels = sorted(
        os.path.relpath(os.path.join(dp, f), d)
        for dp, _, fs in os.walk(d) for f in fs if f.endswith(".java")
    )
    classes = os.path.join(d, "_classes")
    r = subprocess.run(
        ["javac", "-XDrawDiagnostics", "-Xmaxerrs", "10000", "-nowarn", "-d", "_classes"] + rels,
        cwd=d, capture_output=True, text=True)
    shutil.rmtree(classes, ignore_errors=True)
    open(os.path.join(d, "expected.out"), "w").write(r.stderr)
    total += sum(1 for l in r.stderr.splitlines() if ": compiler.err." in l)

print(f"cases {len(cases)}  files {files_written}  javac error lines {total}")
