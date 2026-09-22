//! The tag vocabulary: one closed list of canonical tags, each with the
//! spellings people actually type. Two readers, one list. The work card
//! editor turns typed skills into `skills_tags`, and the job press turns a
//! posting's stack into the same tags, so a match is a plain array overlap
//! with no fuzzy comparison at query time.

/// A canonical tag and every alias that folds onto it. Aliases are matched
/// after lowercasing and trimming a leading `#`; the canonical spelling is
/// itself an alias, so the table reads as one row per tag.
pub(crate) const VOCAB: &[(&str, &[&str])] = &[
    // languages
    ("rust", &["rust", "rustlang"]),
    ("go", &["go", "golang"]),
    ("elixir", &["elixir", "ex"]),
    ("erlang", &["erlang", "beam"]),
    ("python", &["python", "py", "python3"]),
    ("typescript", &["typescript", "ts"]),
    ("javascript", &["javascript", "js", "es6", "ecmascript"]),
    ("c", &["c", "clang", "ansi-c"]),
    ("cpp", &["cpp", "c++", "cplusplus"]),
    ("csharp", &["csharp", "c#", "dotnet", ".net"]),
    ("java", &["java", "jvm"]),
    ("kotlin", &["kotlin", "kt"]),
    ("swift", &["swift"]),
    ("ruby", &["ruby", "rb"]),
    ("php", &["php"]),
    ("scala", &["scala"]),
    ("clojure", &["clojure", "clj"]),
    ("haskell", &["haskell", "hs"]),
    ("ocaml", &["ocaml"]),
    ("zig", &["zig"]),
    ("nim", &["nim"]),
    ("lua", &["lua"]),
    ("perl", &["perl"]),
    ("dart", &["dart"]),
    ("r", &["r", "rlang"]),
    ("julia", &["julia"]),
    ("sql", &["sql"]),
    ("bash", &["bash", "shell", "sh", "zsh", "fish", "scripting"]),
    ("powershell", &["powershell", "pwsh"]),
    ("nix", &["nix", "nixos"]),
    ("wasm", &["wasm", "webassembly"]),
    ("solidity", &["solidity"]),
    ("assembly", &["assembly", "asm"]),
    // frameworks and runtimes
    ("react", &["react", "reactjs", "react.js"]),
    ("nextjs", &["nextjs", "next", "next.js"]),
    ("vue", &["vue", "vuejs", "vue.js", "nuxt"]),
    ("svelte", &["svelte", "sveltekit"]),
    ("angular", &["angular", "angularjs"]),
    ("node", &["node", "nodejs", "node.js"]),
    ("deno", &["deno"]),
    ("bun", &["bun"]),
    ("django", &["django"]),
    ("flask", &["flask"]),
    ("fastapi", &["fastapi"]),
    ("rails", &["rails", "ruby-on-rails", "ror"]),
    ("phoenix", &["phoenix", "liveview"]),
    ("spring", &["spring", "springboot", "spring-boot"]),
    ("laravel", &["laravel"]),
    ("axum", &["axum"]),
    ("actix", &["actix"]),
    ("tokio", &["tokio"]),
    ("tauri", &["tauri"]),
    ("electron", &["electron"]),
    ("flutter", &["flutter"]),
    ("react-native", &["react-native", "reactnative", "rn"]),
    ("ios", &["ios", "swiftui", "uikit"]),
    ("android", &["android"]),
    ("unity", &["unity", "unity3d"]),
    ("unreal", &["unreal", "ue4", "ue5"]),
    ("godot", &["godot"]),
    ("bevy", &["bevy"]),
    ("tui", &["tui", "ratatui", "ncurses", "curses"]),
    ("cli", &["cli", "command-line"]),
    // data
    ("postgres", &["postgres", "postgresql", "pg", "psql"]),
    ("mysql", &["mysql", "mariadb"]),
    ("sqlite", &["sqlite"]),
    ("redis", &["redis", "valkey"]),
    ("mongodb", &["mongodb", "mongo"]),
    ("elasticsearch", &["elasticsearch", "opensearch"]),
    ("kafka", &["kafka"]),
    ("rabbitmq", &["rabbitmq", "amqp"]),
    ("clickhouse", &["clickhouse"]),
    ("graphql", &["graphql", "gql"]),
    ("grpc", &["grpc", "protobuf"]),
    ("rest", &["rest", "restful", "api", "apis"]),
    // infra
    (
        "linux",
        &["linux", "gnu-linux", "arch", "debian", "ubuntu", "fedora"],
    ),
    ("docker", &["docker", "containers", "podman"]),
    ("kubernetes", &["kubernetes", "k8s", "helm"]),
    ("terraform", &["terraform", "opentofu", "iac"]),
    ("ansible", &["ansible"]),
    ("aws", &["aws", "amazon-web-services"]),
    ("gcp", &["gcp", "google-cloud"]),
    ("azure", &["azure"]),
    ("cloudflare", &["cloudflare", "workers"]),
    ("nginx", &["nginx"]),
    ("git", &["git", "github", "gitlab"]),
    ("ci", &["ci", "cicd", "ci-cd", "github-actions", "jenkins"]),
    (
        "observability",
        &[
            "observability",
            "prometheus",
            "grafana",
            "otel",
            "opentelemetry",
        ],
    ),
    ("networking", &["networking", "network", "tcp", "bgp"]),
    ("datacenter", &["datacenter", "data-center", "hpc"]),
    ("operations", &["operations", "ops", "sre", "sysadmin"]),
    (
        "security",
        &["security", "cybersecurity", "infosec", "appsec", "pentest"],
    ),
    ("embedded", &["embedded", "firmware", "rtos", "arduino"]),
    // practice
    ("backend", &["backend", "back-end", "server-side"]),
    ("frontend", &["frontend", "front-end", "web"]),
    ("fullstack", &["fullstack", "full-stack"]),
    ("mobile", &["mobile"]),
    ("devops", &["devops", "platform"]),
    (
        "data",
        &[
            "data",
            "data-engineering",
            "etl",
            "pandas",
            "pandasmatplotlib",
        ],
    ),
    (
        "ml",
        &[
            "ml",
            "machine-learning",
            "ai",
            "deep-learning",
            "pytorch",
            "tensorflow",
            "llm",
        ],
    ),
    ("games", &["games", "gamedev", "game-dev", "game"]),
    ("design", &["design", "ux", "ui", "figma"]),
    (
        "writing",
        &["writing", "docs", "technical-writing", "editing"],
    ),
    ("product", &["product", "pm"]),
    (
        "management",
        &["management", "lead", "engineering-manager", "cto"],
    ),
    ("testing", &["testing", "qa", "test-automation"]),
    (
        "distributed",
        &["distributed", "distributed-systems", "consensus"],
    ),
    ("compilers", &["compilers", "compiler", "llvm", "parsers"]),
    ("crypto", &["crypto", "cryptography", "blockchain", "web3"]),
    ("audio", &["audio", "dsp", "music"]),
    (
        "graphics",
        &["graphics", "opengl", "vulkan", "webgpu", "shaders"],
    ),
    ("vim", &["vim", "neovim", "nvim"]),
    ("emacs", &["emacs"]),
];

/// The canonical tag a typed skill folds onto, or none when the vocabulary
/// does not know it.
pub(crate) fn canonical(skill: &str) -> Option<&'static str> {
    let key = skill.trim().trim_start_matches('#').to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    VOCAB
        .iter()
        .find(|(_, aliases)| aliases.iter().any(|alias| *alias == key))
        .map(|(tag, _)| *tag)
}

/// Typed skills split into what the vocabulary knows and what it does not.
/// Canonical tags are deduplicated in first-seen order; the free tags keep
/// the person's spelling, lowercased, so the row can show them dim. Both
/// lists together never exceed `limit`, the column's cap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Normalized {
    pub(crate) tags: Vec<String>,
    pub(crate) free: Vec<String>,
}

pub(crate) fn normalize(skills: &[String], limit: usize) -> Normalized {
    let mut out = Normalized::default();
    for skill in skills {
        if out.tags.len() + out.free.len() >= limit {
            break;
        }
        match canonical(skill) {
            Some(tag) => {
                if !out.tags.iter().any(|known| known == tag) {
                    out.tags.push(tag.to_string());
                }
            }
            None => {
                let free = skill.trim().trim_start_matches('#').to_ascii_lowercase();
                if !free.is_empty() && !out.free.iter().any(|known| *known == free) {
                    out.free.push(free);
                }
            }
        }
    }
    out
}

/// Every tag the vocabulary knows, for the job press's extraction schema.
#[cfg(test)]
pub(crate) fn all_tags() -> impl Iterator<Item = &'static str> {
    VOCAB.iter().map(|(tag, _)| *tag)
}
