//! CLI-13: `liyasa schema [config|frontmatter|components]`.

use liyasa_config::schema::{SCHEMAS, named, schema_url};

use crate::Exit;
use crate::cli::{Global, Schema};

pub fn run(global: &Global, args: &Schema) -> Exit {
    let Some(which) = &args.which else {
        if global.json {
            let names: Vec<&str> = SCHEMAS.iter().map(|schema| schema.name).collect();
            println!("{}", serde_json::json!({ "schemas": names }));
        } else {
            println!("schemas this build can print:");
            for schema in SCHEMAS {
                println!("  {:<12} {}", schema.name, schema_url(schema));
            }
        }
        return Exit::Success;
    };

    match named(which) {
        Some(schema) => {
            println!("{}", schema.json.trim_end());
            Exit::Success
        }
        None => {
            let known: Vec<&str> = SCHEMAS.iter().map(|schema| schema.name).collect();
            eprintln!(
                "no schema named `{which}`; this build has {}",
                known.join(", ")
            );
            // The command line named something that does not exist, which is a
            // usage error rather than a problem with the project.
            Exit::Usage
        }
    }
}
