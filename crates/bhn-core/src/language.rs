//! Maps a file name to a language id.
//!
//! The id is a stable string the UI resolves to a CodeMirror extension. The
//! core deliberately does not know what CodeMirror is — keeping the mapping
//! here means the Windows and Linux shells get identical language detection
//! for free, and a shell that cannot highlight a language can fall back to
//! plain text without the core caring.
//!
//! Unknown extensions map to `plain`, never to a guess.

/// Language id used when nothing matches, and for untitled scratch buffers.
pub const PLAIN: &str = "plain";

/// Extension (lowercase, no dot) -> language id.
///
/// Several extensions intentionally share an id: the UI has one highlighter
/// for each id, and e.g. `.h` is far more often C/C++ than Objective-C.
#[rustfmt::skip]
const BY_EXTENSION: &[(&str, &str)] = &[
	// Web
	("js", "javascript"), ("mjs", "javascript"), ("cjs", "javascript"),
	("jsx", "jsx"), ("ts", "typescript"), ("mts", "typescript"), ("cts", "typescript"),
	("tsx", "tsx"), ("json", "json"), ("jsonc", "json"), ("json5", "json"),
	("html", "html"), ("htm", "html"), ("xhtml", "html"), ("vue", "html"),
	("svelte", "html"), ("css", "css"), ("scss", "scss"), ("sass", "scss"),
	("less", "less"),
	// Systems
	("c", "c"), ("h", "c"),
	("cpp", "cpp"), ("cxx", "cpp"), ("cc", "cpp"), ("hpp", "cpp"), ("hxx", "cpp"),
	("hh", "cpp"), ("ino", "cpp"),
	("rs", "rust"), ("go", "go"), ("zig", "cpp"),
	("m", "cpp"), ("mm", "cpp"),
	("swift", "swift"), ("java", "java"), ("kt", "java"), ("kts", "java"),
	("scala", "java"), ("groovy", "java"), ("cs", "csharp"),
	// Scripting
	("py", "python"), ("pyw", "python"), ("pyi", "python"),
	("rb", "ruby"), ("gemspec", "ruby"), ("rake", "ruby"),
	("pl", "perl"), ("pm", "perl"), ("t", "perl"),
	("php", "php"), ("phtml", "php"), ("php3", "php"), ("php4", "php"), ("php5", "php"),
	("lua", "lua"), ("tcl", "tcl"), ("r", "r"), ("jl", "python"),
	("sh", "shell"), ("bash", "shell"), ("zsh", "shell"), ("ksh", "shell"),
	("fish", "shell"), ("command", "shell"),
	("ps1", "powershell"), ("psm1", "powershell"), ("psd1", "powershell"),
	("bat", "batch"), ("cmd", "batch"),
	("vb", "vb"), ("vbs", "vb"), ("bas", "vb"),
	("dart", "java"), ("hs", "haskell"), ("clj", "clojure"), ("cljs", "clojure"),
	("erl", "erlang"), ("hrl", "erlang"), ("ex", "erlang"), ("exs", "erlang"),
	// Data / config
	("yaml", "yaml"), ("yml", "yaml"),
	("toml", "toml"),
	("ini", "properties"), ("cfg", "properties"), ("conf", "properties"),
	("properties", "properties"), ("env", "properties"), ("desktop", "properties"),
	("xml", "xml"), ("xsl", "xml"), ("xslt", "xml"), ("xsd", "xml"), ("plist", "xml"),
	("svg", "xml"), ("rss", "xml"), ("atom", "xml"), ("wsdl", "xml"), ("pom", "xml"),
	("csproj", "xml"), ("resx", "xml"), ("storyboard", "xml"), ("xib", "xml"),
	("sql", "sql"), ("psql", "sql"), ("mysql", "sql"),
	("csv", "plain"), ("tsv", "plain"),
	// Prose / docs
	("md", "markdown"), ("markdown", "markdown"), ("mdx", "markdown"),
	("txt", PLAIN), ("text", PLAIN), ("log", PLAIN), ("nfo", PLAIN),
	("tex", "stex"), ("sty", "stex"), ("cls", "stex"),
	// Misc
	("diff", "diff"), ("patch", "diff"),
	("dockerfile", "dockerfile"),
	("cmake", "cmake"), ("mk", "makefile"), ("mak", "makefile"),
	("gradle", "java"), ("graphql", "graphql"), ("gql", "graphql"),
	("proto", "protobuf"), ("asm", "asm"), ("s", "asm"),
	("v", "verilog"), ("sv", "verilog"), ("vhd", "vhdl"), ("vhdl", "vhdl"),
	("htaccess", "properties"), ("nginx", "nginx"),
];

/// Whole file names with no useful extension. Matched case-insensitively.
#[rustfmt::skip]
const BY_FILENAME: &[(&str, &str)] = &[
	("makefile", "makefile"),
	("gnumakefile", "makefile"),
	("dockerfile", "dockerfile"),
	("containerfile", "dockerfile"),
	("cmakelists.txt", "cmake"),
	("rakefile", "ruby"),
	("gemfile", "ruby"),
	("vagrantfile", "ruby"),
	("podfile", "ruby"),
	("brewfile", "ruby"),
	("cargo.lock", "toml"),
	("go.mod", "plain"),
	("go.sum", "plain"),
	(".gitignore", "properties"),
	(".gitattributes", "properties"),
	(".gitconfig", "properties"),
	(".npmrc", "properties"),
	(".editorconfig", "properties"),
	(".env", "properties"),
	(".zshrc", "shell"),
	(".bashrc", "shell"),
	(".bash_profile", "shell"),
	(".profile", "shell"),
	(".zprofile", "shell"),
	(".htaccess", "properties"),
	("nginx.conf", "nginx"),
];

/// Returns the language id for a file name or full path.
///
/// Accepts either, so callers do not have to split a path first. Matching is
/// case-insensitive, because `.PY` and `Makefile` both occur in the wild.
pub fn detect(name_or_path: &str) -> &'static str {
    let name = base_name(name_or_path).to_ascii_lowercase();
    if name.is_empty() {
        return PLAIN;
    }

    for (candidate, id) in BY_FILENAME {
        if name == *candidate {
            return id;
        }
    }

    // A dotfile with no further dot (".profile") has no extension — its whole
    // name is the identifier, and that was handled above. Starting the search
    // at index 1 keeps `rfind` from treating the leading dot as a separator.
    if let Some(dot) = name[1..].rfind('.') {
        let ext = &name[dot + 2..];
        for (candidate, id) in BY_EXTENSION {
            if ext == *candidate {
                return id;
            }
        }
        // `foo.conf.bak`, `app.log.1` — a backup or rotation suffix hides the
        // real extension, so try the one before it rather than giving up.
        let stem = &name[..dot + 1];
        if let Some(prev) = stem[1..].rfind('.') {
            let ext2 = &stem[prev + 2..];
            for (candidate, id) in BY_EXTENSION {
                if ext2 == *candidate {
                    return id;
                }
            }
        }
    }

    PLAIN
}

/// Final path component, handling both separators so a Windows path parses
/// correctly on a Unix build and vice versa.
pub fn base_name(path: &str) -> &str {
    match path.rfind(['/', '\\']) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension_case_insensitively() {
        assert_eq!(detect("main.rs"), "rust");
        assert_eq!(detect("SCRIPT.PY"), "python");
        assert_eq!(detect("index.HTML"), "html");
    }

    #[test]
    fn accepts_full_paths_on_either_platform() {
        assert_eq!(detect("/Users/benjamin/a/b/notes.md"), "markdown");
        assert_eq!(detect(r"C:\Users\benjamin\app\main.cpp"), "cpp");
    }

    #[test]
    fn matches_extensionless_names() {
        assert_eq!(detect("Makefile"), "makefile");
        assert_eq!(detect("/src/Dockerfile"), "dockerfile");
        assert_eq!(detect(".zshrc"), "shell");
    }

    #[test]
    fn a_dotfile_is_not_treated_as_an_extension() {
        // ".env" must match the file-name table, not be read as extension "env"
        // of an empty stem — both give "properties" here, so assert the one
        // that would break: a dotfile whose name is not in either table must
        // fall back to plain rather than panicking on the slice.
        assert_eq!(detect(".somethingunknown"), PLAIN);
        assert_eq!(detect("."), PLAIN);
        assert_eq!(detect(".."), PLAIN);
    }

    #[test]
    fn looks_past_a_backup_suffix() {
        assert_eq!(detect("nginx.conf.bak"), "properties");
        assert_eq!(detect("server.log.1"), PLAIN);
        assert_eq!(detect("schema.sql.orig"), "sql");
    }

    #[test]
    fn unknown_extensions_are_plain_not_guessed() {
        assert_eq!(detect("archive.qqq"), PLAIN);
        assert_eq!(detect("noextension"), PLAIN);
        assert_eq!(detect(""), PLAIN);
    }

    #[test]
    fn base_name_handles_both_separators() {
        assert_eq!(base_name("/a/b/c.txt"), "c.txt");
        assert_eq!(base_name(r"C:\a\b\c.txt"), "c.txt");
        assert_eq!(base_name("c.txt"), "c.txt");
    }
}
