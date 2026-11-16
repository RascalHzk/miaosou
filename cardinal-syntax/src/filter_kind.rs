//! `FilterKind`：Everything 风格过滤器名与其语义分类。

/// Strongly-typed view over Everything's built-in filters. Custom macros fall
/// back to [`FilterKind::Custom`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterKind {
    /// Only match files (`file:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("file:report").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::File));
    /// ```
    File,
    /// Only match folders (`folder:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("folder:Projects").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Folder));
    /// ```
    Folder,
    /// Extension filter (`ext:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("ext:txt").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Ext));
    /// ```
    Ext,
    /// File type categories (`type:` such as `type:picture`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("type:picture").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Type));
    /// ```
    Type,
    /// Audio macro (`audio:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("audio:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Audio));
    /// ```
    Audio,
    /// Video macro (`video:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("video:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Video));
    /// ```
    Video,
    /// Document macro (`doc:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("doc:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Doc));
    /// ```
    Doc,
    /// Executable macro (`exe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("exe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Exe));
    /// ```
    Exe,
    /// Size comparisons or ranges (`size:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("size:>1GB").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Size));
    /// ```
    Size,
    /// Date modified (`dm:` / `datemodified:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dm:today").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::DateModified));
    /// ```
    DateModified,
    /// Date created (`dc:` / `datecreated:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dc:thisweek").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::DateCreated));
    /// ```
    DateCreated,
    /// Date accessed (`da:` / `dateaccessed:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("da:yesterday").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::DateAccessed));
    /// ```
    DateAccessed,
    /// Date run (`dr:` / `daterun:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dr:today").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::DateRun));
    /// ```
    DateRun,
    /// Restrict to direct children of a folder (`parent:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("parent:/Users").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Parent));
    /// ```
    Parent,
    /// Restrict to descendants of a folder (`infolder:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("infolder:/Users/demo").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::InFolder));
    /// ```
    InFolder,
    /// Limit to the folder itself (`nosubfolders:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("nosubfolders:/Users/demo").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::NoSubfolders));
    /// ```
    NoSubfolders,
    /// Require a folder containing matching children (`child:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("child:*.mp3").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Child));
    /// ```
    Child,
    /// Match file-system attributes (`attrib:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("attrib:H").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Attribute));
    /// ```
    Attribute,
    /// Attribute duplicate detection (`attribdupe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("attribdupe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::AttributeDuplicate));
    /// ```
    AttributeDuplicate,
    /// Date-modified duplicate detection (`dmdupe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dmdupe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::DateModifiedDuplicate));
    /// ```
    DateModifiedDuplicate,
    /// Name duplicate detection (`dupe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dupe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Duplicate));
    /// ```
    Duplicate,
    /// Duplicate detection ignoring extensions (`namepartdupe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("namepartdupe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::NamePartDuplicate));
    /// ```
    NamePartDuplicate,
    /// Duplicate detection by size (`sizedupe:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("sizedupe:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::SizeDuplicate));
    /// ```
    SizeDuplicate,
    /// Audio metadata—artist (`artist:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("artist:Daft").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Artist));
    /// ```
    Artist,
    /// Audio metadata—album (`album:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("album:Discovery").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Album));
    /// ```
    Album,
    /// Audio metadata—title (`title:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("title:OneMoreTime").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Title));
    /// ```
    Title,
    /// Audio metadata—genre (`genre:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("genre:house").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Genre));
    /// ```
    Genre,
    /// Audio metadata—year (`year:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("year:2024").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Year));
    /// ```
    Year,
    /// Audio metadata—track number (`track:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("track:01").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Track));
    /// ```
    Track,
    /// Audio metadata—comment (`comment:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("comment:live").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Comment));
    /// ```
    Comment,
    /// Image width comparisons (`width:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("width:>4000").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Width));
    /// ```
    Width,
    /// Image height comparisons (`height:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("height:<=2000").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Height));
    /// ```
    Height,
    /// Combined dimensions (`dimensions:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("dimensions:1920x1080").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Dimensions));
    /// ```
    Dimensions,
    /// Orientation filter (`orientation:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("orientation:horizontal").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Orientation));
    /// ```
    Orientation,
    /// Bit depth filter (`bitdepth:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("bitdepth:24").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::BitDepth));
    /// ```
    BitDepth,
    /// Case-sensitive search toggle (`case:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("case:ABC").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::CaseSensitive));
    /// ```
    CaseSensitive,
    /// Finder tag filter (`tag:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("tag:Project").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Tag));
    /// ```
    Tag,
    /// Content search (`content:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("content:error").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Content));
    /// ```
    Content,
    /// Temporarily disable whole filename matching (`nowholefilename:`).
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("nowholefilename:report").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::NoWholeFilename));
    /// ```
    NoWholeFilename,
    /// User-defined macro or unrecognized filter name.
    /// ```
    /// use cardinal_syntax::{parse_query, Expr, Term, FilterKind};
    /// let Expr::Term(Term::Filter(filter)) = parse_query("proj:").unwrap().expr else { panic!() };
    /// assert!(matches!(filter.kind, FilterKind::Custom(name) if name == "proj"));
    /// ```
    Custom(String),
}

impl FilterKind {
    pub(super) fn from_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "file" => FilterKind::File,
            "folder" => FilterKind::Folder,
            "ext" => FilterKind::Ext,
            "type" => FilterKind::Type,
            "audio" => FilterKind::Audio,
            "video" => FilterKind::Video,
            "doc" => FilterKind::Doc,
            "exe" => FilterKind::Exe,
            "size" => FilterKind::Size,
            "dm" | "datemodified" => FilterKind::DateModified,
            "dc" | "datecreated" => FilterKind::DateCreated,
            "da" | "dateaccessed" => FilterKind::DateAccessed,
            "dr" | "daterun" => FilterKind::DateRun,
            "parent" => FilterKind::Parent,
            "infolder" | "in" => FilterKind::InFolder,
            "nosubfolders" => FilterKind::NoSubfolders,
            "child" => FilterKind::Child,
            "attrib" => FilterKind::Attribute,
            "attribdupe" => FilterKind::AttributeDuplicate,
            "dmdupe" => FilterKind::DateModifiedDuplicate,
            "dupe" => FilterKind::Duplicate,
            "namepartdupe" => FilterKind::NamePartDuplicate,
            "sizedupe" => FilterKind::SizeDuplicate,
            "artist" => FilterKind::Artist,
            "album" => FilterKind::Album,
            "title" => FilterKind::Title,
            "genre" => FilterKind::Genre,
            "year" => FilterKind::Year,
            "track" => FilterKind::Track,
            "comment" => FilterKind::Comment,
            "width" => FilterKind::Width,
            "height" => FilterKind::Height,
            "dimensions" => FilterKind::Dimensions,
            "orientation" => FilterKind::Orientation,
            "bitdepth" => FilterKind::BitDepth,
            "case" => FilterKind::CaseSensitive,
            "tag" | "t" => FilterKind::Tag,
            "content" => FilterKind::Content,
            "nowholefilename" => FilterKind::NoWholeFilename,
            _ => FilterKind::Custom(name.to_string()),
        }
    }
}
