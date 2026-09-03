//! FOMOD scripted installers.
//!
//! A growing number of mods ship as one download that contains every variant
//! the author offers, with a `fomod/ModuleConfig.xml` describing the questions
//! to ask and the files each answer installs. Without reading that script the
//! archive looks like several dozen sibling folders, and choosing between them
//! by hand is exactly the chore the script exists to remove.
//!
//! The script is data from an untrusted archive, so nothing here trusts it:
//! every source and destination is re-checked against the same rules the
//! extractor applies, and a path that tries to leave the package is refused.

use crate::{
    archives,
    error::{AppError, Result},
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// An image is shown beside one option, so a large one is a packaging mistake
/// rather than something worth loading into the interface.
const MAX_IMAGE_BYTES: u64 = 4 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Parsed script
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginType {
    Required,
    Recommended,
    Optional,
    CouldBeUsable,
    NotUsable,
}

impl PluginType {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "required" => Self::Required,
            "recommended" => Self::Recommended,
            "couldbeusable" => Self::CouldBeUsable,
            "notusable" => Self::NotUsable,
            _ => Self::Optional,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Required => "Required",
            Self::Recommended => "Recommended",
            Self::Optional => "Optional",
            Self::CouldBeUsable => "CouldBeUsable",
            Self::NotUsable => "NotUsable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    ExactlyOne,
    AtMostOne,
    AtLeastOne,
    Any,
    All,
}

impl GroupKind {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "selectatmostone" => Self::AtMostOne,
            "selectatleastone" => Self::AtLeastOne,
            "selectall" => Self::All,
            "selectany" => Self::Any,
            // `SelectExactlyOne` is both the common case and the safest
            // reading of a type nobody recognizes: it never installs two
            // alternatives at once.
            _ => Self::ExactlyOne,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ExactlyOne => "SelectExactlyOne",
            Self::AtMostOne => "SelectAtMostOne",
            Self::AtLeastOne => "SelectAtLeastOne",
            Self::Any => "SelectAny",
            Self::All => "SelectAll",
        }
    }
}

/// A condition on the flags earlier steps have set.
///
/// The format also allows conditions on installed game plugins and on tool and
/// game versions. None of those exist for this game, so they are parsed,
/// reported once as a warning, and treated as unmet rather than being silently
/// read as satisfied, which would install files the author gated deliberately.
#[derive(Debug, Clone)]
enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Flag { name: String, value: String },
    Unsupported,
}

impl Condition {
    fn met(&self, flags: &BTreeMap<String, String>) -> bool {
        match self {
            Self::All(inner) => inner.iter().all(|condition| condition.met(flags)),
            Self::Any(inner) => inner.is_empty() || inner.iter().any(|c| c.met(flags)),
            Self::Flag { name, value } => {
                flags.get(name).map(String::as_str) == Some(value.as_str())
            }
            Self::Unsupported => false,
        }
    }
}

#[derive(Debug, Clone)]
enum TypeDescriptor {
    Fixed(PluginType),
    /// The first matching pattern wins, falling back to the default.
    Conditional {
        default: PluginType,
        patterns: Vec<(Condition, PluginType)>,
    },
}

impl TypeDescriptor {
    fn resolve(&self, flags: &BTreeMap<String, String>) -> PluginType {
        match self {
            Self::Fixed(kind) => *kind,
            Self::Conditional { default, patterns } => patterns
                .iter()
                .find(|(condition, _)| condition.met(flags))
                .map_or(*default, |(_, kind)| *kind),
        }
    }
}

/// One `<file>` or `<folder>` the script installs.
#[derive(Debug, Clone)]
struct FileItem {
    source: PathBuf,
    destination: PathBuf,
    priority: i64,
    folder: bool,
    /// Installed whether or not its plugin was selected.
    always: bool,
}

#[derive(Debug, Clone)]
struct Plugin {
    name: String,
    description: Option<String>,
    image: Option<PathBuf>,
    flags: Vec<(String, String)>,
    files: Vec<FileItem>,
    kind: TypeDescriptor,
}

#[derive(Debug, Clone)]
struct Group {
    name: String,
    kind: GroupKind,
    plugins: Vec<Plugin>,
}

#[derive(Debug, Clone)]
struct Step {
    name: String,
    visible: Condition,
    groups: Vec<Group>,
}

/// A parsed `ModuleConfig.xml` together with the directory its paths resolve
/// against.
#[derive(Debug, Clone)]
pub struct Installer {
    pub package_root: PathBuf,
    pub module_name: String,
    module_image: Option<PathBuf>,
    pub info: Info,
    required: Vec<FileItem>,
    steps: Vec<Step>,
    conditional: Vec<(Condition, Vec<FileItem>)>,
    warnings: Vec<String>,
}

/// The publishing metadata from `fomod/info.xml`, which is the only place a
/// FOMOD archive states its own name, author, and version.
#[derive(Debug, Clone, Default)]
pub struct Info {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One answered step, as the interface hands it back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepAnswer {
    pub step: usize,
    /// Plugin ids, in `g<group>p<plugin>` form.
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// A `data:` URL, because the interface cannot read the sandbox directly.
    pub image: Option<String>,
    pub kind: String,
    /// Whether the author's own answer selects this option.
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub name: String,
    pub kind: String,
    pub plugins: Vec<PluginView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub index: usize,
    pub name: String,
    pub groups: Vec<GroupView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    pub module_name: String,
    pub module_image: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    /// The step awaiting an answer, or `None` once every question is answered.
    pub step: Option<StepView>,
    /// Position of that step among the ones actually shown, 1-based.
    pub position: usize,
    /// How many steps have been shown or are still ahead, for the progress
    /// line. Later steps can be hidden by earlier answers, so this is an upper
    /// bound rather than a promise.
    pub total: usize,
    pub complete: bool,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Locating and parsing
// ---------------------------------------------------------------------------

fn child(directory: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(std::result::Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        })
        .map(|entry| entry.path())
}

/// Follows an archive-relative path through a staged tree without regard to
/// case. The script was authored on Windows, where `Fomod\Images\X.png` and
/// `fomod/images/x.png` are the same file, and the staged copy on Linux is not.
fn resolve(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(part) = component else {
            return None;
        };
        current = child(&current, &part.to_string_lossy())?;
    }
    Some(current)
}

/// The directory a FOMOD script's paths are relative to, if the staged tree
/// holds one. Archives regularly wrap their contents in a single folder, so a
/// shallow search is needed rather than a look at the root alone.
pub fn locate(root: &Path) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for entry in WalkDir::new(root)
        .follow_links(false)
        // Depth 1 at the shallowest: the directory holding the script has to
        // be inside the staged tree, so that its parent — everything the
        // script's paths are relative to — is inside it too.
        .min_depth(1)
        .max_depth(3)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if !entry.file_type().is_dir() || !entry.file_name().eq_ignore_ascii_case("fomod") {
            continue;
        }
        if child(entry.path(), "ModuleConfig.xml").is_none() {
            continue;
        }
        let Some(package) = entry.path().parent().map(Path::to_path_buf) else {
            continue;
        };
        // The shallowest script is the one describing the whole download; a
        // deeper one belongs to something the download merely bundles.
        let deeper = found
            .as_ref()
            .is_some_and(|current| current.components().count() <= package.components().count());
        if !deeper {
            found = Some(package);
        }
    }
    found
}

fn attribute<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
        .map(|attribute| attribute.value())
}

fn children<'a>(
    node: roxmltree::Node<'a, 'a>,
    name: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> + 'a {
    node.children().filter(move |child| {
        child.is_element() && child.tag_name().name().eq_ignore_ascii_case(name)
    })
}

fn first<'a>(node: roxmltree::Node<'a, 'a>, name: &'a str) -> Option<roxmltree::Node<'a, 'a>> {
    children(node, name).next()
}

fn text(node: roxmltree::Node<'_, '_>) -> Option<String> {
    let value = node.text().unwrap_or_default().trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// An archive-relative path from a script attribute, refused if it could reach
/// outside the package.
fn script_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    archives::archive_relative(trimmed)
}

fn parse_condition(node: roxmltree::Node<'_, '_>, warnings: &mut Vec<String>) -> Condition {
    let mut inner = Vec::new();
    for child in node.children().filter(roxmltree::Node::is_element) {
        let tag = child.tag_name().name().to_ascii_lowercase();
        match tag.as_str() {
            "dependencies" => inner.push(parse_condition(child, warnings)),
            "flagdependency" => {
                let Some(name) = attribute(child, "flag") else {
                    continue;
                };
                inner.push(Condition::Flag {
                    name: name.trim().to_string(),
                    value: attribute(child, "value")
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                });
            }
            "filedependency" | "gamedependency" | "foseDependency" | "fommdependency"
            | "versiondependency" => {
                warnings.push(format!(
                    "This installer asks about the state of another game plugin or tool ({tag}), \
                     which Zero Company does not have. Options relying on it are treated as \
                     unavailable."
                ));
                inner.push(Condition::Unsupported);
            }
            _ => {}
        }
    }
    if attribute(node, "operator").is_some_and(|op| op.eq_ignore_ascii_case("or")) {
        Condition::Any(inner)
    } else {
        Condition::All(inner)
    }
}

fn parse_files(node: roxmltree::Node<'_, '_>) -> Vec<FileItem> {
    let mut items = Vec::new();
    for child in node.children().filter(roxmltree::Node::is_element) {
        let folder = match child.tag_name().name().to_ascii_lowercase().as_str() {
            "file" => false,
            "folder" => true,
            _ => continue,
        };
        let Some(source) = attribute(child, "source").and_then(script_path) else {
            continue;
        };
        // An omitted or empty destination means "keep the layout the source
        // already has", which for a folder is its own contents at the root.
        let destination = attribute(child, "destination")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or_else(
                || {
                    Some(if folder {
                        PathBuf::new()
                    } else {
                        source.clone()
                    })
                },
                script_path,
            );
        let Some(destination) = destination else {
            continue;
        };
        items.push(FileItem {
            source,
            destination,
            priority: attribute(child, "priority")
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0),
            folder,
            always: attribute(child, "alwaysInstall").is_some_and(|value| value.trim() == "true"),
        });
    }
    items
}

fn parse_type(node: roxmltree::Node<'_, '_>, warnings: &mut Vec<String>) -> TypeDescriptor {
    if let Some(fixed) = first(node, "type") {
        return TypeDescriptor::Fixed(PluginType::parse(attribute(fixed, "name").unwrap_or("")));
    }
    let Some(conditional) = first(node, "dependencyType") else {
        return TypeDescriptor::Fixed(PluginType::Optional);
    };
    let default = first(conditional, "defaultType")
        .and_then(|node| attribute(node, "name"))
        .map_or(PluginType::Optional, PluginType::parse);
    let patterns = first(conditional, "patterns")
        .into_iter()
        .flat_map(|node| children(node, "pattern").collect::<Vec<_>>())
        .map(|pattern| {
            let kind = first(pattern, "type")
                .and_then(|node| attribute(node, "name"))
                .map_or(PluginType::Optional, PluginType::parse);
            let condition = first(pattern, "dependencies")
                .map_or(Condition::All(Vec::new()), |node| {
                    parse_condition(node, warnings)
                });
            (condition, kind)
        })
        .collect();
    TypeDescriptor::Conditional { default, patterns }
}

fn parse_plugin(
    node: roxmltree::Node<'_, '_>,
    package_root: &Path,
    warnings: &mut Vec<String>,
) -> Plugin {
    Plugin {
        name: attribute(node, "name")
            .unwrap_or("Option")
            .trim()
            .to_string(),
        description: first(node, "description").and_then(text),
        image: first(node, "image")
            .and_then(|image| attribute(image, "path"))
            .and_then(script_path)
            .and_then(|path| resolve(package_root, &path)),
        flags: first(node, "conditionFlags")
            .into_iter()
            .flat_map(|node| children(node, "flag").collect::<Vec<_>>())
            .filter_map(|flag| {
                attribute(flag, "name").map(|name| {
                    (
                        name.trim().to_string(),
                        flag.text().unwrap_or_default().trim().to_string(),
                    )
                })
            })
            .collect(),
        files: first(node, "files").map(parse_files).unwrap_or_default(),
        kind: first(node, "typeDescriptor")
            .map_or(TypeDescriptor::Fixed(PluginType::Optional), |node| {
                parse_type(node, warnings)
            }),
    }
}

fn parse_info(package_root: &Path) -> Info {
    let Some(path) = child(package_root, "fomod").and_then(|dir| child(&dir, "info.xml")) else {
        return Info::default();
    };
    let Ok(raw) = fs::read_to_string(&path) else {
        return Info::default();
    };
    let Ok(document) = roxmltree::Document::parse(&raw) else {
        return Info::default();
    };
    let root = document.root_element();
    let read = |name: &str| {
        root.children()
            .find(|child| child.is_element() && child.tag_name().name().eq_ignore_ascii_case(name))
            .and_then(text)
    };
    Info {
        name: read("Name"),
        author: read("Author"),
        version: read("Version"),
        description: read("Description"),
    }
}

/// Reads the script a staged archive carries.
pub fn parse(package_root: &Path) -> Result<Installer> {
    let path = child(package_root, "fomod")
        .and_then(|dir| child(&dir, "ModuleConfig.xml"))
        .ok_or_else(|| AppError::Other("This archive has no fomod/ModuleConfig.xml.".into()))?;
    let raw = fs::read_to_string(&path)?;
    // A BOM is common in files written on Windows and is not valid XML content.
    let document =
        roxmltree::Document::parse(raw.trim_start_matches('\u{feff}')).map_err(|error| {
            AppError::Other(format!("This installer script is not valid XML: {error}"))
        })?;
    let root = document.root_element();
    let mut warnings = Vec::new();

    let steps = first(root, "installSteps")
        .into_iter()
        .flat_map(|node| children(node, "installStep").collect::<Vec<_>>())
        .map(|node| Step {
            name: attribute(node, "name")
                .unwrap_or("Options")
                .trim()
                .to_string(),
            visible: first(node, "visible").map_or(Condition::All(Vec::new()), |node| {
                parse_condition(node, &mut warnings)
            }),
            groups: first(node, "optionalFileGroups")
                .into_iter()
                .flat_map(|node| children(node, "group").collect::<Vec<_>>())
                .map(|node| Group {
                    name: attribute(node, "name")
                        .unwrap_or("Options")
                        .trim()
                        .to_string(),
                    kind: GroupKind::parse(attribute(node, "type").unwrap_or("")),
                    plugins: first(node, "plugins")
                        .into_iter()
                        .flat_map(|node| children(node, "plugin").collect::<Vec<_>>())
                        .map(|node| parse_plugin(node, package_root, &mut warnings))
                        .collect(),
                })
                .collect(),
        })
        .collect();

    let conditional = first(root, "conditionalFileInstalls")
        .and_then(|node| first(node, "patterns"))
        .into_iter()
        .flat_map(|node| children(node, "pattern").collect::<Vec<_>>())
        .map(|pattern| {
            let condition = first(pattern, "dependencies")
                .map_or(Condition::All(Vec::new()), |node| {
                    parse_condition(node, &mut warnings)
                });
            (
                condition,
                first(pattern, "files").map(parse_files).unwrap_or_default(),
            )
        })
        .collect();

    warnings.sort();
    warnings.dedup();
    let info = parse_info(package_root);
    Ok(Installer {
        module_name: first(root, "moduleName")
            .and_then(text)
            .or_else(|| info.name.clone())
            .unwrap_or_else(|| "Mod installer".into()),
        module_image: first(root, "moduleImage")
            .and_then(|node| attribute(node, "path"))
            .and_then(script_path)
            .and_then(|path| resolve(package_root, &path)),
        required: first(root, "requiredInstallFiles")
            .map(parse_files)
            .unwrap_or_default(),
        steps,
        conditional,
        warnings,
        info,
        package_root: package_root.to_path_buf(),
    })
}

// ---------------------------------------------------------------------------
// Running the wizard
// ---------------------------------------------------------------------------

fn plugin_id(group: usize, plugin: usize) -> String {
    format!("g{group}p{plugin}")
}

/// The step the wizard has reached, with the flags every answered step set.
struct Position {
    flags: BTreeMap<String, String>,
    /// The index of the step awaiting an answer.
    current: Option<usize>,
    /// How many steps were shown before it.
    shown: usize,
    /// How many steps earlier answers have hidden.
    skipped: usize,
}

impl Installer {
    /// Replays the answers given so far and reports where that leaves the
    /// wizard. Replaying from the start on every call is what lets a person go
    /// back and change an answer: nothing is remembered between calls that the
    /// answers themselves do not say.
    fn advance(&self, answers: &[StepAnswer]) -> Result<Position> {
        let mut flags = BTreeMap::new();
        let mut shown = 0;
        let mut skipped = 0;
        for (index, step) in self.steps.iter().enumerate() {
            if !step.visible.met(&flags) {
                skipped += 1;
                continue;
            }
            let Some(answer) = answers.iter().find(|answer| answer.step == index) else {
                return Ok(Position {
                    flags,
                    current: Some(index),
                    shown,
                    skipped,
                });
            };
            let selected = self.validate(step, &answer.plugins, &flags)?;
            for (group, plugin) in selected {
                for (name, value) in &step.groups[group].plugins[plugin].flags {
                    flags.insert(name.clone(), value.clone());
                }
            }
            shown += 1;
        }
        Ok(Position {
            flags,
            current: None,
            shown,
            skipped,
        })
    }

    /// Checks an answer against the group rules the script declares, and
    /// returns the selected plugins as `(group, plugin)` positions.
    ///
    /// The interface enforces the same rules while a person is choosing, but
    /// the answer arrives here as plain data and the rules decide which files
    /// get written, so they are enforced where that decision is made.
    fn validate(
        &self,
        step: &Step,
        chosen: &[String],
        flags: &BTreeMap<String, String>,
    ) -> Result<Vec<(usize, usize)>> {
        let chosen: BTreeSet<&str> = chosen.iter().map(String::as_str).collect();
        let mut selected = Vec::new();
        for (group_index, group) in step.groups.iter().enumerate() {
            let mut picked = Vec::new();
            for (plugin_index, plugin) in group.plugins.iter().enumerate() {
                let kind = plugin.kind.resolve(flags);
                let id = plugin_id(group_index, plugin_index);
                let wanted = chosen.contains(id.as_str());
                match kind {
                    PluginType::Required => picked.push(plugin_index),
                    PluginType::NotUsable if wanted => {
                        return Err(AppError::Other(format!(
                            "\"{}\" cannot be installed with the options chosen before it.",
                            plugin.name
                        )))
                    }
                    PluginType::NotUsable => {}
                    _ if wanted => picked.push(plugin_index),
                    _ => {}
                }
            }
            if group.kind == GroupKind::All {
                picked = (0..group.plugins.len()).collect();
            }
            picked.sort_unstable();
            picked.dedup();
            let allowed = match group.kind {
                GroupKind::ExactlyOne => picked.len() == 1,
                GroupKind::AtMostOne => picked.len() <= 1,
                // A group with nothing selectable left cannot be satisfied by
                // the person answering, so it is not held against them.
                GroupKind::AtLeastOne => !picked.is_empty() || group.plugins.is_empty(),
                GroupKind::Any | GroupKind::All => true,
            };
            if !allowed {
                return Err(AppError::Other(format!(
                    "\"{}\" needs {}.",
                    group.name,
                    match group.kind {
                        GroupKind::ExactlyOne => "exactly one option",
                        GroupKind::AtMostOne => "at most one option",
                        _ => "at least one option",
                    }
                )));
            }
            selected.extend(picked.into_iter().map(|plugin| (group_index, plugin)));
        }
        Ok(selected)
    }

    /// The step to display, with each option's availability worked out from the
    /// answers already given and the author's own choice pre-selected.
    fn view(&self, index: usize, flags: &BTreeMap<String, String>) -> StepView {
        let step = &self.steps[index];
        let groups = step
            .groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let kinds: Vec<PluginType> = group
                    .plugins
                    .iter()
                    .map(|plugin| plugin.kind.resolve(flags))
                    .collect();
                // A one-of group has to start on something, so when the author
                // marked nothing as recommended the first usable option is it.
                let fallback = (group.kind == GroupKind::ExactlyOne
                    && !kinds.iter().any(|kind| {
                        matches!(kind, PluginType::Required | PluginType::Recommended)
                    }))
                .then(|| kinds.iter().position(|kind| *kind != PluginType::NotUsable))
                .flatten();
                GroupView {
                    name: group.name.clone(),
                    kind: group.kind.label().to_string(),
                    plugins: group
                        .plugins
                        .iter()
                        .zip(kinds.iter())
                        .enumerate()
                        .map(|(plugin_index, (plugin, kind))| PluginView {
                            id: plugin_id(group_index, plugin_index),
                            name: plugin.name.clone(),
                            description: plugin.description.clone(),
                            image: plugin.image.as_deref().and_then(image_data_url),
                            kind: kind.label().to_string(),
                            selected: group.kind == GroupKind::All
                                || matches!(kind, PluginType::Required | PluginType::Recommended)
                                || fallback == Some(plugin_index),
                        })
                        .collect(),
                }
            })
            .collect();
        StepView {
            index,
            name: step.name.clone(),
            groups,
        }
    }

    /// Builds the session the interface renders.
    pub fn session(&self, session_id: &str, answers: &[StepAnswer]) -> Result<Session> {
        let position = self.advance(answers)?;
        let step = position
            .current
            .map(|index| self.view(index, &position.flags));
        Ok(Session {
            session_id: session_id.to_string(),
            module_name: self.module_name.clone(),
            module_image: self.module_image.as_deref().and_then(image_data_url),
            author: self.info.author.clone(),
            version: self.info.version.clone(),
            description: self.info.description.clone(),
            position: position.shown + 1,
            // An answer still to be given can hide a step further on, so this
            // is the most the wizard can still ask rather than a promise. It
            // only ever falls as questions are answered, never rises.
            total: self.steps.len().saturating_sub(position.skipped).max(1),
            complete: step.is_none(),
            step,
            warnings: self.warnings.clone(),
        })
    }

    /// Every file the answers select, in the order they should be written.
    fn chosen_files(&self, answers: &[StepAnswer]) -> Result<Vec<FileItem>> {
        let mut flags = BTreeMap::new();
        let mut items = self.required.clone();
        for (index, step) in self.steps.iter().enumerate() {
            if !step.visible.met(&flags) {
                continue;
            }
            let Some(answer) = answers.iter().find(|answer| answer.step == index) else {
                return Err(AppError::Other(
                    "This installer still has unanswered questions.".into(),
                ));
            };
            let selected = self.validate(step, &answer.plugins, &flags)?;
            for (group_index, group) in step.groups.iter().enumerate() {
                for (plugin_index, plugin) in group.plugins.iter().enumerate() {
                    let picked = selected.contains(&(group_index, plugin_index));
                    items.extend(
                        plugin
                            .files
                            .iter()
                            .filter(|item| picked || item.always)
                            .cloned(),
                    );
                }
            }
            for (group, plugin) in selected {
                for (name, value) in &step.groups[group].plugins[plugin].flags {
                    flags.insert(name.clone(), value.clone());
                }
            }
        }
        for (condition, files) in &self.conditional {
            if condition.met(&flags) {
                items.extend(files.iter().cloned());
            }
        }
        // Priority decides who wins a shared destination, and a stable sort
        // keeps the script's own order for everything sharing a priority.
        items.sort_by_key(|item| item.priority);
        Ok(items)
    }

    /// Writes the selected files into `destination`, and reports the paths
    /// carrying native code so the caller can warn about them.
    pub fn install(&self, answers: &[StepAnswer], destination: &Path) -> Result<Installed> {
        let items = self.chosen_files(answers)?;
        let mut warnings = Vec::new();
        let mut executables = Vec::new();
        let mut written = 0usize;
        for item in &items {
            let Some(source) = resolve(&self.package_root, &item.source) else {
                warnings.push(format!(
                    "The installer asked for {}, which is not in this archive. It was skipped.",
                    item.source.display()
                ));
                continue;
            };
            // An author writing `<file>` for a directory is common enough that
            // refusing it would fail installers that work everywhere else.
            if item.folder || source.is_dir() {
                for entry in WalkDir::new(&source).follow_links(false) {
                    let entry = entry.map_err(|error| AppError::Other(error.to_string()))?;
                    if entry.file_type().is_symlink() {
                        return Err(AppError::UnsafeArchive(format!(
                            "symbolic link {}",
                            entry.path().display()
                        )));
                    }
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let inside = entry
                        .path()
                        .strip_prefix(&source)
                        .map_err(|error| AppError::Other(error.to_string()))?;
                    copy_into(
                        entry.path(),
                        &item.destination.join(inside),
                        destination,
                        &mut executables,
                    )?;
                    written += 1;
                }
            } else {
                copy_into(&source, &item.destination, destination, &mut executables)?;
                written += 1;
            }
        }
        if written == 0 {
            return Err(AppError::Other(
                "These options install no files. Go back and choose differently.".into(),
            ));
        }
        warnings.extend(self.warnings.iter().cloned());
        executables.sort();
        executables.dedup();
        Ok(Installed {
            warnings,
            executables,
        })
    }
}

pub struct Installed {
    pub warnings: Vec<String>,
    pub executables: Vec<String>,
}

/// A parsed installer waiting on answers, holding the sandbox it was read from
/// so the archive can be discarded once the questions are done with.
pub struct Pending {
    pub installer: Installer,
    pub staging_root: PathBuf,
    pub source: PathBuf,
}

/// Places one file at a script-chosen destination inside `root`.
///
/// The destination came from the archive, so it is re-checked here even though
/// it was checked when parsed: this is the last point before a write.
fn copy_into(
    source: &Path,
    relative: &Path,
    root: &Path,
    executables: &mut Vec<String>,
) -> Result<()> {
    let display = relative.display().to_string();
    let safe = archives::archive_relative(&display)
        .ok_or_else(|| AppError::UnsafeArchive(display.clone()))?;
    let target = root.join(&safe);
    if !target.starts_with(root) {
        return Err(AppError::UnsafeArchive(display));
    }
    if archives::suspicious(&safe) {
        executables.push(safe.display().to_string().replace('\\', "/"));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    // A later item at the same destination is the higher priority one, so it
    // replaces what an earlier item put there.
    fs::copy(source, &target)?;
    Ok(())
}

fn image_data_url(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mime = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return None,
    };
    let bytes = fs::read(path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const SCRIPT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<config>
  <moduleName>Test Mod</moduleName>
  <requiredInstallFiles>
    <file source="core\base.pak" destination="base.pak" priority="0" />
  </requiredInstallFiles>
  <installSteps order="Explicit">
    <installStep name="Pick one">
      <optionalFileGroups order="Explicit">
        <group name="Strength" type="SelectExactlyOne">
          <plugins order="Explicit">
            <plugin name="Light">
              <description>Gentle.</description>
              <conditionFlags><flag name="strength">light</flag></conditionFlags>
              <typeDescriptor><type name="Recommended" /></typeDescriptor>
            </plugin>
            <plugin name="Heavy">
              <description>Strong.</description>
              <conditionFlags><flag name="strength">heavy</flag></conditionFlags>
              <files>
                <file source="heavy\heavy.pak" destination="heavy.pak" priority="0" />
              </files>
              <typeDescriptor><type name="Optional" /></typeDescriptor>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
    <installStep name="Extras">
      <visible>
        <dependencies operator="And">
          <flagDependency flag="strength" value="heavy" />
        </dependencies>
      </visible>
      <optionalFileGroups>
        <group name="Extras" type="SelectAny">
          <plugins>
            <plugin name="Bonus">
              <files><folder source="bonus" destination="" priority="0" /></files>
              <typeDescriptor><type name="Optional" /></typeDescriptor>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
  <conditionalFileInstalls>
    <patterns>
      <pattern>
        <dependencies operator="And">
          <flagDependency flag="strength" value="light" />
        </dependencies>
        <files>
          <file source="light\light.pak" destination="light.pak" priority="0" />
        </files>
      </pattern>
    </patterns>
  </conditionalFileInstalls>
</config>"#;

    fn package() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("fomod")).unwrap();
        fs::write(root.join("fomod/ModuleConfig.xml"), SCRIPT).unwrap();
        for (path, body) in [
            ("core/base.pak", "base"),
            ("heavy/heavy.pak", "heavy"),
            ("light/light.pak", "light"),
            ("bonus/extra/thing.pak", "bonus"),
        ] {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, body).unwrap();
        }
        dir
    }

    fn answer(step: usize, plugins: &[&str]) -> StepAnswer {
        StepAnswer {
            step,
            plugins: plugins.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    #[test]
    fn locates_a_script_inside_a_wrapping_folder() {
        let dir = tempdir().unwrap();
        let inner = dir.path().join("Some Mod 1.0/fomod");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("ModuleConfig.xml"), SCRIPT).unwrap();
        assert_eq!(locate(dir.path()), Some(dir.path().join("Some Mod 1.0")));
        assert_eq!(locate(&dir.path().join("Some Mod 1.0/fomod")), None);
    }

    #[test]
    fn recommended_option_starts_selected() {
        let dir = package();
        let installer = parse(dir.path()).unwrap();
        let session = installer.session("s", &[]).unwrap();
        let step = session.step.unwrap();
        assert_eq!(step.name, "Pick one");
        assert_eq!(step.groups[0].kind, "SelectExactlyOne");
        assert!(step.groups[0].plugins[0].selected);
        assert!(!step.groups[0].plugins[1].selected);
        assert!(!session.complete);
    }

    #[test]
    fn a_hidden_step_is_skipped_and_conditional_files_follow_the_flags() {
        let dir = package();
        let installer = parse(dir.path()).unwrap();
        let answers = vec![answer(0, &["g0p0"])];
        // Choosing Light hides the Extras step, so the wizard is finished.
        assert!(installer.session("s", &answers).unwrap().complete);
        let out = tempdir().unwrap();
        installer.install(&answers, out.path()).unwrap();
        assert!(out.path().join("base.pak").is_file());
        assert!(out.path().join("light.pak").is_file());
        assert!(!out.path().join("heavy.pak").exists());
    }

    #[test]
    fn a_visible_step_is_asked_and_a_folder_keeps_its_layout() {
        let dir = package();
        let installer = parse(dir.path()).unwrap();
        let first = vec![answer(0, &["g0p1"])];
        let session = installer.session("s", &first).unwrap();
        assert_eq!(session.step.as_ref().unwrap().name, "Extras");
        assert!(!session.complete);
        let both = vec![answer(0, &["g0p1"]), answer(1, &["g0p0"])];
        let out = tempdir().unwrap();
        installer.install(&both, out.path()).unwrap();
        assert!(out.path().join("heavy.pak").is_file());
        assert!(out.path().join("extra/thing.pak").is_file());
        assert!(!out.path().join("light.pak").exists());
    }

    #[test]
    fn one_of_group_refuses_two_answers() {
        let dir = package();
        let installer = parse(dir.path()).unwrap();
        assert!(installer
            .session("s", &[answer(0, &["g0p0", "g0p1"])])
            .is_err());
        assert!(installer.session("s", &[answer(0, &[])]).is_err());
    }

    #[test]
    fn installing_before_every_question_is_answered_is_refused() {
        let dir = package();
        let out = tempdir().unwrap();
        let installer = parse(dir.path()).unwrap();
        assert!(installer
            .install(&[answer(0, &["g0p1"])], out.path())
            .is_err());
    }

    #[test]
    fn a_traversing_destination_is_refused() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("fomod")).unwrap();
        fs::write(dir.path().join("payload.pak"), "x").unwrap();
        fs::write(
            dir.path().join("fomod/ModuleConfig.xml"),
            r#"<config><moduleName>Bad</moduleName><requiredInstallFiles>
               <file source="payload.pak" destination="..\..\evil.pak" />
               </requiredInstallFiles></config>"#,
        )
        .unwrap();
        let installer = parse(dir.path()).unwrap();
        // The destination was refused while parsing, so nothing is left to
        // install and the answer is a refusal rather than a written file.
        let out = tempdir().unwrap();
        assert!(installer.install(&[], out.path()).is_err());
        assert!(!out.path().join("evil.pak").exists());
    }

    #[test]
    fn a_conditional_type_follows_earlier_answers() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("fomod")).unwrap();
        fs::write(dir.path().join("a.pak"), "a").unwrap();
        fs::write(
            dir.path().join("fomod/ModuleConfig.xml"),
            r#"<config><moduleName>C</moduleName><installSteps>
            <installStep name="One"><optionalFileGroups><group name="G" type="SelectExactlyOne">
              <plugins>
                <plugin name="Yes"><conditionFlags><flag name="k">y</flag></conditionFlags>
                  <typeDescriptor><type name="Recommended" /></typeDescriptor></plugin>
              </plugins></group></optionalFileGroups></installStep>
            <installStep name="Two"><optionalFileGroups><group name="H" type="SelectAny">
              <plugins>
                <plugin name="Gated"><files><file source="a.pak" destination="a.pak" /></files>
                  <typeDescriptor><dependencyType><defaultType name="NotUsable" /><patterns>
                    <pattern><dependencies><flagDependency flag="k" value="y" /></dependencies>
                      <type name="Required" /></pattern>
                  </patterns></dependencyType></typeDescriptor></plugin>
              </plugins></group></optionalFileGroups></installStep>
            </installSteps></config>"#,
        )
        .unwrap();
        let installer = parse(dir.path()).unwrap();
        let session = installer.session("s", &[answer(0, &["g0p0"])]).unwrap();
        let step = session.step.unwrap();
        assert_eq!(step.groups[0].plugins[0].kind, "Required");
        assert!(step.groups[0].plugins[0].selected);
        // A Required option installs whether or not the answer names it.
        let out = tempdir().unwrap();
        installer
            .install(&[answer(0, &["g0p0"]), answer(1, &[])], out.path())
            .unwrap();
        assert!(out.path().join("a.pak").is_file());
    }
}
