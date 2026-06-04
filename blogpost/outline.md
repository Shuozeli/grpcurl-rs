# Blog Post Outline: AI Agent Migration of grpcurl (Go to Rust)

## Title Candidates
- "I mass-prompted an AI agent to migrate grpcurl from Go to Rust -- here's the honest scorecard"
- "40 AI agent sessions to migrate a Go CLI to Rust: what worked, what didn't, and what surprised me"
- "69/72 byte-identical outputs: migrating grpcurl from Go to Rust with Claude Code"

## Posting Strategy
- Target: Friday 14:00-17:00 UTC (9am-12pm ET)
- Lead with the migration story, not the AI angle
- Be honest about failures -- HN rewards intellectual honesty
- Include concrete numbers (LOC, test counts, match rates, session counts)
- Respond to early comments quickly

---

## 1. The Setup (what and why)
- grpcurl: popular Go CLI for interacting with gRPC servers (~10k GitHub stars)
- Goal: full-featured Rust port with behavioral parity, not a rewrite-from-scratch
- Tool: Claude Code (Anthropic's CLI agent) as the primary implementer
- Human role: architect, reviewer, course-corrector -- not a typist
- 40 documented interaction steps over the full migration

## 2. What the Agent Did Well
- **Go-to-Rust translation at speed**: 12 source modules, ~5000 lines of Rust, generated across sessions with minimal manual edits
- **Byte-for-byte output parity**: 69/72 verification cases produce identical output to Go (the 3 diffs are cosmetic: help flag syntax, version string, timing data)
- **Test generation**: 160 tests (57 unit + 103 integration) -- the agent wrote all of them, including test infrastructure (TestServer with ephemeral ports, shared helpers, testdata)
- **Idiomatic translation, not transliteration**: used Rust enums instead of Go interfaces, `LazyLock` instead of `sync.Once`, `DescriptorPool` O(1) lookups instead of Go's linear scan, `InvokeContext` struct instead of 8-parameter functions
- **Dependency wiring**: correctly integrated tonic, prost-reflect, protox, rustls with custom certificate verifiers, Unix socket connectors, gzip compression
- **Bug discovery during testing**: found and fixed a real bug in reflection descriptor resolution (inter-dependent files + missing well-known type dependencies) that the Python verification scripts had missed

## 3. What the Agent Cannot Do (honest failures)
- **Does not think ahead about architecture**: built a monolithic binary first, never suggested "this should be a library crate too." Human had to recognize (at Step 33) that Go's `package grpcurl` is both a library and CLI, then drive the extraction
- **Constant restructuring was human-driven**: 3 major restructures (monolith -> library extraction -> CLI submodule -> workspace cleanup), each prompted by the human. The agent executes restructuring flawlessly but never initiates it
- **No pushback on design**: agent never said "this will be hard to test later" or "this coupling will bite us." It builds exactly what you ask, including designs with foreseeable problems
- **Verbosity management**: agent tends to over-document (800+ line migration journal) and over-build (added comprehensive test helpers some of which were unused). Human had to prune
- **Cannot verify its own correctness end-to-end**: the agent wrote tests, but the human had to design the verification strategy (side-by-side Go vs Rust comparison, 72-case report). The agent doesn't spontaneously ask "how do we know this is correct?"

## 4. Why I Had to Constantly Restructure
- The agent builds forward, never backward -- it optimizes for "make the next test pass," not "will this structure scale?"
- Step 1-14: everything in one crate, fine for prototyping
- Step 33: human realizes the library should be extractable (like Go's `package grpcurl`)
- Step 35-36: extract `grpcurl-core` library crate
- Step 37: move CLI into `grpcurl-cli` submodule, root becomes pure workspace
- Each restructure was painless to execute (agent moved files, fixed paths, zero code changes needed) but the agent would never have suggested any of them
- **Takeaway**: AI agents are excellent refactoring executors but poor refactoring initiators

## 5. Risks in the Current Project
- **prost-reflect dependency**: the entire dynamic message system (parsing, formatting, reflection, codec) depends on one crate. If it breaks or is abandoned, the migration is stuck. Go's `protoreflect` is battle-tested; Rust's ecosystem is younger
- **~~unsafe Send/Sync~~**: Previously had one `unsafe impl` on `ServerSource`, but this was resolved by switching to `Mutex<DescriptorPool>` which auto-derives Send+Sync. No unsafe code remains in the codebase
- **async/sync bridge**: `block_in_place` + `block_on` pattern to call async reflection from a sync trait. Works but is a runtime hack -- a truly async `DescriptorSource` trait would be cleaner
- **No performance benchmarks**: zero latency/throughput comparison vs Go. The agent never suggested benchmarking. For a CLI tool this may not matter, but we genuinely don't know if it's faster or slower
- **Binary size**: 6.9MB release (with LTO+strip) vs Go's ~15MB. Better, but driven by human-added profile settings, not agent initiative

## 6. Verification Done
- **72-case automated comparison**: Go vs Rust output comparison across all command types (list, describe, invoke), all 4 streaming types, verbose output, error messages, export formats
- **69/72 byte-identical**: 3 permanent cosmetic differences documented
- **160 Rust integration tests**: all pass, covering offline (protoset/proto) and online (reflection server) scenarios
- **Clippy clean**: 0 warnings after productionalization
- **Manual spot-checks**: ran against Go's test server for every feature during development

## 7. How This Changes the Software Industry
- **Migration-as-a-service becomes viable**: the bottleneck for language migrations was always "who will do the tedious line-by-line work?" AI agents remove that bottleneck. The human cost shifts from implementation to architecture and verification
- **The architect role becomes critical**: AI makes junior implementation work trivially cheap, but someone still needs to say "this should be a library" or "we need a verification strategy." The gap between "code that compiles" and "code that's well-structured" is entirely a human judgment call today
- **Verification is the new bottleneck**: generating code is fast; proving it's correct is slow. The 72-case verification report took more human thought than any single implementation step
- **Technical debt accumulates differently**: instead of "we hacked this together under deadline pressure," it's "the agent built exactly what we asked for, and what we asked for was wrong." Debt comes from insufficient human foresight, not insufficient human time
- **Open source maintenance changes**: one person + AI agent can now maintain a meaningful Rust port of a Go tool. The "bus factor" concern shifts from "can anyone maintain this code?" to "does anyone understand the architectural decisions?"

## 8. Closing (the honest take)
- AI agents are force multipliers, not replacements. This migration would have taken weeks of manual work; it took days of directed agent sessions
- The agent wrote ~95% of the code by volume. The human made ~95% of the decisions that mattered
- The result is production-quality code that I'd ship -- but only because a human was reviewing every step
- If I had let the agent run unsupervised, I'd have a working monolithic binary with no library extraction, no test verification strategy, and no awareness of its own architectural debt
