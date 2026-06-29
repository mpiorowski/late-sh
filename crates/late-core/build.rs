use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR environment variable is not set by cargo");
    let dest_path = Path::new(&out_dir).join("migrations.rs");

    let migrations_dir = Path::new("migrations");

    // Tell cargo to re-run the build script if the migrations directory changes.
    // This handles both new files and modified files.
    println!("cargo:rerun-if-changed=migrations");

    if !migrations_dir.exists() {
        fs::write(
            &dest_path,
            "const MIGRATIONS: &[(&str, &str)] = &[];
",
        )
        .expect("Failed to write empty migrations.rs");
        return;
    }

    let mut entries: Vec<_> = fs::read_dir(migrations_dir)
        .expect("Failed to read migrations directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("sql"))
        .collect();

    entries.sort();

    let mut migrations_code = String::new();
    migrations_code.push_str(
        "/// Auto-generated list of migrations
",
    );
    migrations_code.push_str(
        "const MIGRATIONS: &[(&str, &str)] = &[
",
    );

    for path in entries {
        let file_stem = path
            .file_stem()
            .expect("Migration file must have a stem")
            .to_str()
            .expect("Migration file stem must be valid UTF-8");

        let abs_path = fs::canonicalize(&path)
            .unwrap_or_else(|e| panic!("Failed to canonicalize path {:?}: {}", path, e));

        let abs_path_str = abs_path
            .to_str()
            .expect("Absolute path must be valid UTF-8");

        migrations_code.push_str(&format!(
            "    (\"{file_stem}\", include_str!(\"{abs_path_str}\")),\n"
        ));
    }

    migrations_code.push_str(
        "];
",
    );

    fs::write(&dest_path, migrations_code)
        .unwrap_or_else(|e| panic!("Failed to write migrations.rs to {:?}: {}", dest_path, e));
}
