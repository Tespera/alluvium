//! Incremental update of `wiki/index.md`.
//!
//! Walks `wiki/concepts/` and `wiki/entities/`, reads each topic page's
//! frontmatter (title only — body is NOT loaded), and rebuilds the
//! `<!-- ALLUVIUM-INDEX-START -->` … `<!-- ALLUVIUM-INDEX-END -->`-fenced
//! managed region in `wiki/index.md`.
//!
//! Anything outside the managed region (including a `## My notes` user
//! section) is preserved verbatim. Per `examples/sample-vault/wiki/index.md`,
//! the convention is that user-only content lives below the END marker.
//!
//! Frontmatter-only scan keeps the cost O(N pages) with tiny constants —
//! the index can be regenerated for thousands of pages without blowing
//! the LLM context (none is needed; this module is pure I/O + string work).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use super::frontmatter;

const START_MARKER: &str = "<!-- ALLUVIUM-INDEX-START -->";
const END_MARKER: &str = "<!-- ALLUVIUM-INDEX-END -->";

const DEFAULT_INDEX: &str = "# Index\n\n<!-- ALLUVIUM-INDEX-START -->\n<!-- ALLUVIUM-INDEX-END -->\n\n## My notes\n\n<!-- Anything below this line is yours. Alluvium will not touch it. -->\n";

/// Rebuild the managed region of `<alluvium_root>/wiki/index.md`.
pub fn update(alluvium_root: &Path) -> Result<()> {
    let wiki = alluvium_root.join("wiki");
    let entities = collect_entries(&wiki.join("entities"), "entities")?;
    let concepts = collect_entries(&wiki.join("concepts"), "concepts")?;

    let new_block = build_managed_block(&entities, &concepts);
    let index_path = wiki.join("index.md");

    let existing = std::fs::read_to_string(&index_path).unwrap_or_else(|_| DEFAULT_INDEX.into());
    let new_content = replace_managed_region(&existing, &new_block);

    super::writer::write_atomic(&index_path, &new_content)
}

#[derive(Debug)]
struct IndexEntry {
    title: String,
    slug: String,
    subdir: &'static str,
}

fn collect_entries(dir: &Path, subdir_name: &'static str) -> Result<Vec<IndexEntry>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<IndexEntry> = Vec::new();
    let read_dir = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut paths: Vec<PathBuf> = read_dir
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();

    for path in paths {
        let Some(slug) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            tracing::warn!(file = %path.display(), "skipping unreadable index source");
            continue;
        };
        let (fm, _body) = match frontmatter::parse(&content) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    file = %path.display(),
                    error = %format!("{e:#}"),
                    "skipping page with malformed frontmatter"
                );
                continue;
            }
        };
        let title = fm
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(slug)
            .to_string();
        entries.push(IndexEntry {
            title,
            slug: slug.to_string(),
            subdir: subdir_name,
        });
    }
    entries.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(entries)
}

fn build_managed_block(entities: &[IndexEntry], concepts: &[IndexEntry]) -> String {
    let mut s = String::new();
    s.push_str("\n## Projects (entities)\n\n");
    if entities.is_empty() {
        s.push_str("_Empty — no entities yet._\n");
    } else {
        for e in entities {
            s.push_str(&format!("- [[{}/{}]] · {}\n", e.subdir, e.slug, e.title));
        }
    }
    s.push_str("\n## Concepts\n\n");
    if concepts.is_empty() {
        s.push_str("_Empty — no concepts yet._\n");
    } else {
        for c in concepts {
            s.push_str(&format!("- [[{}/{}]] · {}\n", c.subdir, c.slug, c.title));
        }
    }
    s.push('\n');
    s
}

fn replace_managed_region(existing: &str, new_inside: &str) -> String {
    if let (Some(s), Some(e)) = (existing.find(START_MARKER), existing.find(END_MARKER)) {
        if e > s {
            let mut out = String::with_capacity(existing.len() + new_inside.len());
            out.push_str(&existing[..s + START_MARKER.len()]);
            out.push_str(new_inside);
            out.push_str(&existing[e..]);
            return out;
        }
    }
    // No markers in existing — append a managed region at the end without
    // touching the user's prior content.
    let mut out = existing.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(START_MARKER);
    out.push_str(new_inside);
    out.push_str(END_MARKER);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_page(dir: &Path, name: &str, title: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let content = format!("---\ntitle: {title}\ntype: concept\n---\n\n# {title}\n");
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn update_creates_index_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        write_page(
            &alluvium_root.join("wiki/concepts"),
            "claude-code-hooks.md",
            "Claude Code Hooks",
        );
        update(&alluvium_root).unwrap();
        let index = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        assert!(index.contains("Claude Code Hooks"));
        assert!(index.contains("[[concepts/claude-code-hooks]]"));
        assert!(index.contains("ALLUVIUM-INDEX-START"));
        assert!(index.contains("ALLUVIUM-INDEX-END"));
    }

    #[test]
    fn update_groups_entities_and_concepts_separately() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        write_page(
            &alluvium_root.join("wiki/entities"),
            "alluvium.md",
            "Alluvium",
        );
        write_page(&alluvium_root.join("wiki/concepts"), "hooks.md", "Hooks");
        update(&alluvium_root).unwrap();
        let index = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        let entities_pos = index.find("## Projects (entities)").unwrap();
        let concepts_pos = index.find("## Concepts").unwrap();
        let alluvium_pos = index.find("Alluvium").unwrap();
        let hooks_pos = index.find("Hooks").unwrap();
        assert!(entities_pos < alluvium_pos);
        assert!(alluvium_pos < concepts_pos);
        assert!(concepts_pos < hooks_pos);
    }

    #[test]
    fn update_alphabetizes_entries_within_section() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        let concepts_dir = alluvium_root.join("wiki/concepts");
        write_page(&concepts_dir, "zeta.md", "Zeta");
        write_page(&concepts_dir, "alpha.md", "Alpha");
        write_page(&concepts_dir, "mu.md", "Mu");
        update(&alluvium_root).unwrap();
        let index = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        let alpha = index.find("Alpha").unwrap();
        let mu = index.find("Mu").unwrap();
        let zeta = index.find("Zeta").unwrap();
        assert!(alpha < mu);
        assert!(mu < zeta);
    }

    #[test]
    fn update_preserves_user_section_below_end_marker() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        write_page(&alluvium_root.join("wiki/concepts"), "x.md", "X");

        // Pre-seed index.md with user content below the END marker.
        let wiki = alluvium_root.join("wiki");
        std::fs::create_dir_all(&wiki).unwrap();
        let initial = "# Index\n\n<!-- ALLUVIUM-INDEX-START -->\n(stale)\n<!-- ALLUVIUM-INDEX-END -->\n\n## My notes\n\nMy hand-written content I care about.\n";
        std::fs::write(wiki.join("index.md"), initial).unwrap();

        update(&alluvium_root).unwrap();
        let after = std::fs::read_to_string(wiki.join("index.md")).unwrap();
        assert!(after.contains("My hand-written content I care about"));
        assert!(!after.contains("(stale)"));
        assert!(after.contains("X"));
    }

    #[test]
    fn update_is_idempotent_when_nothing_changes() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        write_page(&alluvium_root.join("wiki/concepts"), "x.md", "X");
        update(&alluvium_root).unwrap();
        let first = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        update(&alluvium_root).unwrap();
        let second = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn update_with_empty_directories_renders_empty_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        std::fs::create_dir_all(alluvium_root.join("wiki/concepts")).unwrap();
        std::fs::create_dir_all(alluvium_root.join("wiki/entities")).unwrap();
        update(&alluvium_root).unwrap();
        let index = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        assert!(index.contains("_Empty — no entities yet._"));
        assert!(index.contains("_Empty — no concepts yet._"));
    }

    #[test]
    fn update_skips_pages_with_unreadable_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let alluvium_root = dir.path().join("Alluvium");
        let concepts_dir = alluvium_root.join("wiki/concepts");
        std::fs::create_dir_all(&concepts_dir).unwrap();
        // Good page.
        write_page(&concepts_dir, "good.md", "Good Page");
        // Bad page (unterminated frontmatter is forgiving — falls through to no-fm).
        // Use truly invalid YAML inside a fenced block.
        std::fs::write(
            concepts_dir.join("broken.md"),
            "---\nkey: : : invalid yaml\n---\nbody",
        )
        .unwrap();
        update(&alluvium_root).unwrap();
        let index = std::fs::read_to_string(alluvium_root.join("wiki/index.md")).unwrap();
        assert!(index.contains("Good Page"));
        // broken.md should be skipped.
        assert!(!index.contains("broken"));
    }

    #[test]
    fn replace_managed_region_with_no_markers_appends_new_block() {
        let existing = "# Index\n\nUser-only content here.\n";
        let result = replace_managed_region(existing, "\nNEW BLOCK\n");
        assert!(result.contains("User-only content here"));
        assert!(result.contains("ALLUVIUM-INDEX-START"));
        assert!(result.contains("NEW BLOCK"));
        assert!(result.contains("ALLUVIUM-INDEX-END"));
    }
}
