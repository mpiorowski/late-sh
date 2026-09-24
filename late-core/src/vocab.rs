//! The tag vocabulary: one closed list of canonical tags, each in a group,
//! each with the spellings people actually type. Every reader folds onto
//! it: the tag picker offers it, a work card's skills and a profile's langs
//! hold nothing else, and the job press turns a posting's stack into the
//! same tags, so a match is a plain array overlap with no fuzzy comparison
//! at query time.

/// The most tags a card's skills or a profile's langs hold (the `skills`
/// and `skills_tags` columns cap at 12, migrations 041 and 192).
pub const TAG_LIMIT: usize = 12;

/// Where a tag sits in the picker. Langs are the `Language` group alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Language,
    Framework,
    Data,
    Infra,
    Practice,
    Tool,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::Language,
        Group::Framework,
        Group::Data,
        Group::Infra,
        Group::Practice,
        Group::Tool,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Group::Language => "languages",
            Group::Framework => "frameworks and runtimes",
            Group::Data => "data",
            Group::Infra => "infra",
            Group::Practice => "practices",
            Group::Tool => "tools",
        }
    }
}

/// A canonical tag, its group, and every alias that folds onto it. Aliases
/// are matched after lowercasing and trimming a leading `#`; the canonical
/// spelling is itself an alias, so the table reads as one row per tag.
pub const VOCAB: &[(&str, Group, &[&str])] = &[
    ("rust", Group::Language, &["rust", "rustlang"]),
    ("go", Group::Language, &["go", "golang"]),
    ("elixir", Group::Language, &["elixir", "ex"]),
    ("erlang", Group::Language, &["erlang", "beam"]),
    ("python", Group::Language, &["python", "py", "python3"]),
    ("typescript", Group::Language, &["typescript", "ts"]),
    (
        "javascript",
        Group::Language,
        &["javascript", "js", "es6", "ecmascript"],
    ),
    ("c", Group::Language, &["c", "clang", "ansi-c"]),
    ("cpp", Group::Language, &["cpp", "c++", "cplusplus"]),
    (
        "csharp",
        Group::Language,
        &["csharp", "c#", "dotnet", ".net"],
    ),
    ("java", Group::Language, &["java", "jvm"]),
    ("kotlin", Group::Language, &["kotlin", "kt"]),
    ("swift", Group::Language, &["swift"]),
    ("ruby", Group::Language, &["ruby", "rb"]),
    ("php", Group::Language, &["php"]),
    ("scala", Group::Language, &["scala"]),
    ("clojure", Group::Language, &["clojure", "clj"]),
    ("haskell", Group::Language, &["haskell", "hs"]),
    ("ocaml", Group::Language, &["ocaml"]),
    ("zig", Group::Language, &["zig"]),
    ("nim", Group::Language, &["nim"]),
    ("lua", Group::Language, &["lua"]),
    ("perl", Group::Language, &["perl"]),
    ("dart", Group::Language, &["dart"]),
    ("r", Group::Language, &["r", "rlang"]),
    ("julia", Group::Language, &["julia"]),
    ("sql", Group::Language, &["sql"]),
    (
        "bash",
        Group::Language,
        &["bash", "shell", "sh", "zsh", "fish", "scripting"],
    ),
    ("powershell", Group::Language, &["powershell", "pwsh"]),
    ("nix", Group::Language, &["nix", "nixos"]),
    ("wasm", Group::Language, &["wasm", "webassembly"]),
    ("solidity", Group::Language, &["solidity"]),
    ("assembly", Group::Language, &["assembly", "asm"]),
    ("react", Group::Framework, &["react", "reactjs", "react.js"]),
    ("nextjs", Group::Framework, &["nextjs", "next", "next.js"]),
    ("vue", Group::Framework, &["vue", "vuejs", "vue.js", "nuxt"]),
    ("svelte", Group::Framework, &["svelte", "sveltekit"]),
    ("angular", Group::Framework, &["angular", "angularjs"]),
    ("node", Group::Framework, &["node", "nodejs", "node.js"]),
    ("deno", Group::Framework, &["deno"]),
    ("bun", Group::Framework, &["bun"]),
    ("django", Group::Framework, &["django"]),
    ("flask", Group::Framework, &["flask"]),
    ("fastapi", Group::Framework, &["fastapi"]),
    (
        "rails",
        Group::Framework,
        &["rails", "ruby-on-rails", "ror"],
    ),
    ("phoenix", Group::Framework, &["phoenix", "liveview"]),
    (
        "spring",
        Group::Framework,
        &["spring", "springboot", "spring-boot"],
    ),
    ("laravel", Group::Framework, &["laravel"]),
    ("axum", Group::Framework, &["axum"]),
    ("actix", Group::Framework, &["actix"]),
    ("tokio", Group::Framework, &["tokio"]),
    ("tauri", Group::Framework, &["tauri"]),
    ("electron", Group::Framework, &["electron"]),
    ("flutter", Group::Framework, &["flutter"]),
    (
        "react-native",
        Group::Framework,
        &["react-native", "reactnative", "rn"],
    ),
    ("ios", Group::Framework, &["ios", "swiftui", "uikit"]),
    ("android", Group::Framework, &["android"]),
    ("unity", Group::Framework, &["unity", "unity3d"]),
    ("unreal", Group::Framework, &["unreal", "ue4", "ue5"]),
    ("godot", Group::Framework, &["godot"]),
    ("bevy", Group::Framework, &["bevy"]),
    (
        "tui",
        Group::Framework,
        &["tui", "ratatui", "ncurses", "curses"],
    ),
    ("cli", Group::Framework, &["cli", "command-line"]),
    (
        "postgres",
        Group::Data,
        &["postgres", "postgresql", "pg", "psql"],
    ),
    ("mysql", Group::Data, &["mysql", "mariadb"]),
    ("sqlite", Group::Data, &["sqlite"]),
    ("redis", Group::Data, &["redis", "valkey"]),
    ("mongodb", Group::Data, &["mongodb", "mongo"]),
    (
        "elasticsearch",
        Group::Data,
        &["elasticsearch", "opensearch"],
    ),
    ("kafka", Group::Data, &["kafka"]),
    ("rabbitmq", Group::Data, &["rabbitmq", "amqp"]),
    ("clickhouse", Group::Data, &["clickhouse"]),
    ("graphql", Group::Data, &["graphql", "gql"]),
    ("grpc", Group::Data, &["grpc", "protobuf"]),
    ("rest", Group::Data, &["rest", "restful"]),
    (
        "linux",
        Group::Infra,
        &["linux", "gnu-linux", "arch", "debian", "ubuntu", "fedora"],
    ),
    ("docker", Group::Infra, &["docker", "containers", "podman"]),
    ("kubernetes", Group::Infra, &["kubernetes", "k8s", "helm"]),
    ("terraform", Group::Infra, &["terraform", "opentofu", "iac"]),
    ("ansible", Group::Infra, &["ansible"]),
    ("aws", Group::Infra, &["aws", "amazon-web-services"]),
    ("gcp", Group::Infra, &["gcp", "google-cloud"]),
    ("azure", Group::Infra, &["azure"]),
    ("cloudflare", Group::Infra, &["cloudflare", "workers"]),
    ("nginx", Group::Infra, &["nginx"]),
    ("git", Group::Infra, &["git", "github", "gitlab"]),
    (
        "ci",
        Group::Infra,
        &["ci", "cicd", "ci-cd", "github-actions", "jenkins"],
    ),
    (
        "observability",
        Group::Infra,
        &[
            "observability",
            "prometheus",
            "grafana",
            "otel",
            "opentelemetry",
        ],
    ),
    (
        "networking",
        Group::Infra,
        &["networking", "network", "tcp", "bgp"],
    ),
    (
        "datacenter",
        Group::Infra,
        &["datacenter", "data-center", "hpc"],
    ),
    (
        "operations",
        Group::Infra,
        &["operations", "ops", "sre", "sysadmin"],
    ),
    (
        "security",
        Group::Infra,
        &["security", "cybersecurity", "infosec", "appsec", "pentest"],
    ),
    (
        "embedded",
        Group::Infra,
        &["embedded", "firmware", "rtos", "arduino"],
    ),
    (
        "backend",
        Group::Practice,
        &["backend", "back-end", "server-side"],
    ),
    ("frontend", Group::Practice, &["frontend", "front-end"]),
    ("fullstack", Group::Practice, &["fullstack", "full-stack"]),
    ("mobile", Group::Practice, &["mobile"]),
    ("devops", Group::Practice, &["devops", "platform"]),
    (
        "data",
        Group::Practice,
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
        Group::Practice,
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
    (
        "games",
        Group::Practice,
        &["games", "gamedev", "game-dev", "game"],
    ),
    ("design", Group::Practice, &["design", "ux", "ui", "figma"]),
    (
        "writing",
        Group::Practice,
        &["writing", "docs", "technical-writing", "editing"],
    ),
    ("product", Group::Practice, &["product", "pm"]),
    (
        "management",
        Group::Practice,
        &["management", "lead", "engineering-manager", "cto"],
    ),
    (
        "testing",
        Group::Practice,
        &["testing", "qa", "test-automation"],
    ),
    (
        "distributed",
        Group::Practice,
        &["distributed", "distributed-systems", "consensus"],
    ),
    (
        "compilers",
        Group::Practice,
        &["compilers", "compiler", "llvm", "parsers"],
    ),
    (
        "crypto",
        Group::Practice,
        &["crypto", "cryptography", "blockchain", "web3"],
    ),
    ("audio", Group::Practice, &["audio", "dsp", "music"]),
    (
        "graphics",
        Group::Practice,
        &["graphics", "opengl", "vulkan", "webgpu", "shaders"],
    ),
    ("vim", Group::Tool, &["vim", "neovim", "nvim"]),
    ("emacs", Group::Tool, &["emacs"]),
];

/// The canonical tag a typed skill folds onto, or none when the vocabulary
/// does not know it.
pub fn canonical(skill: &str) -> Option<&'static str> {
    let key = skill.trim().trim_start_matches('#').to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    VOCAB
        .iter()
        .find(|(_, _, aliases)| aliases.iter().any(|alias| *alias == key))
        .map(|(tag, _, _)| *tag)
}

/// The group a canonical tag sits in, or none for a word outside the list.
pub fn group_of(tag: &str) -> Option<Group> {
    VOCAB
        .iter()
        .find(|(known, _, _)| *known == tag)
        .map(|(_, group, _)| *group)
}

/// The aliases of a canonical tag, the canonical spelling first; empty for
/// a word outside the list.
pub fn aliases_of(tag: &str) -> &'static [&'static str] {
    VOCAB
        .iter()
        .find(|(known, _, _)| *known == tag)
        .map(|(_, _, aliases)| *aliases)
        .unwrap_or(&[])
}

/// Typed skills split into what the vocabulary knows and what it does not.
/// Canonical tags are deduplicated in first-seen order; the free tags keep
/// the person's spelling, lowercased, so a caller can show them dim. Both
/// lists together never exceed `limit`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Normalized {
    pub tags: Vec<String>,
    pub free: Vec<String>,
}

pub fn normalize(skills: &[String], limit: usize) -> Normalized {
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
                if !free.is_empty() && !out.free.contains(&free) {
                    out.free.push(free);
                }
            }
        }
    }
    out
}

/// A profile's langs as stored: each value split on commas and whitespace,
/// folded onto the vocabulary, kept only when it is a language, at most
/// `TAG_LIMIT`, in first-seen order. Anything the list does not know is
/// dropped, so a row read back holds canonical language tags and nothing
/// else.
pub fn normalize_langs<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for value in values {
        for word in value.split(|c: char| c == ',' || c.is_whitespace()) {
            let Some(tag) = canonical(word) else {
                continue;
            };
            if group_of(tag) != Some(Group::Language) || out.iter().any(|known| known == tag) {
                continue;
            }
            out.push(tag.to_string());
            if out.len() >= TAG_LIMIT {
                return out;
            }
        }
    }
    out
}

/// Every tag the vocabulary knows, in the list's order, for the job press's
/// prompt and the picker.
pub fn all_tags() -> impl Iterator<Item = &'static str> {
    VOCAB.iter().map(|(tag, _, _)| *tag)
}

/// The tags of one group, in the list's order.
pub fn tags_in(group: Group) -> impl Iterator<Item = &'static str> {
    VOCAB
        .iter()
        .filter(move |(_, known, _)| *known == group)
        .map(|(tag, _, _)| *tag)
}
