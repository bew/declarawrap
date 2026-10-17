// Wrapper-config parsing: turns a parsed KDL document into the `config` model.
//
// Every leaf directive is a `DirectiveParser` type that owns its parsed value
// and its error label, so parsing is declared once per directive.
// The `env` and `args` block containers are plain assembler helpers that
// dispatch children.

use std::collections::HashMap;
use std::marker::PhantomData;

use anyhow::anyhow;
use kdl::{KdlDocument, KdlEntry, KdlNode};

use crate::config::{
    Append, Arg0Behavior, ArgsDirective, ArgsSequenceUpdate, ArgsTransformation, BinWrapperConfig,
    ChdirDirective, CmdDirective, CommandDirective, EnvClearDirective, EnvDirective,
    EnvSequenceUpdate, EnvSetDirective, EnvUnsetDirective, InitScriptDirective, Prepend,
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
            "Node arg expected to be positional, got property {prop_name:?}"
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

/// Split a node's entries into positional arguments and named properties.
fn split_args_props(node: &KdlNode) -> anyhow::Result<(Vec<&str>, HashMap<&str, &str>)> {
    let mut positionals = Vec::new();
    let mut properties = HashMap::new();
    for arg in node.entries() {
        let value = get_arg_string(arg)?;
        match arg.name() {
            Some(prop_name) => {
                properties.insert(prop_name.value(), value);
            }
            None => {
                positionals.push(value);
            }
        }
    }
    Ok((positionals, properties))
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
//
// The env syntax is not settled yet, so the env bodies stay unimplemented.

impl DirectiveParser for EnvClearDirective {
    const NAME: &'static str = "env:clear";

    fn parse_inner(_node: &KdlNode) -> anyhow::Result<Self> {
        todo!()
    }
}

impl DirectiveParser for EnvSetDirective {
    const NAME: &'static str = "env:set";

    fn parse_inner(_node: &KdlNode) -> anyhow::Result<Self> {
        todo!()
    }
}

impl DirectiveParser for EnvUnsetDirective {
    const NAME: &'static str = "env:unset";

    fn parse_inner(_node: &KdlNode) -> anyhow::Result<Self> {
        todo!()
    }
}

impl DirectiveParser for EnvSequenceUpdate<Prepend> {
    const NAME: &'static str = "env:prepend";

    fn parse_inner(_node: &KdlNode) -> anyhow::Result<Self> {
        todo!()
    }
}

impl DirectiveParser for EnvSequenceUpdate<Append> {
    const NAME: &'static str = "env:append";

    fn parse_inner(_node: &KdlNode) -> anyhow::Result<Self> {
        todo!()
    }
}

// -------------------------------------------------------
// Args directives

/// Shared body of the args `prepend`/`append` directives.
fn parse_args_sequence<D>(node: &KdlNode) -> anyhow::Result<ArgsSequenceUpdate<D>> {
    let (args, props) = split_args_props(node)?;
    if args.is_empty() {
        return Err(anyhow!("Node is missing arguments"));
    }

    let arg_strings = args.iter().map(|s| (*s).into()).collect();
    let transformation = if props.is_empty() {
        None
    } else {
        let prop_value = props.get("transform");
        match prop_value {
            Some(&"word-split") => Some(ArgsTransformation::WordSplit),
            _ => return Err(anyhow!("Unknown transformation {prop_value:?}")),
        }
    };

    Ok(ArgsSequenceUpdate {
        arg_strings,
        transformation,
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
// Block containers

/// Parse an `env { … }` block into its directives, in textual order.
fn parse_env_block(node: &KdlNode) -> anyhow::Result<Vec<EnvDirective>> {
    check_no_args(node).map_err(|err| label("env", err))?;
    let inner_doc = get_inner_doc(node).map_err(|err| label("env", err))?;

    let mut directives = Vec::new();
    for child in inner_doc.nodes() {
        let directive = match child.name().value() {
            "clear" => EnvDirective::Clear(EnvClearDirective::parse(child)?),
            "set" => EnvDirective::Set(EnvSetDirective::parse(child)?),
            "unset" => EnvDirective::Unset(EnvUnsetDirective::parse(child)?),
            "prepend" => EnvDirective::Prepend(EnvSequenceUpdate::<Prepend>::parse(child)?),
            "append" => EnvDirective::Append(EnvSequenceUpdate::<Append>::parse(child)?),
            verb => return Err(label("env", anyhow!("Unhandled directive {verb:?}"))),
        };
        directives.push(directive);
    }
    Ok(directives)
}

/// Parse an `args { … }` block into its directives, in textual order.
fn parse_args_block(node: &KdlNode) -> anyhow::Result<Vec<ArgsDirective>> {
    check_no_args(node).map_err(|err| label("args", err))?;
    let inner_doc = get_inner_doc(node).map_err(|err| label("args", err))?;

    let mut directives = Vec::new();
    for child in inner_doc.nodes() {
        let directive = match child.name().value() {
            "prepend" => ArgsDirective::Prepend(ArgsSequenceUpdate::<Prepend>::parse(child)?),
            "append" => ArgsDirective::Append(ArgsSequenceUpdate::<Append>::parse(child)?),
            verb => return Err(label("args", anyhow!("Unhandled directive {verb:?}"))),
        };
        directives.push(directive);
    }
    Ok(directives)
}

// -------------------------------------------------------
// Document assembler

/// Parse a full wrapper config from a KDL document.
///
/// Requires a `command` directive; other directives are collected per category
/// in textual order.
pub(crate) fn parse_bin_wrapper_config(doc: &KdlDocument) -> anyhow::Result<BinWrapperConfig> {
    let mut command: Option<CommandDirective> = None;
    let mut cmd_directives = Vec::new();
    let mut env_directives = Vec::new();
    let mut args_directives = Vec::new();

    for node in doc.nodes() {
        match node.name().value() {
            "command" => command = Some(CommandDirective::parse(node)?),
            "chdir" => cmd_directives.push(CmdDirective::Chdir(ChdirDirective::parse(node)?)),
            "init-script" => {
                cmd_directives.push(CmdDirective::InitScript(InitScriptDirective::parse(node)?));
            }
            "args" => args_directives.extend(parse_args_block(node)?),
            "env" => env_directives.extend(parse_env_block(node)?),
            directive => return Err(anyhow!("Unknown directive {directive:?} 👀")),
        }
    }

    let Some(command) = command else {
        return Err(anyhow!("Missing command!"));
    };

    Ok(BinWrapperConfig {
        command,
        arg0_behavior: Arg0Behavior::Inherit,
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
        ArgsDirective, ArgsSequenceUpdate, ArgsTransformation, ChdirDirective, CmdDirective,
        CommandDirective, InitScriptDirective,
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
        let input = r#"
            command "supercommand!"
            init-script {
                interpreter "someshell"
                content """
                    echo 'foo!'
                    echo 'and bar!'
                    """
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
            config.cmd_directives,
            &[CmdDirective::InitScript(InitScriptDirective {
                interpreter: "someshell".into(),
                content: "echo 'foo!'\necho 'and bar!'".into(),
            })]
        );
    }

    #[test]
    fn parse_cmd_args() {
        let input = r###"
            command "supercommand!"

            args {
                prepend "first" "second" "third"
                append transform="word-split" "first second" "third fourth fifth"
            }
        "###;
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
                    arg_strings: vec!["first".into(), "second".into(), "third".into()],
                    transformation: None,
                    _direction: PhantomData,
                }),
                ArgsDirective::Append(ArgsSequenceUpdate {
                    arg_strings: vec!["first second".into(), "third fourth fifth".into()],
                    transformation: Some(ArgsTransformation::WordSplit),
                    _direction: PhantomData,
                }),
            ]
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
        assert!(err.contains("positional"), "{err:?}");
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
    fn rejects_args_with_arguments() {
        let input = r#"
            command "supercommand!"
            args "oops" {
                prepend "first"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("unexpected arguments"), "{err:?}");
    }

    #[test]
    fn rejects_args_without_children() {
        let input = r#"
            command "supercommand!"
            args
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("expected to have children"), "{err:?}");
    }

    #[test]
    fn rejects_args_op_without_arguments() {
        let input = r#"
            command "supercommand!"
            args {
                prepend
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("missing arguments"), "{err:?}");
    }

    #[test]
    fn rejects_args_op_with_unknown_transform() {
        let input = r#"
            command "supercommand!"
            args {
                append transform="nope" "first"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("Unknown transformation"), "{err:?}");
    }

    #[test]
    fn rejects_init_script_missing_interpreter() {
        let input = r#"
            command "supercommand!"
            init-script {
                content "echo 'foo!'"
            }
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let err = assert_err!(parse_bin_wrapper_config(&doc));
        assert!(err.contains("interpreter"), "{err:?}");
    }
}
