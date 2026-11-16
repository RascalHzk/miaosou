use crate::{
    SearchCache, SearchOptions, SegmentKind, SegmentMatcher, SegmentMatcherConcrete, SlabIndex,
    SlabNodeMetadataCompact, build_segment_matchers, cache::NAME_POOL,
};
use anyhow::{Result, anyhow, bail};
use cardinal_syntax::{ArgumentKind, Expr, Filter, FilterArgument, FilterKind, Term};
use file_tags::{read_tags_from_path, search_tags_using_mdfind};
use fswalk::NodeFileType;
use hashbrown::HashSet;
use memchr::arch::all::rabinkarp;
use query_segmentation::query_segmentation;
use rayon::iter::{ParallelBridge, ParallelIterator};
use regex::RegexBuilder;
use search_cancel::CancellationToken;
use std::{collections::BTreeSet, fs::File, io::Read, path::Path};

pub(crate) const CONTENT_BUFFER_BYTES: usize = 64 * 1024;

/// Threshold for switching from iterating file metadata to using Spotlight (mdfind).
/// When the base set exceeds this size, Spotlight's indexed search is faster than
/// reading xattr metadata for each file individually.
const TAG_FILTER_MDFIND_THRESHOLD: usize = 10000;

impl SearchCache {
    pub(crate) fn evaluate_expr(
        &mut self,
        expr: &Expr,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        match expr {
            Expr::Empty => Ok(self.nodes_from_base_ref(base, token)),
            Expr::Term(term) => self.evaluate_term(term, base, options, token),
            Expr::Not(inner) => self.evaluate_not(inner, base, options, token),
            Expr::And(parts) => self.evaluate_and(parts, base.cloned(), options, token),
            Expr::Or(parts) => self.evaluate_or(parts, base, options, token),
        }
    }

    fn evaluate_and(
        &mut self,
        parts: &[Expr],
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let mut current: Option<Vec<SlabIndex>> = base;
        for part in parts {
            match part {
                Expr::Not(inner) => {
                    let Some(x) = self.evaluate_not(inner, current.as_ref(), options, token)?
                    else {
                        return Ok(None);
                    };
                    current = Some(x);
                }
                Expr::Term(Term::Filter(filter)) => {
                    let base = current.take();
                    let Some(nodes) = self.evaluate_filter(filter, base, options, token)? else {
                        return Ok(None);
                    };
                    current = Some(nodes);
                }
                _ => {
                    let Some(nodes) = self.evaluate_expr(part, current.as_ref(), options, token)?
                    else {
                        return Ok(None);
                    };
                    current = Some(nodes);
                }
            }
        }
        Ok(Some(current.expect("at least one part in AND expression")))
    }

    fn evaluate_or(
        &mut self,
        parts: &[Expr],
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let mut result: Vec<SlabIndex> = Vec::new();
        let mut seen: HashSet<SlabIndex> = HashSet::new();
        for part in parts {
            let candidate = self.evaluate_expr(part, base, options, token)?;
            let Some(nodes) = candidate else {
                return Ok(None);
            };
            if token.is_cancelled().is_none() {
                return Ok(None);
            }
            for index in nodes {
                if seen.insert(index) {
                    result.push(index);
                }
            }
        }
        Ok(Some(result))
    }

    fn evaluate_not(
        &mut self,
        inner: &Expr,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(mut universe) = self.nodes_from_base_ref(base, token) else {
            return Ok(None);
        };
        if let Some(negated) = self.evaluate_expr(inner, base, options, token)? {
            if difference_in_place(&mut universe, &negated, token).is_none() {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }
        Ok(Some(universe))
    }

    fn evaluate_term(
        &mut self,
        term: &Term,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        if base.is_some_and(Vec::is_empty) {
            return Ok(Some(Vec::new()));
        }

        match term {
            Term::Word(text) => self.evaluate_phrase_with_base(text, base, options, token),
            Term::Regex(pattern) => self.evaluate_regex_with_base(pattern, base, options, token),
            Term::Filter(filter) => self.evaluate_filter(filter, base.cloned(), options, token),
        }
    }

    fn evaluate_phrase_with_base(
        &self,
        text: &str,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(mut nodes) = self.evaluate_phrase(text, options, token)? else {
            return Ok(None);
        };
        if let Some(base) = base
            && intersect_in_place(&mut nodes, base, token).is_none()
        {
            return Ok(None);
        }
        Ok(Some(nodes))
    }

    fn evaluate_phrase(
        &self,
        text: &str,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let segments = query_segmentation(text);
        if segments.is_empty() {
            bail!("Unprocessable term: {text:?}");
        }
        let matchers = build_segment_matchers(&segments, options)
            .map_err(|err| anyhow!("Invalid regex pattern: {err}"))?;
        Ok(self.execute_matchers(&matchers, token))
    }

    fn execute_matchers(
        &self,
        matchers: &[SegmentMatcher],
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        // node_set of matching nodes, sorted by file path
        let mut node_set: Option<Vec<SlabIndex>> = None;

        let mut pending_globstar = false;
        let mut saw_matcher = false;
        let mut saw_globstar = false;
        for matcher in matchers {
            match matcher {
                SegmentMatcher::GlobStar => {
                    saw_globstar = true;
                    pending_globstar = true;
                }
                SegmentMatcher::Star => {
                    saw_matcher = true;
                    let new_node_set = if let Some(nodes) = &node_set {
                        if pending_globstar {
                            self.all_descendant_segments(nodes, token)
                        } else {
                            self.all_direct_children(nodes, token)
                        }
                    } else {
                        self.search_empty(token)
                    }?;
                    node_set = Some(new_node_set);
                    pending_globstar = false;
                }
                SegmentMatcher::Concrete(concrete) => {
                    saw_matcher = true;
                    let new_node_set = if let Some(nodes) = &node_set {
                        if pending_globstar {
                            self.match_descendant_segments(nodes, concrete, token)
                        } else {
                            self.match_direct_child_segments(nodes, concrete, token)
                        }
                    } else {
                        self.match_initial_segment(concrete, token)
                    }?;
                    node_set = Some(new_node_set);
                    pending_globstar = false;
                }
            }
        }

        let mut nodes = if pending_globstar {
            if let Some(nodes) = node_set.take() {
                Some(self.all_descendant_segments(&nodes, token)?)
            } else {
                self.search_empty(token)
            }
        } else if saw_matcher {
            node_set
        } else {
            self.search_empty(token)
        };
        // Deduplicate results if a globstar and a matcher were used, for correctness
        // e.g. There is a file `/bar/emm/bar/foo`, searching for `bar/**/foo`
        // will match it twice. We want to return it only once.
        if saw_globstar
            && saw_matcher
            && let Some(nodes) = &mut nodes
        {
            dedup_indices_in_place(nodes);
        }
        nodes
    }

    fn match_initial_segment(
        &self,
        matcher: &SegmentMatcherConcrete,
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let names: BTreeSet<_> = match matcher {
            SegmentMatcherConcrete::Plain { kind, needle } => match kind {
                SegmentKind::Substr => NAME_POOL.search_substr(needle, token),
                SegmentKind::Prefix => NAME_POOL.search_prefix(needle, token),
                SegmentKind::Suffix => NAME_POOL.search_suffix(needle, token),
                SegmentKind::Exact => NAME_POOL.search_exact(needle, token),
            },
            SegmentMatcherConcrete::PlainInsensitive { kind, needle } => match kind {
                SegmentKind::Substr => NAME_POOL.search_substr_insensitive(needle, token),
                SegmentKind::Prefix => NAME_POOL.search_prefix_insensitive(needle, token),
                SegmentKind::Suffix => NAME_POOL.search_suffix_insensitive(needle, token),
                SegmentKind::Exact => NAME_POOL.search_exact_insensitive(needle, token),
            },
            SegmentMatcherConcrete::Regex { regex } => NAME_POOL.search_regex(regex, token),
        }?;
        let mut nodes = Vec::with_capacity(names.len());
        for (i, name) in names.iter().enumerate() {
            token.is_cancelled_sparse(i)?;
            if let Some(indices) = self.name_index.get(name) {
                nodes.extend(indices.iter().copied());
            }
        }
        Some(nodes)
    }

    fn match_direct_child_segments(
        &self,
        parents: &[SlabIndex],
        matcher: &SegmentMatcherConcrete,
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let mut new_node_set = Vec::new();
        for (i, &node) in parents.iter().enumerate() {
            token.is_cancelled_sparse(i)?;
            let mut child_matches = self.file_nodes[node]
                .children
                .iter()
                .filter_map(|&child| {
                    let name = self.file_nodes[child].name();
                    if matcher.matches(name) {
                        Some((name, child))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            child_matches.sort_unstable_by_key(|(name, _)| *name);
            new_node_set.extend(child_matches.into_iter().map(|(_, index)| index));
        }
        Some(new_node_set)
    }

    fn all_direct_children(
        &self,
        parents: &[SlabIndex],
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let mut new_node_set = Vec::new();
        for (i, &node) in parents.iter().enumerate() {
            token.is_cancelled_sparse(i)?;
            let mut child_matches = self.file_nodes[node]
                .children
                .iter()
                .map(|&child| {
                    let name = self.file_nodes[child].name();
                    (name, child)
                })
                .collect::<Vec<_>>();
            child_matches.sort_unstable_by_key(|(name, _)| *name);
            new_node_set.extend(child_matches.into_iter().map(|(_, index)| index));
        }
        Some(new_node_set)
    }

    fn match_descendant_segments(
        &self,
        parents: &[SlabIndex],
        matcher: &SegmentMatcherConcrete,
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let mut matches = Vec::new();
        let mut visited = 0usize;
        for &node in parents {
            token.is_cancelled_sparse(visited)?;
            let descendants = self.all_subnodes(node, token)?;
            for descendant in descendants {
                token.is_cancelled_sparse(visited)?;
                visited += 1;
                let name = self.file_nodes[descendant].name();
                if matcher.matches(name) {
                    matches.push((name, descendant));
                }
            }
        }
        matches.sort_unstable_by_key(|(name, _)| *name);
        Some(matches.into_iter().map(|(_, index)| index).collect())
    }

    fn all_descendant_segments(
        &self,
        parents: &[SlabIndex],
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        let mut matches = Vec::new();
        let mut visited = 0usize;
        for &node in parents {
            token.is_cancelled_sparse(visited)?;
            let descendants = self.all_subnodes(node, token)?;
            for descendant in descendants {
                token.is_cancelled_sparse(visited)?;
                visited += 1;
                let name = self.file_nodes[descendant].name();
                matches.push((name, descendant));
            }
        }
        matches.sort_unstable_by_key(|(name, _)| *name);
        Some(matches.into_iter().map(|(_, index)| index).collect())
    }

    fn evaluate_regex(
        &self,
        pattern: &str,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let mut builder = RegexBuilder::new(pattern);
        builder.case_insensitive(options.case_insensitive);
        let regex = builder
            .build()
            .map_err(|err| anyhow!("Invalid regex pattern: {err}"))?;
        let matcher = SegmentMatcher::Concrete(SegmentMatcherConcrete::Regex { regex });
        Ok(self.execute_matchers(std::slice::from_ref(&matcher), token))
    }

    fn evaluate_regex_with_base(
        &self,
        pattern: &str,
        base: Option<&Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(mut nodes) = self.evaluate_regex(pattern, options, token)? else {
            return Ok(None);
        };
        if let Some(base) = base
            && intersect_in_place(&mut nodes, base, token).is_none()
        {
            return Ok(None);
        }
        Ok(Some(nodes))
    }

    fn evaluate_filter(
        &mut self,
        filter: &Filter,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        match filter.kind {
            FilterKind::File => self.evaluate_type_filter(
                NodeFileType::File,
                base,
                filter.argument.as_ref(),
                options,
                token,
            ),
            FilterKind::Folder => self.evaluate_type_filter(
                NodeFileType::Dir,
                base,
                filter.argument.as_ref(),
                options,
                token,
            ),
            FilterKind::Ext => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("ext: requires at least one extension"))?;
                self.evaluate_extension_filter(argument, base, token)
            }
            FilterKind::Parent => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("parent: requires a folder path"))?;
                self.evaluate_parent_filter(argument, base, options, token)
            }
            FilterKind::InFolder => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("infolder: requires a folder path"))?;
                self.evaluate_infolder_filter(argument, base, options, token)
            }
            FilterKind::NoSubfolders => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("nosubfolders: requires a folder path"))?;
                self.evaluate_nosubfolders_filter(argument, base, options, token)
            }
            FilterKind::Type => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("type: requires a category"))?;
                self.evaluate_named_type_filter(&argument.raw, base, options, token)
            }
            FilterKind::Audio => {
                self.evaluate_type_macro("audio", base, filter.argument.as_ref(), options, token)
            }
            FilterKind::Video => {
                self.evaluate_type_macro("video", base, filter.argument.as_ref(), options, token)
            }
            FilterKind::Doc => {
                self.evaluate_type_macro("doc", base, filter.argument.as_ref(), options, token)
            }
            FilterKind::Exe => {
                self.evaluate_type_macro("exe", base, filter.argument.as_ref(), options, token)
            }
            FilterKind::Size => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("size: requires a value"))?;
                self.evaluate_size_filter(argument, base, token)
            }
            FilterKind::DateModified => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("dm: requires a date or range"))?;
                self.evaluate_date_filter(DateField::Modified, argument, base, token)
            }
            FilterKind::DateCreated => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("dc: requires a date or range"))?;
                self.evaluate_date_filter(DateField::Created, argument, base, token)
            }
            FilterKind::Content => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("content: requires a value"))?;
                self.evaluate_content_filter(argument, base, options, token)
            }
            FilterKind::Tag => {
                let argument = filter
                    .argument
                    .as_ref()
                    .ok_or_else(|| anyhow!("tag: requires a value"))?;
                self.evaluate_tag_filter(argument, base, options, token)
            }
            _ => bail!("Filter {:?} is not supported yet", filter.kind),
        }
    }

    fn evaluate_type_filter(
        &self,
        file_type: NodeFileType,
        base: Option<Vec<SlabIndex>>,
        argument: Option<&FilterArgument>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let (mut nodes, argument_applied) = match (base, argument) {
            (Some(nodes), _) => (nodes, false),
            (None, Some(arg)) => match self.evaluate_phrase(&arg.raw, options, token)? {
                Some(nodes) => (nodes, true),
                None => return Ok(None),
            },
            (None, None) => match self.search_empty(token) {
                Some(nodes) => (nodes, false),
                None => return Ok(None),
            },
        };

        if !argument_applied && let Some(arg) = argument {
            let Some(matches) = self.evaluate_phrase(&arg.raw, options, token)? else {
                return Ok(None);
            };
            if intersect_in_place(&mut nodes, &matches, token).is_none() {
                return Ok(None);
            }
        }

        Ok(filter_nodes(nodes, token, |index| {
            self.file_nodes[index].file_type_hint() == file_type
        }))
    }

    fn evaluate_extension_filter(
        &self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let extensions = normalize_extensions(argument);
        if extensions.is_empty() {
            bail!("ext: requires non-empty extensions");
        }
        let Some(nodes) = self.nodes_from_base(base, token) else {
            return Ok(None);
        };
        Ok(filter_nodes(nodes, token, |index| {
            let node = &self.file_nodes[index];
            if node.file_type_hint() != NodeFileType::File {
                return false;
            }
            extension_of(node.name())
                .map(|ext| extensions.contains(ext.as_str()))
                .unwrap_or(false)
        }))
    }

    fn evaluate_parent_filter(
        &self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(target) =
            self.node_index_for_path_with_case(Path::new(&argument.raw), options.case_insensitive)
        else {
            bail!(
                "Parent filter {:?} is not found in file system",
                argument.raw
            );
        };
        let children = self.file_nodes[target].children.to_vec();
        if let Some(mut nodes) = base {
            if intersect_in_place(&mut nodes, &children, token).is_none() {
                return Ok(None);
            }
            Ok(Some(nodes))
        } else {
            Ok(Some(children))
        }
    }

    fn evaluate_infolder_filter(
        &self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(target) =
            self.node_index_for_path_with_case(Path::new(&argument.raw), options.case_insensitive)
        else {
            bail!(
                "Parent filter {:?} is not found in file system",
                argument.raw
            );
        };
        let Some(children) = self.all_subnodes(target, token) else {
            return Ok(None);
        };
        if let Some(mut nodes) = base {
            if intersect_in_place(&mut nodes, &children, token).is_none() {
                return Ok(None);
            }
            Ok(Some(nodes))
        } else {
            Ok(Some(children))
        }
    }

    fn evaluate_nosubfolders_filter(
        &self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let Some(target) =
            self.node_index_for_path_with_case(Path::new(&argument.raw), options.case_insensitive)
        else {
            bail!(
                "nosubfolders filter {:?} is not found in file system",
                argument.raw
            );
        };
        if self.file_nodes[target].file_type_hint() != NodeFileType::Dir {
            bail!("nosubfolders path {:?} is not a folder", argument.raw);
        }

        let nodes = base.unwrap_or_else(|| self.file_nodes[target].children.to_vec());

        Ok(filter_nodes(nodes, token, |index| {
            self.keep_node_for_nosubfolders(index, target)
        }))
    }

    fn keep_node_for_nosubfolders(&self, index: SlabIndex, root: SlabIndex) -> bool {
        index == root || {
            let node = &self.file_nodes[index];
            node.parent() == Some(root) && node.file_type_hint() != NodeFileType::Dir
        }
    }

    fn evaluate_named_type_filter(
        &self,
        raw: &str,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let name = raw.trim();
        if name.is_empty() {
            bail!("type: requires a category");
        }
        let normalized = name.to_ascii_lowercase();
        let Some(target) = lookup_type_group(&normalized) else {
            bail!("Unknown type category: {name}");
        };
        self.apply_type_group(target, base, options, token)
    }

    fn evaluate_type_macro(
        &self,
        name: &'static str,
        base: Option<Vec<SlabIndex>>,
        argument: Option<&FilterArgument>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let group_nodes = self.apply_type_group(
            lookup_type_group(name).expect("built-in macro should map to a known type group"),
            base,
            options,
            token,
        )?;
        let Some(mut nodes) = group_nodes else {
            return Ok(None);
        };
        let Some(argument) = argument else {
            return Ok(Some(nodes));
        };
        let Some(matches) = self.evaluate_phrase(&argument.raw, options, token)? else {
            return Ok(None);
        };
        if intersect_in_place(&mut nodes, &matches, token).is_none() {
            return Ok(None);
        }
        Ok(Some(nodes))
    }

    fn apply_type_group(
        &self,
        target: TypeFilterTarget,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        match target {
            TypeFilterTarget::NodeType(file_type) => {
                self.evaluate_type_filter(file_type, base, None, options, token)
            }
            TypeFilterTarget::Extensions(list) => self.filter_static_extensions(list, base, token),
        }
    }

    fn filter_static_extensions(
        &self,
        extensions: &'static [&'static str],
        base: Option<Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        if extensions.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let Some(nodes) = self.nodes_from_base(base, token) else {
            return Ok(None);
        };
        Ok(filter_nodes(nodes, token, |index| {
            let node = &self.file_nodes[index];
            if node.file_type_hint() != NodeFileType::File {
                return false;
            }
            if let Some(ext) = extension_of(node.name()) {
                extensions.iter().any(|needle| *needle == ext)
            } else {
                false
            }
        }))
    }

    fn evaluate_size_filter(
        &mut self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let predicate = SizePredicate::parse(argument)?;
        let Some(nodes) = self.nodes_from_base(base, token) else {
            return Ok(None);
        };
        Ok(filter_nodes(nodes, token, |index| {
            let node = &self.file_nodes[index];
            if node.file_type_hint() != NodeFileType::File {
                return false;
            }
            let metadata = self.ensure_metadata(index);
            let Some(meta) = metadata.as_ref() else {
                return false;
            };
            let size = meta.size();

            predicate.matches(size as u64)
        }))
    }

    fn evaluate_date_filter(
        &mut self,
        field: DateField,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let context = DateContext::capture();
        let predicate = DatePredicate::parse(argument, &context)?;
        let Some(nodes) = self.nodes_from_base(base, token) else {
            return Ok(None);
        };
        Ok(filter_nodes(nodes, token, |index| {
            let Some(timestamp) = self.node_timestamp(index, field) else {
                return false;
            };
            predicate.matches(timestamp)
        }))
    }

    fn evaluate_content_filter(
        &mut self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let ghost;
        let needle = if options.case_insensitive {
            ghost = argument.raw.to_ascii_lowercase().into_bytes();
            &ghost
        } else {
            argument.raw.as_bytes()
        };
        if needle.is_empty() {
            bail!("content: requires a value");
        }

        let Some(nodes) = self.nodes_from_base(base, token) else {
            return Ok(None);
        };

        let matched_indices = nodes
            .into_iter()
            .filter(|index| self.file_nodes[*index].file_type_hint() == NodeFileType::File)
            .filter_map(|index| self.node_path(index).map(|path| (index, path)))
            .par_bridge()
            .filter_map(|(index, path)| {
                self.node_content_matches(&path, needle, options.case_insensitive, token)?
                    .then_some(index)
            })
            .collect();

        Ok(token.is_cancelled().map(|()| matched_indices))
    }

    fn evaluate_tag_filter(
        &mut self,
        argument: &FilterArgument,
        base: Option<Vec<SlabIndex>>,
        options: SearchOptions,
        token: CancellationToken,
    ) -> Result<Option<Vec<SlabIndex>>> {
        let raw_needles: Vec<String> = match &argument.kind {
            ArgumentKind::Bare | ArgumentKind::Phrase => {
                let raw = argument.raw.trim();
                if raw.is_empty() {
                    bail!("tag: requires a value");
                }
                vec![raw.to_string()]
            }
            ArgumentKind::List(values) => {
                let normalized: Vec<String> = values
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
                    .collect();

                if normalized.is_empty() {
                    bail!("tag: requires a value");
                }
                normalized
            }
            ArgumentKind::Range(_) | ArgumentKind::Comparison(_) => {
                bail!("tag: does not support ranges or comparisons");
            }
        };
        let needles = if options.case_insensitive {
            raw_needles
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect()
        } else {
            raw_needles
        };

        let Some(nodes) = self.nodes_from_base(base.clone(), token) else {
            return Ok(None);
        };

        // If base is a small set, filtering it by accessing file metadata;
        // otherwise use mdfind to quickly narrow down.
        let matched_indices = if nodes.len() <= TAG_FILTER_MDFIND_THRESHOLD {
            nodes
                .into_iter()
                .filter_map(|index| self.node_path(index).map(|path| (index, path)))
                .par_bridge()
                .filter_map(|(index, path)| {
                    self.node_tags_match_any(&path, &needles, options.case_insensitive, token)?
                        .then_some(index)
                })
                .collect()
        } else {
            let spotlight_indices: Vec<SlabIndex> =
                search_tags_using_mdfind(needles, options.case_insensitive)?
                    .into_iter()
                    .filter_map(|path| self.node_index_for_path(&path))
                    .collect();

            match base {
                Some(base) => {
                    let mut nodes = base;
                    let allowed = spotlight_indices.iter().copied().collect::<HashSet<_>>();
                    nodes.retain(|index| allowed.contains(index));
                    nodes
                }
                None => spotlight_indices,
            }
        };

        Ok(token.is_cancelled().map(|()| matched_indices))
    }

    /// user need to ensure that needle is lowercased when case_insensitive is set
    fn node_content_matches(
        &self,
        path: &Path,
        needle: &[u8],
        case_insensitive: bool,
        token: CancellationToken,
    ) -> Option<bool> {
        token.is_cancelled()?;

        let Ok(mut file) = File::open(path) else {
            return Some(false);
        };

        if needle.len() == 1 {
            let needle = needle[0];
            let mut buffer = vec![0u8; CONTENT_BUFFER_BYTES];
            if case_insensitive {
                let lowercase_target = needle.to_ascii_lowercase();
                let uppercase_target = needle.to_ascii_uppercase();
                loop {
                    token.is_cancelled()?;
                    let read = match file.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => count,
                        Err(_) => return Some(false),
                    };
                    if buffer[..read]
                        .iter()
                        .any(|&c| c == lowercase_target || c == uppercase_target)
                    {
                        return Some(true);
                    }
                }
            } else {
                loop {
                    token.is_cancelled()?;
                    let read = match file.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => count,
                        Err(_) => return Some(false),
                    };
                    if buffer[..read].contains(&needle) {
                        return Some(true);
                    }
                }
            };

            return Some(false);
        }

        // ensure needle is lowercased if case_insensitive is set
        if case_insensitive {
            debug_assert_eq!(needle, needle.to_ascii_lowercase());
        }
        let overlap = needle.len().saturating_sub(1);
        let finder = rabinkarp::Finder::new(needle);
        let mut buffer = vec![0u8; CONTENT_BUFFER_BYTES + overlap];
        let mut carry_len = 0usize;

        loop {
            token.is_cancelled()?;

            let Ok(read) = file.read(&mut buffer[carry_len..]) else {
                return Some(false);
            };
            if read == 0 {
                break;
            }

            let chunk_len = carry_len + read;
            let chunk = &mut buffer[..chunk_len];

            if case_insensitive {
                chunk[carry_len..].make_ascii_lowercase();
            }

            if finder.find(chunk, needle).is_some() {
                return Some(true);
            }

            let keep = overlap.min(chunk.len());
            if keep > 0 {
                let start = chunk.len().saturating_sub(keep);
                chunk.copy_within(start.., 0);
            }
            carry_len = keep;
        }

        Some(false)
    }

    fn node_tags_match_any(
        &self,
        path: &Path,
        needles: &[String],
        case_insensitive: bool,
        token: CancellationToken,
    ) -> Option<bool> {
        token.is_cancelled()?;

        let tags = read_tags_from_path(path, case_insensitive)?;
        let matched = tags
            .iter()
            .any(|tag| needles.iter().any(|needle| tag.contains(needle)));
        Some(matched)
    }

    fn nodes_from_base(
        &self,
        base: Option<Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        match base {
            Some(nodes) => Some(nodes),
            None => self.search_empty(token),
        }
    }

    fn nodes_from_base_ref(
        &self,
        base: Option<&Vec<SlabIndex>>,
        token: CancellationToken,
    ) -> Option<Vec<SlabIndex>> {
        match base {
            Some(nodes) => Some(nodes.clone()),
            None => self.search_empty(token),
        }
    }

    fn node_timestamp(&mut self, index: SlabIndex, field: DateField) -> Option<i64> {
        let metadata = self.ensure_metadata(index);
        let meta = metadata.as_ref()?;
        match field {
            DateField::Modified => meta.mtime(),
            DateField::Created => meta.ctime(),
        }
        .map(|value| value.get() as i64)
    }

    fn ensure_metadata(&mut self, index: SlabIndex) -> SlabNodeMetadataCompact {
        let current = self.file_nodes[index].metadata;
        if current.is_some() {
            return current;
        }
        let path = self
            .node_path(index)
            .expect("node index is not present in slab");
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(data) => SlabNodeMetadataCompact::some(data.into()),
            Err(_) => SlabNodeMetadataCompact::unaccessible(),
        };
        self.file_nodes[index].metadata = metadata;
        metadata
    }
}

// 拆分出的私有辅助模块（仅本模块使用）
mod dates;
mod filters;
mod ops;
mod sizes;

use self::{dates::*, filters::*, ops::*, sizes::*};
