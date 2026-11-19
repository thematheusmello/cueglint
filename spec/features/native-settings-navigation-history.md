# Native Settings navigation history

Cueglint keeps recent Settings categories while the app is open. The sidebar,
links from other categories and links from the assistant use the same history.
Back and Forward revisit categories without sending commands to the session
worker. Selecting the current category keeps the current history. Choosing a new
category after Back clears the forward path. History retains up to 64 steps.

The controls use Cueglint's own icons, disabled states and visible keyboard focus.
The hit areas are 40 × 36 logical points at x=230 and x=271, y=6 in the Settings
viewport. Section headings have space to the right of the controls. The General
hero and sidebar rows keep their existing coordinates.

Search text, mode and provider drafts, and the backup path stay in place.
Back and Forward are disabled during backup review and restore, and an action
addressed to a previously enabled control cannot leave the review.

The [pinned observation](reference-ui.md#settings-window) records visible Back
and Forward controls. [The separate 2.1.29 measurement](reference-general-visible-2.1.29.md)
records their hit areas. Category history and keyboard behavior are Cueglint
implementation choices. The reference transition behavior remains unobserved.
