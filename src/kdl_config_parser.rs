// Wrapper-config parsing: turns a parsed KDL document into the `config` model.
//
// Every leaf directive is a `DirectiveParser` type that owns its parsed value
// and its error label, so parsing is declared once per directive.
// Top-level nodes dispatch on a `category:verb` name (e.g. `env:set`,
// `args:append`, `arg0:replace`); each category has a small dispatch helper.
// Values inside an op block are `-` or `value` child nodes, each carrying one
// string and an optional `transform="a,b"` property.

use std::collections::HashSet;
use std::marker::PhantomData;

use anyhow::anyhow;
use kdl::{KdlDocument, KdlEntry, KdlNode};

use crate::config::{
    Append, Arg0Behavior, ArgsDirective, ArgsSequenceUpdate, BinWrapperConfig, ChdirDirective,
    CmdDirective, CommandDirective, EnvClearDirective, EnvDirective, EnvIfInSequenceBehavior,
    EnvIfSetBehavior, EnvSequenceUpdate, EnvSetDirective, EnvUnsetDirective, InitScriptDirective,
    Prepend, ValueEntry, ValueTransformation,
};

// -------------------------------------------------------
// Shape helpers
//
// These unwrap the KDL node shape and return bare error messages;
// callers add a directive label (see `label`).

/// Prefix a bare parse error with the directive it came from.
fn label(directive: &str, err: anyhow::Error) -> anyhow::Error {
    anyhow!("{directive}: {err}")
}

/// Reject any positional argument or property.
fn check_no_args(node: &KdlNode) -> anyhow::Result<()> {
    if !node.entries().is_empty() {
        return Err(anyhow!("Node has unexpected arguments/properties"));
    }
    Ok(())
}

/// Reject any child document.
fn check_no_children(node: &KdlNode) -> anyhow::Result<()> {
    if node.children().is_some() {
        return Err(anyhow!("Node has unexpected children"));
    }
    Ok(())
}

/// Require a child document and return it.
fn get_inner_doc(node: &KdlNode) -> anyhow::Result<&KdlDocument> {
    node.children()
        .ok_or(anyhow!("Node expected to have children"))
}

/// Require exactly one positional string argument and return it.
fn get_single_arg_string(node: &KdlNode) -> anyhow::Result<&str> {
    let nb_args = node.entries().len();
    if nb_args != 1 {
        return Err(anyhow!("Node expected 1 arg, got {nb_args}"));
    }
    let arg = node.entries().first().unwrap(); // SAFETY: exactly one entry
    check_arg_is_positional(arg)?;
    get_arg_string(arg)
}

/// Reject an entry that is a property rather than a positional argument.
fn check_arg_is_positional(arg: &KdlEntry) -> anyhow::Result<()> {
    if let Some(prop_name) = arg.name() {
        return Err(anyhow!(
            "Node arg expected to be positional, got property {:?}",
            prop_name.value()
        ));
    }
    Ok(())
}

/// Require an entry to hold a string and return it.
fn get_arg_string(arg: &KdlEntry) -> anyhow::Result<&str> {
    let arg_value = arg.value();
    arg_value
        .as_string()
        .ok_or(anyhow!("Node arg has unexpected type, expected String got {arg_value:?}"))
}

/// Require a named child node inside `doc` and return it.
fn get_node<'a>(doc: &'a KdlDocument, name: &str) -> anyhow::Result<&'a KdlNode> {
    doc.get(name).ok_or(anyhow!("Missing '{name}' node"))
}

/// Collect the value children of `node`, in textual order.
///
/// A value child is either `-` or `value`: `value` is the single-item spelling,
/// `-` the list spelling, and the two may be mixed.
///
/// Errors when the node has no value children at all: a value list is required
/// wherever these are expected.
fn get_value_nodes(node: &KdlNode) -> anyhow::Result<Vec<&KdlNode>> {
    let doc = get_inner_doc(node)?;
    let value_nodes: Vec<&KdlNode> = doc
        .nodes()
        .iter()
        .filter(|child| matches!(child.name().value(), "-" | "value"))
        .collect();
    if value_nodes.is_empty() {
        return Err(anyhow!("Node is missing '-' or 'value' value children"));
    }
    Ok(value_nodes)
}

/// Reject any child of `doc` whose name is not in `allowed`.
///
/// Guards against typos and directives that belong to another op.
fn check_allowed_children(doc: &KdlDocument, allowed: &[&str]) -> anyhow::Result<()> {
    for child in doc.nodes() {
        let name = child.name().value();
        if !allowed.contains(&name) {
            return Err(anyhow!("Unexpected child node {name:?}"));
        }
    }
    Ok(())
}

/// Reject two option children sharing a name.
///
/// Value children (`-` and `value`) may repeat, so they are exempt.
fn check_no_duplicate_children(doc: &KdlDocument) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for child in doc.nodes() {
        let name = child.name().value();
        if matches!(name, "-" | "value") {
            continue;
        }
        if !seen.insert(name) {
            return Err(anyhow!("Duplicate child node {name:?}"));
        }
    }
    Ok(())
}

/// Parse a comma-separated transform list from a node's single positional string.
fn parse_transform_list(node: &KdlNode) -> anyhow::Result<Vec<ValueTransformation>> {
    parse_transform_list_str(get_single_arg_string(node)?)
}

/// Parse a comma-separated transform list from a raw string.
fn parse_transform_list_str(raw: &str) -> anyhow::Result<Vec<ValueTransformation>> {
    if raw.is_empty() {
        return Err(anyhow!("Transform list is empty"));
    }
    raw.split(',').map(parse_transform_kind).collect()
}

/// Resolve one transform kind name to its enum variant.
fn parse_transform_kind(name: &str) -> anyhow::Result<ValueTransformation> {
    match name {
        "word-split" => Ok(ValueTransformation::WordSplit),
        // `env-vars` is kept as an alias for the older `eval "env-vars"` spelling.
        "expand-env" | "env-vars" => Ok(ValueTransformation::ExpandEnv),
        other => Err(anyhow!("Unknown transformation {other:?}")),
    }
}

/// Parse an optional `default-transform "a,b"` child into a transform list.
///
/// Returns an empty list when the child is absent.
fn parse_default_transform(doc: &KdlDocument) -> anyhow::Result<Vec<ValueTransformation>> {
    match doc.get("default-transform") {
        Some(node) => parse_transform_list(node),
        None => Ok(Vec::new()),
    }
}

/// Parse a value node into a [`ValueEntry`].
///
/// The node holds exactly one positional string and at most a `transform="a,b"`
/// property. A value-level `transform` replaces `default_transforms` entirely;
/// without it, the per-op defaults apply.
fn parse_value_entry(
    node: &KdlNode,
    default_transforms: &[ValueTransformation],
) -> anyhow::Result<ValueEntry> {
    check_no_children(node)?;

    let mut text: Option<&str> = None;
    let mut transforms: Option<Vec<ValueTransformation>> = None;
    for entry in node.entries() {
        match entry.name() {
            Some(prop_name) => {
                if prop_name.value() != "transform" {
                    return Err(anyhow!(
                        "Unexpected property {:?} on value node",
                        prop_name.value()
                    ));
                }
                transforms = Some(parse_transform_list_str(get_arg_string(entry)?)?);
            }
            None => {
                if text.is_some() {
                    return Err(anyhow!("Value node expected exactly one string"));
                }
                text = Some(get_arg_string(entry)?);
            }
        }
    }

    let text = text
        .ok_or(anyhow!("Value node is missing its string"))?
        .to_string();
    let transforms = transforms.unwrap_or_else(|| default_transforms.to_vec());
    Ok(ValueEntry { text, transforms })
}

/// Parse every value child of `node` into a [`ValueEntry`], in textual order.
///
/// Applies `default_transforms` to entries that carry no own `transform=`.
fn collect_value_entries(
    node: &KdlNode,
    default_transforms: &[ValueTransformation],
) -> anyhow::Result<Vec<ValueEntry>> {
    get_value_nodes(node)?
        .into_iter()
        .map(|value_node| parse_value_entry(value_node, default_transforms))
        .collect()
}

/// Parse a required `if-set` child into its behaviour.
fn parse_if_set(doc: &KdlDocument) -> anyhow::Result<EnvIfSetBehavior> {
    let value = get_single_arg_string(get_node(doc, "if-set")?)?;
    match value {
        "replace" => Ok(EnvIfSetBehavior::Replace),
        "error" => Ok(EnvIfSetBehavior::Error),
        "skip" => Ok(EnvIfSetBehavior::Skip),
        other => Err(anyhow!("Unknown if-set behavior {other:?}")),
    }
}

/// Parse a required `if-present` child into its behaviour.
fn parse_if_present(doc: &KdlDocument) -> anyhow::Result<EnvIfInSequenceBehavior> {
    let value = get_single_arg_string(get_node(doc, "if-present")?)?;
    match value {
        "keep" => Ok(EnvIfInSequenceBehavior::Keep),
        "remove" => Ok(EnvIfInSequenceBehavior::Remove),
        "error" => Ok(EnvIfInSequenceBehavior::Error),
        "skip" => Ok(EnvIfInSequenceBehavior::Skip),
        other => Err(anyhow!("Unknown if-present behavior {other:?}")),
    }
}

// -------------------------------------------------------
// The directive trait

/// A single KDL directive that parses itself from a [`KdlNode`].
trait DirectiveParser: Sized {
    /// Directive label used to prefix parse errors (e.g. `"env:set"`).
    const NAME: &'static str;

    /// Directive-specific extraction, returning bare error messages.
    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self>;

    /// Parse `node`, decorating any error with [`Self::NAME`].
    fn parse(node: &KdlNode) -> anyhow::Result<Self> {
        Self::parse_inner(node).map_err(|err| label(Self::NAME, err))
    }
}

// -------------------------------------------------------
// Command directives

impl DirectiveParser for CommandDirective {
    const NAME: &'static str = "command";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        Ok(Self {
            program: get_single_arg_string(node)?.to_string(),
        })
    }
}

impl DirectiveParser for ChdirDirective {
    const NAME: &'static str = "chdir";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        check_no_children(node)?;
        Ok(Self {
            target_dir: get_single_arg_string(node)?.to_string(),
        })
    }
}

impl DirectiveParser for InitScriptDirective {
    const NAME: &'static str = "init-script";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        check_no_args(node)?;
        let inner_doc = get_inner_doc(node)?;

        let interpreter = {
            let node = get_node(inner_doc, "interpreter")?;
            get_single_arg_string(node)?.to_string()
        };
        let content = {
            let node = get_node(inner_doc, "content")?;
            get_single_arg_string(node)?.to_string()
        };

        Ok(Self {
            interpreter,
            content,
        })
    }
}

// -------------------------------------------------------
// Env directives

impl DirectiveParser for EnvClearDirective {
    const NAME: &'static str = "env:clear";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        check_no_args(node)?;
        check_no_children(node)?;
        Ok(Self)
    }
}

impl DirectiveParser for EnvSetDirective {
    const NAME: &'static str = "env:set";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        let var_name = get_single_arg_string(node)?.to_string();
        let doc = get_inner_doc(node)?;
        check_allowed_children(doc, &["-", "value", "sep", "if-set", "default-transform"])?;
        check_no_duplicate_children(doc)?;

        let sep = doc
            .get("sep")
            .map(|sep_node| get_single_arg_string(sep_node).map(str::to_string))
            .transpose()?;
        let if_set = parse_if_set(doc)?;
        let default_transforms = parse_default_transform(doc)?;
        let values = collect_value_entries(node, &default_transforms)?;

        // A lone value needs no separator; joining 2+ values is impossible
        // without one, so reject the ambiguous case rather than guess.
        if sep.is_none() && values.len() > 1 {
            // The `DirectiveParser::parse` wrapper already prefixes `env:set:`,
            // so the message stays free of a repeated directive name.
            return Err(anyhow!("joining multiple values requires a 'sep'"));
        }

        Ok(Self {
            var_name,
            sep,
            values,
            if_set,
        })
    }
}

impl DirectiveParser for EnvUnsetDirective {
    const NAME: &'static str = "env:unset";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        check_no_children(node)?;
        Ok(Self {
            var_name: get_single_arg_string(node)?.to_string(),
        })
    }
}

/// Shared body of the env `prepend`/`append` directives.
fn parse_env_sequence<D>(node: &KdlNode) -> anyhow::Result<EnvSequenceUpdate<D>> {
    let var_name = get_single_arg_string(node)?.to_string();
    let doc = get_inner_doc(node)?;
    check_allowed_children(doc, &["-", "value", "sep", "if-present", "default-transform"])?;
    check_no_duplicate_children(doc)?;

    let sep = get_single_arg_string(get_node(doc, "sep")?)?.to_string();
    let if_present = parse_if_present(doc)?;
    let default_transforms = parse_default_transform(doc)?;
    let values = collect_value_entries(node, &default_transforms)?;

    Ok(EnvSequenceUpdate {
        var_name,
        sep,
        values,
        if_present,
        _direction: PhantomData,
    })
}

impl DirectiveParser for EnvSequenceUpdate<Prepend> {
    const NAME: &'static str = "env:prepend";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        parse_env_sequence(node)
    }
}

impl DirectiveParser for EnvSequenceUpdate<Append> {
    const NAME: &'static str = "env:append";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        parse_env_sequence(node)
    }
}

// -------------------------------------------------------
// Args directives

/// Shared body of the args `prepend`/`append` directives.
fn parse_args_sequence<D>(node: &KdlNode) -> anyhow::Result<ArgsSequenceUpdate<D>> {
    check_no_args(node)?;
    let doc = get_inner_doc(node)?;
    check_allowed_children(doc, &["-", "value", "default-transform"])?;
    check_no_duplicate_children(doc)?;

    let default_transforms = parse_default_transform(doc)?;
    let values = collect_value_entries(node, &default_transforms)?;

    Ok(ArgsSequenceUpdate {
        values,
        _direction: PhantomData,
    })
}

impl DirectiveParser for ArgsSequenceUpdate<Prepend> {
    const NAME: &'static str = "args:prepend";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        parse_args_sequence(node)
    }
}

impl DirectiveParser for ArgsSequenceUpdate<Append> {
    const NAME: &'static str = "args:append";

    fn parse_inner(node: &KdlNode) -> anyhow::Result<Self> {
        parse_args_sequence(node)
    }
}

// -------------------------------------------------------
// Category dispatch

/// Parse an `env:<verb>` node into its directive.
fn parse_env_op(verb: &str, node: &KdlNode) -> anyhow::Result<EnvDirective> {
    match verb {
        "clear" => Ok(EnvDirective::Clear(EnvClearDirective::parse(node)?)),
        "set" => Ok(EnvDirective::Set(EnvSetDirective::parse(node)?)),
        "unset" => Ok(EnvDirective::Unset(EnvUnsetDirective::parse(node)?)),
        "prepend" => Ok(EnvDirective::Prepend(EnvSequenceUpdate::<Prepend>::parse(node)?)),
        "append" => Ok(EnvDirective::Append(EnvSequenceUpdate::<Append>::parse(node)?)),
        other => Err(label("env", anyhow!("Unknown op {other:?}"))),
    }
}

/// Parse an `args:<verb>` node into its directive.
fn parse_args_op(verb: &str, node: &KdlNode) -> anyhow::Result<ArgsDirective> {
    match verb {
        "prepend" => Ok(ArgsDirective::Prepend(ArgsSequenceUpdate::<Prepend>::parse(node)?)),
        "append" => Ok(ArgsDirective::Append(ArgsSequenceUpdate::<Append>::parse(node)?)),
        other => Err(label("args", anyhow!("Unknown op {other:?}"))),
    }
}

/// Parse an `arg0:<verb>` node into its behaviour.
fn parse_arg0_op(verb: &str, node: &KdlNode) -> anyhow::Result<Arg0Behavior> {
    match verb {
        "inherit" => {
            check_no_args(node)?;
            check_no_children(node)?;
            Ok(Arg0Behavior::Inherit)
        }
        "replace" => {
            check_no_children(node)?;
            Ok(Arg0Behavior::Replace(get_single_arg_string(node)?.to_string()))
        }
        other => Err(label("arg0", anyhow!("Unknown op {other:?}"))),
    }
}

// -------------------------------------------------------
// Document assembler

/// Parse a full wrapper config from a KDL document.
///
/// Requires a `command` directive; other directives are collected per category
/// in textual order. Top-level nodes are either bare (`command`, `chdir`) or
/// namespaced (`env:set`, `args:append`, `arg0:replace`).
pub(crate) fn parse_bin_wrapper_config(doc: &KdlDocument) -> anyhow::Result<BinWrapperConfig> {
    let mut command: Option<CommandDirective> = None;
    let mut arg0_behavior: Option<Arg0Behavior> = None;
    let mut cmd_directives = Vec::new();
    let mut env_directives = Vec::new();
    let mut args_directives = Vec::new();

    for node in doc.nodes() {
        let name = node.name().value();
        match name {
            "command" => command = Some(CommandDirective::parse(node)?),
            "chdir" => cmd_directives.push(CmdDirective::Chdir(ChdirDirective::parse(node)?)),
            _ => {
                // Without a colon this is a bare `env`/`args` block — a leftover
                // from the old syntax — or an unknown bare directive.
                let Some((category, verb)) = name.split_once(':') else {
                    return Err(anyhow!("Unknown directive {name:?} 👀"));
                };
                match category {
                    "env" => env_directives.push(parse_env_op(verb, node)?),
                    "args" => args_directives.push(parse_args_op(verb, node)?),
                    "arg0" => arg0_behavior = Some(parse_arg0_op(verb, node)?),
                    other => return Err(anyhow!("Unknown category {other:?} in {name:?}")),
                }
            }
        }
    }

    let Some(command) = command else {
        return Err(anyhow!("Missing command!"));
    };

    Ok(BinWrapperConfig {
        command,
        arg0_behavior: arg0_behavior.unwrap_or(Arg0Behavior::Inherit),
        env_directives,
        cmd_directives,
        args_directives,
    })
}

// -------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::*;
    use crate::config::{
        Arg0Behavior, ArgsDirective, ArgsSequenceUpdate, ChdirDirective, CmdDirective,
        CommandDirective, EnvClearDirective, EnvDirective, EnvIfInSequenceBehavior,
        EnvIfSetBehavior, EnvSequenceUpdate, EnvSetDirective, EnvUnsetDirective, InitScriptDirective,
        ValueEntry, ValueTransformation,
    };

    macro_rules! assert_ok {
        ($expr:expr) => {
            match $expr {
                Ok(value) => value,
                Err(err) => panic!("expected Ok, got Err({err:?})"),
            }
        };
    }

    macro_rules! assert_err {
        ($expr:expr) => {
            match $expr {
                Ok(value) => panic!("expected Err, got Ok({value:?})"),
                Err(err) => format!("{err}"),
            }
        };
    }

    #[test]
    fn parse_cmd_op_chdir() {
        let input = r#"
            command "supercommand!"
            chdir "foobar"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.command,
            CommandDirective {
                program: "supercommand!".into()
            }
        );
        assert_eq!(
            config.cmd_directives,
            &[CmdDirective::Chdir(ChdirDirective {
                target_dir: "foobar".into()
            })]
        );
    }

    #[test]
    fn parse_cmd_op_init_script() {
        // `init-script` is dormant: it is no longer reachable from the document
        // dispatch, so its directive parser is exercised directly.
        let input = r#"
            init-script {
                interpreter "someshell"
                content """
                    echo 'foo!'
                    echo 'and bar!'
                    """
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let node = doc.get("init-script").unwrap();
        let directive = assert_ok!(InitScriptDirective::parse(node));

        assert_eq!(
            directive,
            InitScriptDirective {
                interpreter: "someshell".into(),
                content: "echo 'foo!'\necho 'and bar!'".into(),
            }
        );
    }

    #[test]
    fn parse_cmd_args() {
        let input = r#"
            command "supercommand!"

            args:prepend {
                - "first"
                - "second"
                - "third"
            }
            args:append {
                default-transform "word-split"
                - "foo bar baz"
                - "hello world"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.command,
            CommandDirective {
                program: "supercommand!".into()
            }
        );
        assert_eq!(
            config.args_directives,
            &[
                ArgsDirective::Prepend(ArgsSequenceUpdate {
                    values: vec![
                        ValueEntry {
                            text: "first".into(),
                            transforms: vec![],
                        },
                        ValueEntry {
                            text: "second".into(),
                            transforms: vec![],
                        },
                        ValueEntry {
                            text: "third".into(),
                            transforms: vec![],
                        },
                    ],
                    _direction: PhantomData,
                }),
                ArgsDirective::Append(ArgsSequenceUpdate {
                    values: vec![
                        ValueEntry {
                            text: "foo bar baz".into(),
                            transforms: vec![ValueTransformation::WordSplit],
                        },
                        ValueEntry {
                            text: "hello world".into(),
                            transforms: vec![ValueTransformation::WordSplit],
                        },
                    ],
                    _direction: PhantomData,
                }),
            ]
        );
    }

    #[test]
    fn parse_arg0_replace() {
        let input = r#"
            command "supercommand!"
            arg0:replace "custom-bin"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));
        assert_eq!(config.arg0_behavior, Arg0Behavior::Replace("custom-bin".into()));
    }

    #[test]
    fn parse_arg0_inherit() {
        let input = r#"
            command "supercommand!"
            arg0:inherit
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));
        assert_eq!(config.arg0_behavior, Arg0Behavior::Inherit);
    }

    #[test]
    fn parse_arg0_defaults_to_inherit() {
        let input = r#"
            command "supercommand!"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));
        assert_eq!(config.arg0_behavior, Arg0Behavior::Inherit);
    }

    #[test]
    fn parse_env_sequence_ops() {
        let input = r#"
            command "supercommand!"
            env:prepend "PATH" {
                sep ":"
                - "new/path/bin"
                - "/opt/extra/bin"
                if-present "keep"
            }
            env:append "PATH" {
                sep ":"
                - "tail/bin"
                if-present "skip"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.env_directives,
            &[
                EnvDirective::Prepend(EnvSequenceUpdate {
                    var_name: "PATH".into(),
                    sep: ":".into(),
                    values: vec![
                        ValueEntry {
                            text: "new/path/bin".into(),
                            transforms: vec![],
                        },
                        ValueEntry {
                            text: "/opt/extra/bin".into(),
                            transforms: vec![],
                        },
                    ],
                    if_present: EnvIfInSequenceBehavior::Keep,
                    _direction: PhantomData,
                }),
                EnvDirective::Append(EnvSequenceUpdate {
                    var_name: "PATH".into(),
                    sep: ":".into(),
                    values: vec![ValueEntry {
                        text: "tail/bin".into(),
                        transforms: vec![],
                    }],
                    if_present: EnvIfInSequenceBehavior::Skip,
                    _direction: PhantomData,
                }),
            ]
        );
    }

    #[test]
    fn parse_env_set_op() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                value "some-dir:$ZSH_CONFIG_DIR" transform="expand-env"
                if-set "replace"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.env_directives,
            &[EnvDirective::Set(EnvSetDirective {
                var_name: "FOO".into(),
                sep: None,
                values: vec![ValueEntry {
                    text: "some-dir:$ZSH_CONFIG_DIR".into(),
                    transforms: vec![ValueTransformation::ExpandEnv],
                }],
                if_set: EnvIfSetBehavior::Replace,
            })]
        );
    }

    #[test]
    fn parse_env_set_multi_value_with_sep() {
        let input = r#"
            command "supercommand!"
            env:set "PATH" {
                sep ":"
                - "/a/bin"
                - "/b/bin"
                if-set "replace"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.env_directives,
            &[EnvDirective::Set(EnvSetDirective {
                var_name: "PATH".into(),
                sep: Some(":".into()),
                values: vec![
                    ValueEntry {
                        text: "/a/bin".into(),
                        transforms: vec![],
                    },
                    ValueEntry {
                        text: "/b/bin".into(),
                        transforms: vec![],
                    },
                ],
                if_set: EnvIfSetBehavior::Replace,
            })]
        );
    }

    #[test]
    fn parse_value_single_item_shorthand() {
        let input = r#"
            command "supercommand!"
            env:prepend "PATH" {
                sep ":"
                value "/a/bin"
                if-present "keep"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.env_directives,
            &[EnvDirective::Prepend(EnvSequenceUpdate {
                var_name: "PATH".into(),
                sep: ":".into(),
                values: vec![ValueEntry {
                    text: "/a/bin".into(),
                    transforms: vec![],
                }],
                if_present: EnvIfInSequenceBehavior::Keep,
                _direction: PhantomData,
            })]
        );
    }

    #[test]
    fn parse_env_clear_and_unset() {
        let input = r#"
            command "supercommand!"
            env:clear
            env:unset "HOME"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.env_directives,
            &[
                EnvDirective::Clear(EnvClearDirective),
                EnvDirective::Unset(EnvUnsetDirective {
                    var_name: "HOME".into()
                }),
            ]
        );
    }

    #[test]
    fn parse_value_transform_overrides_default() {
        let input = r#"
            command "supercommand!"
            args:append {
                default-transform "word-split,expand-env"
                - "split me"
                - "literal" transform="expand-env"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let config = assert_ok!(parse_bin_wrapper_config(&doc));

        assert_eq!(
            config.args_directives,
            &[ArgsDirective::Append(ArgsSequenceUpdate {
                values: vec![
                    ValueEntry {
                        text: "split me".into(),
                        transforms: vec![
                            ValueTransformation::WordSplit,
                            ValueTransformation::ExpandEnv,
                        ],
                    },
                    ValueEntry {
                        text: "literal".into(),
                        transforms: vec![ValueTransformation::ExpandEnv],
                    },
                ],
                _direction: PhantomData,
            })]
        );
    }

    #[test]
    fn rejects_unknown_directive() {
        let input = r#"
            unknown-thing "value"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown directive"), "{err:?}");
    }

    #[test]
    fn rejects_missing_command() {
        let input = r#"
            chdir "foobar"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Missing command"), "{err:?}");
    }

    #[test]
    fn rejects_non_string_argument() {
        let input = r#"
            command 42
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("unexpected type"), "{err:?}");
    }

    #[test]
    fn rejects_property_where_positional_expected() {
        let input = r#"
            command prop="value"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(
            err,
            "command: Node arg expected to be positional, got property \"prop\""
        );
    }

    #[test]
    fn rejects_chdir_with_children() {
        let input = r#"
            command "supercommand!"
            chdir "/tmp" {
                nested "x"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("unexpected children"), "{err:?}");
    }

    #[test]
    fn rejects_bare_env_block() {
        let input = r#"
            command "supercommand!"
            env {
                set "Y" {
                    value "v"
                    if-set "replace"
                }
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown directive"), "{err:?}");
    }

    #[test]
    fn rejects_unknown_category() {
        let input = r#"
            command "supercommand!"
            foo:bar "x"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown category"), "{err:?}");
    }

    #[test]
    fn rejects_unknown_env_op() {
        let input = r#"
            command "supercommand!"
            env:bogus "x"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown op"), "{err:?}");
    }

    #[test]
    fn rejects_unknown_args_op() {
        let input = r#"
            command "supercommand!"
            args:bogus {
                - "x"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown op"), "{err:?}");
    }

    #[test]
    fn rejects_unknown_arg0_op() {
        let input = r#"
            command "supercommand!"
            arg0:bogus
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown op"), "{err:?}");
    }

    #[test]
    fn rejects_args_op_with_arguments() {
        let input = r#"
            command "supercommand!"
            args:prepend "oops" {
                - "first"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("unexpected arguments"), "{err:?}");
    }

    #[test]
    fn rejects_args_op_without_children() {
        let input = r#"
            command "supercommand!"
            args:prepend
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("expected to have children"), "{err:?}");
    }

    #[test]
    fn rejects_args_op_without_values() {
        let input = r#"
            command "supercommand!"
            args:prepend {
                default-transform "word-split"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(
            err,
            "args:prepend: Node is missing '-' or 'value' value children"
        );
    }

    #[test]
    fn rejects_env_sequence_missing_sep() {
        let input = r#"
            command "supercommand!"
            env:prepend "PATH" {
                - "x"
                if-present "keep"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(err, "env:prepend: Missing 'sep' node");
    }

    #[test]
    fn rejects_env_set_missing_value() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                if-set "replace"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(
            err,
            "env:set: Node is missing '-' or 'value' value children"
        );
    }

    #[test]
    fn rejects_env_set_missing_if_set() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                value "v"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(err, "env:set: Missing 'if-set' node");
    }

    #[test]
    fn rejects_unknown_if_set_behavior() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                value "v"
                if-set "bogus"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown if-set"), "{err:?}");
    }

    #[test]
    fn rejects_unknown_transform() {
        let input = r#"
            command "supercommand!"
            args:append {
                - "first" transform="nope"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown transformation"), "{err:?}");
    }

    #[test]
    fn rejects_unexpected_child() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                value "v"
                if-set "replace"
                bogus "z"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unexpected child node"), "{err:?}");
    }

    #[test]
    fn rejects_duplicate_option_child() {
        let input = r#"
            command "supercommand!"
            env:set "FOO" {
                value "a"
                if-set "replace"
                if-set "skip"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Duplicate child node"), "{err:?}");
    }

    #[test]
    fn rejects_env_set_multi_value_without_sep() {
        let input = r#"
            command "supercommand!"
            env:set "PATH" {
                - "/a/bin"
                - "/b/bin"
                if-set "replace"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert_eq!(err, "env:set: joining multiple values requires a 'sep'");
    }

    #[test]
    fn rejects_arg0_replace_without_argument() {
        let input = r#"
            command "supercommand!"
            arg0:replace
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("expected 1 arg"), "{err:?}");
    }

    #[test]
    fn rejects_init_script_missing_interpreter() {
        let input = r#"
            init-script {
                content "echo 'foo!'"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let node = doc.get("init-script").unwrap();
        let err = assert_err!(InitScriptDirective::parse(node));
        assert_eq!(err, "init-script: Missing 'interpreter' node");
    }

    #[test]
    fn parses_shipped_samples() {
        // Regression guard: the shipped samples must stay parseable as the
        // surface evolves. A leading shebang is part of the wrapper-script
        // convention, so it is stripped before parsing.
        for path in [
            "sample-configs/full-config.kdl",
            "sample-configs/simple-echo-surround.kdl",
            "sample-configs/full-config-alt-colon-namespace-lists.kdl",
        ] {
            let text = std::fs::read_to_string(path).unwrap();
            let body = match text.starts_with("#!") {
                true => text.split_once('\n').unwrap().1,
                false => text.as_str(),
            };
            let doc: KdlDocument = body.parse().unwrap_or_else(|err| panic!("{path}: {err}"));
            parse_bin_wrapper_config(&doc).unwrap_or_else(|err| panic!("{path}: {err}"));
        }
    }
}
