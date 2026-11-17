# Native answer content: implementation contract

This is an independently authored implementation contract. Reference observations remain
in `reference-ui.md` and `reference-findings.json`; this document adds no observed reference
state, geometry or successful parity claim. Scope is completed and streaming assistant
answers, with editable notes and their autosave path preserved.

- Render paragraphs, headings, emphasis, lists, quotes, rules and fenced code through
  the single native egui stack. Body size is14points; current exact reference font metrics
  and wrapping are unverified. Preserve text/code source for exact copying.
- Images show alt text; raw HTML is literal. Formatting performs no network fetch. Only an
  explicit click on a validated HTTP(S) link may open it. Mixed-format link labels expose one
  accessible target. Revision-scoped link identity rejects delayed old accessibility clicks
  after a same-length destination rewrite. Full-response Copy uses the latest exact source.
- Parse outside drawing on one coalesced cancellable worker. Results match session, request,
  context and text revision. Parser limits:2MiB input,100000events,25000nodes, depth32 and
  4MiB AST text. Cache:16answers/4MiB source. Render:2048nodes/256KiB per answer. Bounded source
  previews show an explicit notice at64KiB; the full original remains available to Copy.
- A long list below the render budget, including the original80-item fixture, remains whole.
  Histories above16 replies use Earlier/Later/Latest windows; older views retain their last
  visible request anchor as new replies arrive. This is a temporary continuous-scroll gap.
- Deterministic native fixtures `markdown` and `markdown-long` seed a stopped saved session
  with original visibly labelled answers. They use690×500 primary viewports and no inference,
  capture or downloads. Screenshot capture waits for the actual formatted document.

Implementation tests cover stale worker results, malformed/bounded input, literal hostile
content, URL rules, code/source copying, link/selection accessibility roles, streaming/context
replacement,200-answer navigation and stable copy identity. These headless checks do not
establish native OS accessibility or reference parity. Native screenshots and matched
interaction scenarios have separate evidence. Offscreen/streaming selection persistence,
full note formatting, typography, extended table layout and continuous history scrolling
remain unverified/gapped.
