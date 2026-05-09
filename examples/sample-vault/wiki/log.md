# Log

Append-only chronological ledger. **Alluvium only appends; it never modifies
existing lines.** You can also handwrite your own notes here — Alluvium treats
non-Alluvium content as yours and leaves it alone.

Entry format Alluvium uses: `- HH:MM <session title> → [[touched-pages]]`.

Sessions are grouped under date headings (`## YYYY-MM-DD`). New sessions append
to the matching date heading, or create a new one at the end.
