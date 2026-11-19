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
states.
[Reference session observations](reference-sessions-assistant.md)

## Ask saved meetings locally

History offers a separate opt-in action when the query is nonblank. It saves
pending edits, requires the selected Local answer model, and opens a durable
text conversation titled **History chat**. The request contains saved meeting
excerpts, the selected mode and previous local replies from that conversation.
Screen capture and knowledge files are excluded. The query stays available
after an error, including when there are no usable excerpts. Reopen the
conversation from its History row. Selecting a hosted provider disables new
questions and follow-ups in this conversation. Ordinary assistant questions
keep their configured provider and capture behavior.

Retrieval ranks distinct case-insensitive query terms in passage text and
meeting titles. It searches up to 64 recently saved stopped meetings, up to
16 KiB each of notes and summary and up to 64 KiB of finalized transcript
text per meeting. Terms and ranking ties use deterministic limits and ordering.
Each request retains up to four passages of 256 characters, with at most two
per meeting. A follow-up with no lexical match can reuse the previous answer's
exact excerpts. Expand **Saved meeting sources** to see the original text and
the meeting title, date, source type, and citation label used in the prompt.

Each frozen excerpt retains the source meeting ID, field or transcript
segment ID, exact UTF-8 byte span and SHA-256 of its frozen title and text.
Renaming a meeting preserves the original attribution. When a cited span
changes or its meeting is deleted, a transaction removes the dependent
conversation's excerpts, answers and previous assistant turns. The affected
requests show a withdrawal message when reopened and reject late model
results. The conversation keeps authored questions and notes and can continue
with available sources. Previously exported copies and independent backups
retain their original contents.

Source selection and request persistence use one SQLite transaction. The
desktop and CLI take the same exclusive database lock for their lifetime.
Use Cueglint's commands for edits and deletion so the active worker can
retire a dependent stream. Direct SQLite changes cannot notify that worker.

The lexical retrieval, local provider boundary and withdrawal are Cueglint
implementation contracts. Matched reference behavior, native visual review
and quality measurements on a real model remain pending.
