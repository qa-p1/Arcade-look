//! File type detection: extension + well-known names + magic bytes + content sniffing.
//! Detection never fails: anything unrecognised becomes `Kind::Binary` (hex view) or
//! `Kind::Text` when the content looks like text.

use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Folder,
    Image,
    ImageDecode,
    ImageRaw,
    ImagePsd,
    Svg,
    Video,
    Audio,
    Pdf,
    Markdown,
    Code,
    Text,
    Json,
    Notebook,
    Csv,
    Spreadsheet,
    Document,
    Presentation,
    Epub,
    Archive,
    Font,
    Model,
    Html,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub kind: Kind,
    /// Sub-format, e.g. "docx", "tar.gz", "glb".
    pub format: String,
    /// highlight.js language for code-like kinds.
    pub lang: Option<String>,
    pub mime: String,
}

pub const SNIFF_LEN: usize = 8192;

/// Lower-cased "full" extension, honouring compound archive extensions (tar.gz, …).
pub fn extension_of(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    for compound in ["tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.zstd", "tar.lzma"] {
        if lower.ends_with(&format!(".{compound}")) {
            return compound.to_string();
        }
    }
    match lower.rfind('.') {
        Some(i) if i > 0 || lower.len() > 1 => lower[i + 1..].to_string(),
        _ => String::new(),
    }
}

pub fn detect(path: &Path) -> Detection {
    if path.is_dir() {
        return det(Kind::Folder, "folder", None, "inode/directory");
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = extension_of(&name);

    let mut head = Vec::with_capacity(SNIFF_LEN);
    if let Ok(f) = File::open(path) {
        let _ = f.take(SNIFF_LEN as u64).read_to_end(&mut head);
    }
    detect_from(&name, &ext, &head)
}

pub fn detect_from(name: &str, ext: &str, head: &[u8]) -> Detection {
    // MPEG transport streams share ".ts" with TypeScript: check for sync bytes.
    if (ext == "ts" || ext == "mts" || ext == "m2ts")
        && head.len() > 376
        && head[0] == 0x47
        && head[188] == 0x47
    {
        return det(Kind::Video, ext, None, "video/mp2t");
    }
    if let Some(d) = by_extension(ext) {
        return d;
    }
    if let Some(d) = by_filename(name) {
        return d;
    }
    if let Some(lang) = shebang_lang(head) {
        return det(Kind::Code, "script", Some(lang), "text/plain");
    }
    if let Some(d) = by_magic(head) {
        return d;
    }
    if looks_like_text(head) {
        let trimmed = trim_ascii_start(head);
        if trimmed.starts_with(b"{") || trimmed.starts_with(b"[") {
            // Might be JSON without an extension; the viewer falls back to code if invalid.
            return det(Kind::Json, "json", Some("json"), "application/json");
        }
        if trimmed.starts_with(b"<?xml") {
            return det(Kind::Code, "xml", Some("xml"), "application/xml");
        }
        if starts_with_ci(trimmed, b"<!doctype html") || starts_with_ci(trimmed, b"<html") {
            return det(Kind::Html, "html", Some("xml"), "text/html");
        }
        return det(Kind::Text, "text", Some("plaintext"), "text/plain");
    }
    det(Kind::Binary, "binary", None, "application/octet-stream")
}

fn det(kind: Kind, format: &str, lang: Option<&str>, mime: &str) -> Detection {
    Detection {
        kind,
        format: format.to_string(),
        lang: lang.map(str::to_string),
        mime: mime.to_string(),
    }
}

fn trim_ascii_start(b: &[u8]) -> &[u8] {
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b);
    let start = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    &b[start..]
}

fn starts_with_ci(b: &[u8], prefix: &[u8]) -> bool {
    b.len() >= prefix.len() && b[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn by_extension(ext: &str) -> Option<Detection> {
    use Kind::*;
    let d = match ext {
        // Images the webview can show directly.
        "png" | "apng" => det(Image, ext, None, mime_img(ext)),
        "jpg" | "jpeg" | "jpe" | "jfif" | "pjpeg" | "pjp" => det(Image, "jpeg", None, "image/jpeg"),
        "gif" | "webp" | "bmp" | "dib" | "ico" | "cur" | "avif" | "heic" | "heif" | "jxl" => {
            det(Image, ext, None, mime_img(ext))
        }
        "svg" => det(Svg, "svg", Some("xml"), "image/svg+xml"),
        // Images decoded in Rust.
        "tif" | "tiff" => det(ImageDecode, "tiff", None, "image/tiff"),
        "tga" | "icb" | "vda" | "vst" => det(ImageDecode, "tga", None, "image/x-tga"),
        "dds" => det(ImageDecode, "dds", None, "image/vnd-ms.dds"),
        "hdr" => det(ImageDecode, "hdr", None, "image/vnd.radiance"),
        "exr" => det(ImageDecode, "exr", None, "image/x-exr"),
        "qoi" => det(ImageDecode, "qoi", None, "image/qoi"),
        "pnm" | "pbm" | "pgm" | "ppm" | "pam" => det(ImageDecode, ext, None, "image/x-portable-anymap"),
        "ff" | "farbfeld" => det(ImageDecode, "farbfeld", None, "image/x-farbfeld"),
        "psd" | "psb" => det(ImagePsd, ext, None, "image/vnd.adobe.photoshop"),
        "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "dng" | "orf" | "rw2"
        | "raf" | "pef" | "srw" | "x3f" | "3fr" | "mef" | "mos" | "erf" | "kdc" | "dcr" | "rwl"
        | "iiq" => det(ImageRaw, ext, None, "image/x-raw"),
        // Media.
        "mp4" | "m4v" | "mov" | "qt" | "webm" | "mkv" | "ogv" | "avi" | "wmv" | "flv" | "mpg"
        | "mpeg" | "m2ts" | "mts" | "3gp" | "3g2" | "f4v" | "asf" => {
            det(Video, ext, None, mime_video(ext))
        }
        "mp3" | "m4a" | "m4b" | "aac" | "flac" | "wav" | "wave" | "ogg" | "oga" | "opus"
        | "weba" | "aif" | "aiff" | "aifc" | "caf" | "wma" | "ape" | "wv" | "mka" | "spx" => {
            det(Audio, ext, None, mime_audio(ext))
        }
        // Documents.
        "pdf" | "ai" => det(Pdf, "pdf", None, "application/pdf"),
        "md" | "markdown" | "mdown" | "mkd" | "mkdn" | "mdwn" | "mdx" | "rmd" | "qmd" => {
            det(Markdown, "markdown", Some("markdown"), "text/markdown")
        }
        "json" | "geojson" | "topojson" | "har" | "webmanifest" | "jsonl" | "ndjson" | "map"
        | "jsonc" | "json5" => det(Json, ext, Some("json"), "application/json"),
        "ipynb" => det(Notebook, "ipynb", Some("json"), "application/x-ipynb+json"),
        "csv" => det(Csv, "csv", None, "text/csv"),
        "tsv" | "tab" => det(Csv, "tsv", None, "text/tab-separated-values"),
        "psv" => det(Csv, "psv", None, "text/plain"),
        "xlsx" | "xlsm" | "xlsb" | "xls" | "xla" | "xlam" | "ods" => {
            det(Spreadsheet, ext, None, "application/vnd.ms-excel")
        }
        "docx" | "docm" | "dotx" | "dotm" => det(Document, "docx", None, MIME_DOCX),
        "odt" | "ott" => det(Document, "odt", None, "application/vnd.oasis.opendocument.text"),
        "rtf" => det(Document, "rtf", None, "application/rtf"),
        "epub" => det(Epub, "epub", None, "application/epub+zip"),
        "pptx" | "pptm" | "ppsx" | "potx" => det(Presentation, "pptx", None, MIME_PPTX),
        "odp" | "otp" => det(Presentation, "odp", None, "application/vnd.oasis.opendocument.presentation"),
        // Archives.
        "zip" | "jar" | "war" | "ear" | "apk" | "aab" | "ipa" | "xpi" | "whl" | "nupkg"
        | "vsix" | "cbz" | "kmz" | "sketch" | "aar" | "appx" | "msix" => {
            det(Archive, "zip", None, "application/zip")
        }
        "tar" => det(Archive, "tar", None, "application/x-tar"),
        "tgz" | "tar.gz" => det(Archive, "tar.gz", None, "application/gzip"),
        "tbz" | "tbz2" | "tz2" | "tar.bz2" => det(Archive, "tar.bz2", None, "application/x-bzip2"),
        "txz" | "tar.xz" | "tar.lzma" => det(Archive, "tar.xz", None, "application/x-xz"),
        "tzst" | "tar.zst" | "tar.zstd" => det(Archive, "tar.zst", None, "application/zstd"),
        "gz" | "gzip" | "svgz" => det(Archive, "gz", None, "application/gzip"),
        "bz2" => det(Archive, "bz2", None, "application/x-bzip2"),
        "xz" | "lzma" => det(Archive, "xz", None, "application/x-xz"),
        "zst" | "zstd" => det(Archive, "zst", None, "application/zstd"),
        "7z" | "cb7" => det(Archive, "7z", None, "application/x-7z-compressed"),
        // Fonts.
        "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2" => det(Font, ext, None, mime_font(ext)),
        // 3D.
        "glb" | "gltf" | "obj" | "stl" | "ply" | "fbx" | "dae" | "3mf" => {
            det(Model, ext, None, "model/gltf-binary")
        }
        // Web.
        "html" | "htm" | "xhtml" | "shtml" => det(Html, "html", Some("xml"), "text/html"),
        _ => {
            if let Some(lang) = code_lang(ext) {
                return Some(match lang {
                    "plaintext" => det(Text, ext, Some(lang), "text/plain"),
                    // A language highlight.js doesn't ship: code view without colours.
                    "plaintext-code" => det(Code, ext, Some("plaintext"), "text/plain"),
                    _ => det(Code, ext, Some(lang), "text/plain"),
                });
            }
            return None;
        }
    };
    Some(d)
}

const MIME_DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const MIME_PPTX: &str = "application/vnd.openxmlformats-officedocument.presentationml.presentation";

fn mime_img(ext: &str) -> &'static str {
    match ext {
        "png" | "apng" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" | "dib" => "image/bmp",
        "ico" | "cur" => "image/x-icon",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "jxl" => "image/jxl",
        _ => "image/jpeg",
    }
}

fn mime_video(ext: &str) -> &'static str {
    match ext {
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        "mov" | "qt" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "wmv" | "asf" => "video/x-ms-wmv",
        "flv" | "f4v" => "video/x-flv",
        "mpg" | "mpeg" => "video/mpeg",
        "m2ts" | "mts" | "ts" => "video/mp2t",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        _ => "video/mp4",
    }
}

fn mime_audio(ext: &str) -> &'static str {
    match ext {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "wav" | "wave" => "audio/wav",
        "ogg" | "oga" | "spx" => "audio/ogg",
        "opus" => "audio/opus",
        "weba" => "audio/webm",
        "aif" | "aiff" | "aifc" => "audio/aiff",
        "caf" => "audio/x-caf",
        "wma" => "audio/x-ms-wma",
        "mka" => "audio/x-matroska",
        _ => "application/octet-stream",
    }
}

fn mime_font(ext: &str) -> &'static str {
    match ext {
        "otf" | "otc" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttc" => "font/collection",
        _ => "font/ttf",
    }
}

/// Map a lower-case extension to a highlight.js language id.
pub fn code_lang(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "py" | "pyw" | "pyi" | "pyx" | "gyp" => "python",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ino" | "ipp" | "tpp"
        | "cu" | "cuh" | "hlsl" | "metal" => "cpp",
        "cs" | "csx" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "go" => "go",
        "rb" | "rake" | "gemspec" | "podspec" => "ruby",
        "php" | "phtml" => "php",
        "pl" | "pm" | "t" => "perl",
        "lua" => "lua",
        "sh" | "bash" | "zsh" | "ksh" | "fish" | "command" | "ebuild" => "bash",
        "ps1" | "psm1" | "psd1" => "powershell",
        "bat" | "cmd" => "dos",
        "sql" | "psql" | "mysql" => "sql",
        "r" => "r",
        "scala" | "sc" | "sbt" => "scala",
        "dart" => "dart",
        "hs" | "lhs" | "purs" => "haskell",
        "ex" | "exs" | "heex" => "elixir",
        "erl" | "hrl" => "erlang",
        "clj" | "cljs" | "cljc" | "edn" => "clojure",
        "ml" | "mli" => "ocaml",
        "fs" | "fsx" | "fsi" => "fsharp",
        "vb" | "vbs" => "vbnet",
        "m" | "mm" => "objectivec",
        "groovy" | "gradle" | "gvy" => "groovy",
        "nim" | "nims" => "nim",
        "jl" => "julia",
        "d" => "d",
        "pas" | "pp" | "dpr" => "delphi",
        "f" | "f90" | "f95" | "f03" | "for" => "fortran",
        "asm" | "s" | "nasm" => "x86asm",
        "vue" | "svelte" | "astro" | "ejs" | "xml" | "xsd" | "xsl" | "xslt" | "plist"
        | "csproj" | "vbproj" | "fsproj" | "vcxproj" | "props" | "targets" | "resx" | "rss"
        | "atom" | "kml" | "gpx" | "xaml" | "storyboard" | "xib" | "wsdl" | "dtd" | "nuspec"
        | "rdf" | "opf" | "ncx" | "musicxml" | "glade" | "ui" | "manifest" | "xul" => "xml",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",
        "styl" => "stylus",
        "yaml" | "yml" => "yaml",
        "toml" | "ini" | "cfg" | "conf" | "inf" | "desktop" | "service" | "reg" | "editorconfig"
        | "gitconfig" | "npmrc" | "url" | "lock" => "ini",
        "properties" => "properties",
        "tex" | "sty" | "cls" | "bib" | "ltx" => "latex",
        "dockerfile" | "containerfile" => "dockerfile",
        "mk" | "mak" | "make" => "makefile",
        "cmake" => "cmake",
        "nginx" => "nginx",
        "proto" => "protobuf",
        "graphql" | "gql" => "graphql",
        "diff" | "patch" | "rej" => "diff",
        "http" | "rest" => "http",
        "vim" => "vim",
        "el" | "lisp" | "lsp" | "cl" => "lisp",
        "scm" | "ss" | "rkt" => "scheme",
        "coffee" => "coffeescript",
        "elm" => "elm",
        "cr" => "crystal",
        "wat" | "wast" => "wasm",
        "glsl" | "vert" | "frag" | "geom" | "comp" | "tesc" | "tese" | "vs" | "fsh" | "vsh" => {
            "glsl"
        }
        "awk" => "awk",
        "tcl" => "tcl",
        "applescript" => "applescript",
        "ahk" => "autohotkey",
        "nix" => "nix",
        "twig" => "twig",
        "hbs" | "handlebars" | "mustache" => "handlebars",
        "erb" => "erb",
        "haml" => "haml",
        "adoc" | "asciidoc" => "asciidoc",
        "pde" => "processing",
        "v" | "sv" | "svh" | "vh" => "verilog",
        "vhd" | "vhdl" => "vhdl",
        "prisma" | "tf" | "tfvars" | "hcl" | "sol" | "zig" | "odin" | "gleam" | "mojo" | "jai"
        | "nu" => "plaintext-code",
        "txt" | "text" | "log" | "nfo" | "rst" | "org" | "srt" | "vtt" | "sub" | "ass" | "me"
        | "1st" | "readme" | "license" | "out" | "err" | "pid" | "asc" | "pem" | "crt" | "cer"
        | "csr" | "key" | "pub" | "sig" | "env" | "gitignore" | "gitattributes" | "dockerignore"
        | "npmignore" | "eslintignore" | "prettierignore" | "gitmodules" | "mailmap" | "sln" => {
            "plaintext"
        }
        _ => return None,
    })
}

fn by_filename(name: &str) -> Option<Detection> {
    let lower = name.to_ascii_lowercase();
    let lang = match lower.as_str() {
        "dockerfile" | "containerfile" => "dockerfile",
        "makefile" | "gnumakefile" | "bsdmakefile" => "makefile",
        "cmakelists.txt" => "cmake",
        "gemfile" | "rakefile" | "vagrantfile" | "podfile" | "fastfile" | "brewfile"
        | "guardfile" | "capfile" => "ruby",
        "jenkinsfile" => "groovy",
        "pkgbuild" | "apkbuild" | ".bashrc" | ".bash_profile" | ".bash_logout" | ".profile"
        | ".zshrc" | ".zprofile" | ".zshenv" | ".bash_aliases" | ".xinitrc" | ".xprofile" => {
            "bash"
        }
        "nginx.conf" => "nginx",
        ".vimrc" | ".gvimrc" => "vim",
        "cargo.lock" | "pipfile" | "poetry.lock" | ".gitconfig" | ".editorconfig" | ".npmrc"
        | ".yarnrc" | ".pylintrc" | ".flake8" | "setup.cfg" | "tox.ini" => "ini",
        ".babelrc" | ".eslintrc" | ".prettierrc" | ".swcrc" | "tsconfig.json" => {
            return Some(det(Kind::Json, "json", Some("json"), "application/json"))
        }
        _ => {
            if lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") {
                "dockerfile"
            } else if lower.starts_with(".env") || lower.starts_with("readme")
                || lower.starts_with("license") || lower.starts_with("licence")
                || lower.starts_with("changelog") || lower.starts_with("authors")
                || lower.starts_with("copying") || lower.starts_with("notice")
                || lower.starts_with("contributors") || lower == "todo" || lower == "procfile"
                || lower == "codeowners" || lower.starts_with(".git")
            {
                "plaintext"
            } else {
                return None;
            }
        }
    };
    let kind = if lang == "plaintext" { Kind::Text } else { Kind::Code };
    Some(det(kind, "text", Some(lang), "text/plain"))
}

fn by_magic(head: &[u8]) -> Option<Detection> {
    if head.is_empty() {
        return None;
    }
    if head.starts_with(b"%PDF-") {
        return Some(det(Kind::Pdf, "pdf", None, "application/pdf"));
    }
    if head.starts_with(b"8BPS") {
        return Some(det(Kind::ImagePsd, "psd", None, "image/vnd.adobe.photoshop"));
    }
    if head.len() > 262 && &head[257..262] == b"ustar" {
        return Some(det(Kind::Archive, "tar", None, "application/x-tar"));
    }
    if head.starts_with(b"glTF") {
        return Some(det(Kind::Model, "glb", None, "model/gltf-binary"));
    }
    let t = infer::get(head)?;
    let mime = t.mime_type();
    let ext = t.extension();
    let d = match t.matcher_type() {
        infer::MatcherType::Image => match ext {
            "tif" | "tiff" => det(Kind::ImageDecode, "tiff", None, mime),
            "psd" => det(Kind::ImagePsd, "psd", None, mime),
            "exr" | "hdr" | "dds" | "qoi" | "tga" => det(Kind::ImageDecode, ext, None, mime),
            "cr2" | "nef" | "arw" | "dng" | "orf" | "rw2" | "raf" => {
                det(Kind::ImageRaw, ext, None, mime)
            }
            _ => det(Kind::Image, ext, None, mime),
        },
        infer::MatcherType::Video => det(Kind::Video, ext, None, mime),
        infer::MatcherType::Audio => det(Kind::Audio, ext, None, mime),
        infer::MatcherType::Font => det(Kind::Font, ext, None, mime),
        infer::MatcherType::Archive => match ext {
            "zip" | "jar" => det(Kind::Archive, "zip", None, mime),
            "tar" => det(Kind::Archive, "tar", None, mime),
            "gz" => det(Kind::Archive, "gz", None, mime),
            "bz2" => det(Kind::Archive, "bz2", None, mime),
            "xz" => det(Kind::Archive, "xz", None, mime),
            "zst" => det(Kind::Archive, "zst", None, mime),
            "7z" => det(Kind::Archive, "7z", None, mime),
            "pdf" => det(Kind::Pdf, "pdf", None, mime),
            "epub" => det(Kind::Epub, "epub", None, mime),
            "rtf" => det(Kind::Document, "rtf", None, mime),
            _ => det(Kind::Binary, ext, None, mime),
        },
        infer::MatcherType::Doc => match ext {
            "docx" => det(Kind::Document, "docx", None, mime),
            "xlsx" | "xls" => det(Kind::Spreadsheet, ext, None, mime),
            "pptx" => det(Kind::Presentation, "pptx", None, mime),
            "odt" => det(Kind::Document, "odt", None, mime),
            "ods" => det(Kind::Spreadsheet, "ods", None, mime),
            "odp" => det(Kind::Presentation, "odp", None, mime),
            _ => det(Kind::Binary, ext, None, mime),
        },
        infer::MatcherType::Text => match ext {
            "html" => det(Kind::Html, "html", Some("xml"), mime),
            "xml" => det(Kind::Code, "xml", Some("xml"), mime),
            _ => det(Kind::Text, ext, Some("plaintext"), mime),
        },
        _ => det(Kind::Binary, ext, None, mime),
    };
    Some(d)
}

/// Heuristic: UTF-16 BOM, or no NUL bytes and mostly printable.
pub fn looks_like_text(head: &[u8]) -> bool {
    if head.is_empty() {
        return true;
    }
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return true;
    }
    if head.contains(&0) {
        return false;
    }
    if valid_utf8_prefix(head) {
        return true;
    }
    let control = head
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B))
        .count();
    control * 100 / head.len() < 3
}

/// True when `b` is valid UTF-8, allowing a truncated sequence at the very end.
pub fn valid_utf8_prefix(b: &[u8]) -> bool {
    match std::str::from_utf8(b) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && b.len() - e.valid_up_to() < 4,
    }
}

fn shebang_lang(head: &[u8]) -> Option<&'static str> {
    if !head.starts_with(b"#!") {
        return None;
    }
    let line_end = head.iter().position(|&b| b == b'\n').unwrap_or(head.len().min(200));
    let line = String::from_utf8_lossy(&head[..line_end]).to_ascii_lowercase();
    let table: &[(&str, &str)] = &[
        ("python", "python"),
        ("node", "javascript"),
        ("deno", "typescript"),
        ("bun", "javascript"),
        ("ruby", "ruby"),
        ("perl", "perl"),
        ("php", "php"),
        ("lua", "lua"),
        ("pwsh", "powershell"),
        ("fish", "bash"),
        ("zsh", "bash"),
        ("bash", "bash"),
        ("/sh", "bash"),
        (" sh", "bash"),
        ("dash", "bash"),
        ("awk", "awk"),
        ("tclsh", "tcl"),
    ];
    table.iter().find(|(k, _)| line.contains(k)).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str, head: &[u8]) -> Kind {
        detect_from(name, &extension_of(name), head).kind
    }

    #[test]
    fn extensions() {
        assert_eq!(extension_of("a.TAR.GZ"), "tar.gz");
        assert_eq!(extension_of("photo.JPG"), "jpg");
        assert_eq!(extension_of("Makefile"), "");
        assert_eq!(extension_of(".bashrc"), "bashrc");
    }

    #[test]
    fn kinds() {
        assert_eq!(kind("x.png", b""), Kind::Image);
        assert_eq!(kind("x.rs", b"fn main(){}"), Kind::Code);
        assert_eq!(kind("Dockerfile", b"FROM x"), Kind::Code);
        assert_eq!(kind("notes", b"hello world\n"), Kind::Text);
        assert_eq!(kind("blob", b"\x00\x01\x02\x03"), Kind::Binary);
        assert_eq!(kind("doc", b"%PDF-1.7\n"), Kind::Pdf);
        assert_eq!(kind("run", b"#!/usr/bin/env python3\nprint(1)"), Kind::Code);
        assert_eq!(kind("x.tar.gz", b""), Kind::Archive);
        assert_eq!(kind("data", b"  {\"a\": 1}"), Kind::Json);
        assert_eq!(kind("README", b"# Title"), Kind::Text);
    }

    #[test]
    fn mpeg_ts_vs_typescript() {
        let mut ts = vec![0u8; 400];
        ts[0] = 0x47;
        ts[188] = 0x47;
        assert_eq!(kind("clip.ts", &ts), Kind::Video);
        assert_eq!(kind("index.ts", b"export const a = 1;"), Kind::Code);
    }

    #[test]
    fn utf8_prefix() {
        assert!(valid_utf8_prefix("héllo".as_bytes()));
        let s = "日本".as_bytes();
        assert!(valid_utf8_prefix(&s[..4])); // truncated mid-char
        assert!(!valid_utf8_prefix(&[0xff, 0xfe, 0x41, 0x41, 0x41]));
    }
}
