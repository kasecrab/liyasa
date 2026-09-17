//! Where the name under the cursor is written down.
//!
//! ED-61 names snippets and facts. Both are files in the project, and a fact is
//! a key inside one, so the location is narrowed to the key's own line rather
//! than to the top of the file — a `facts/pricing.json` with two hundred
//! entries is not a useful answer on its own. Routes are answered too, because
//! a link target is the same question about a different kind of name.

use std::path::{Path, PathBuf};

use liyasa_core::vfs::{Vfs, VfsPath};

use crate::locate::{self, Target};
use crate::protocol::{Location, Position, Range};
use crate::text::Text;
use crate::uri;
use crate::workspace::Workspace;

/// `root` is the project directory the client opened; without one there is no
/// file to point at, and the request is answered with nothing.
pub fn at(
    text: &Text,
    workspace: &Workspace,
    vfs: &dyn Vfs,
    root: Option<&Path>,
    offset: u32,
) -> Option<Location> {
    let root = root?;
    let (file, key) = match locate::at(text, offset)? {
        Target::Snippet { name, .. } => (workspace.snippets.get(&name)?.file.clone(), None),
        Target::Path { path, .. } => {
            let fact = path
                .strip_prefix("facts.")
                .and_then(|path| workspace.facts.get(path))?;
            // The last segment is the key as the file spells it; the segments
            // above it are the objects on the way down.
            let key = fact.path.rsplit('.').next().map(str::to_owned);
            (fact.file.clone(), key)
        }
        Target::Route { route, .. } => {
            let page = workspace
                .pages
                .get(route.trim_end_matches('/'))
                .or_else(|| workspace.pages.get(&route))?;
            (page.file.clone(), None)
        }
        Target::Component { .. } | Target::Prop { .. } => return None,
    };

    Some(Location {
        uri: uri::from_path(&join(root, &file)),
        range: key
            .and_then(|key| key_position(vfs, &file, &key))
            .map_or_else(Range::default, Range::empty),
    })
}

fn join(root: &Path, file: &VfsPath) -> PathBuf {
    root.join(file.as_str())
}

/// The first line that declares `key`, in either JSON or YAML. A textual search
/// rather than a parse: both formats write the key at the start of its own
/// line, and a parse would have to keep positions the value types do not carry.
fn key_position(vfs: &dyn Vfs, file: &VfsPath, key: &str) -> Option<Position> {
    let bytes = vfs.read(file).ok()?;
    let text = String::from_utf8(bytes.to_vec()).ok()?;
    let quoted = format!("\"{key}\"");
    let bare = format!("{key}:");
    for (line, body) in text.lines().enumerate() {
        let trimmed = body.trim_start();
        if !trimmed.starts_with(&quoted) && !trimmed.starts_with(&bare) {
            continue;
        }
        return Some(Position {
            line: u32::try_from(line).unwrap_or(0),
            character: u32::try_from(body.len() - trimmed.len()).unwrap_or(0),
        });
    }
    None
}
