//! `alluvium consolidate` — placeholder for v0.1.
//!
//! In v0.1 this command is intentionally a no-op stub. The full
//! implementation (LLM-driven rewrite of fragmented topic pages to defend
//! against append-only drift, per ADR-009 + KNOWLEDGE_MODEL.md) lands in
//! v0.2 once we have real-world Alluvium-managed vaults to test against.

use anyhow::Result;

pub async fn run() -> Result<()> {
    println!(
        "alluvium consolidate: not implemented in v0.1.\n\n\
         The full consolidate command (LLM-driven rewrite of fragmented topic\n\
         pages to defend against append-only drift) lands in v0.2.\n\n\
         For now, manual consolidation works fine: open a topic page in\n\
         Obsidian, edit content INSIDE the alluvium:fact blocks (it will be\n\
         overwritten next archive — so move good content OUTSIDE the markers\n\
         to preserve it), or split a long page into multiple topic pages by\n\
         hand."
    );
    Ok(())
}
