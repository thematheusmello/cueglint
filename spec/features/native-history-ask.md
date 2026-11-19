# Ask from History search

A nonblank search shows **Ask the assistant** ahead of the saved results.
Filtering and editing the search stay local to History. Activating the action
submits the trimmed text to a fresh conversation using the chosen mode. This
works with matching rows and with an empty result list.

The action saves pending note edits before queuing the question. The History
query and the ordinary assistant input keep their own drafts. A fresh request
excludes the previously selected meeting and its replies. An active response
keeps ownership until it finishes or the user stops it.

History uses the current answer model and its ordinary capture settings. When
screen context is enabled, the action identifies the configured display or
selected-window text route. A hosted provider is identified before submission.
With the single-window route, the existing system picker still controls the
window choice. Failures keep the query available for another explicit attempt.

Original fixture checks exercise the accessible action, pending note ordering,
bounded input, refusal during active generation, and the new request's context.
The pinned reference observation establishes the leading action in both search
states. Questions grounded across prior meetings require their own evidence.
[Reference session observations](reference-sessions-assistant.md)
