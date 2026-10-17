use std::collections::HashMap;

use anyhow::anyhow;
use kdl::{KdlDocument, KdlEntry, KdlNode};

use crate::config::{
    Arg0Behavior, ArgsOperation, ArgsSequenceUpdate, ArgsTransformation, BinWrapperConfig, CmdOperation, EnvOperation
};

fn check_no_args(node: &KdlNode, directive: &str) -> anyhow::Result<()> {
    match node.entries().len() {
        0 => Ok(()),
        _ => Err(anyhow!("{directive}: Node has unexpected arguments/properties")),
    }
}

fn check_no_children(node: &KdlNode, directive: &str) -> anyhow::Result<()> {
    match node.children() {
        Some(_) => Err(anyhow!("{directive}: Node has unexpected children")),
        None => Ok(()),
    }
}

fn get_inner_doc<'a>(node: &'a KdlNode, directive: &str) -> anyhow::Result<&'a KdlDocument> {
    node.children().ok_or(anyhow!("{directive}: Node expected to have children"))
}

fn get_node<'a>(doc: &'a KdlDocument, node_name: &str, directive: &str) -> anyhow::Result<&'a KdlNode> {
    doc.get(node_name).ok_or(anyhow!("{directive}: Missing '{node_name}' node"))
}

fn get_single_arg_string<'a>(node: &'a KdlNode, directive: &str) -> anyhow::Result<&'a str> {
    let nb_args = node.entries().len();
    if nb_args != 1 {
        return Err(anyhow!("{directive}: Node expected 1 arg, got {nb_args}"))
    }
    let arg = node.entries().first().unwrap(); // SAFETY: we know there's one!
    check_arg_is_positional(arg, directive)?;
    get_arg_string(arg, directive)
}

fn check_arg_is_positional(arg: &KdlEntry, directive: &str) -> anyhow::Result<()> {
    if let Some(prop_name) = arg.name() {
        return Err(anyhow!("{directive}: Node arg expected to be positional, got property {prop_name:?}"))
    }
    Ok(())
}

fn get_arg_string<'a>(arg: &'a KdlEntry, directive: &str) -> anyhow::Result<&'a str> {
    let arg_value = arg.value();
    arg_value.as_string().ok_or(
        anyhow!("{directive}: Node arg has unexpected type, expected String got {arg_value:?}"),
    )
}

fn split_args_props<'a>(node: &'a KdlNode, directive: &str) -> anyhow::Result<(Vec<&'a str>, HashMap<&'a str, &'a str>)> {
    let mut vec = Vec::new();
    let mut map = HashMap::new();
    for arg in node.entries() {
        let value = get_arg_string(arg, directive)?;
        match arg.name() {
            Some(prop_name) => {
                map.insert(prop_name.value(), value);
            }
            _ => {
                vec.push(value);
            }
        }
    }
    Ok((vec, map))
}

impl TryFrom<KdlNode> for ArgsSequenceUpdate {
    type Error = anyhow::Error;

    fn try_from(node: KdlNode) -> Result<Self, Self::Error> {
        let directive = node.name().value().to_string();

        // args look like:
        // - `node "arg1" "arg2" "arg3" ...`
        // - `node "arg1 arg2 arg3" transform="word-split"`
        // - `node transform="word-split" "arg1 arg2 arg3"`
        let (args, props) = split_args_props(&node, "args > *")?;
        if args.len() == 0 {
            return Err(anyhow!("args: Node is missing arguments"))
        }
        let arg_strings = args.iter().map(|s| (*s).into()).collect();
        if props.len() == 0 {
            Ok(ArgsSequenceUpdate {
                arg_strings,
                transformation: None,
            })
        } else {
            let prop_value = props.get("transform");
            let transformation = match prop_value {
                Some(&"word-split") => Some(ArgsTransformation::WordSplit),
                _ => {
                    return Err(anyhow!("{directive}: Unknown transformation {prop_value:?}"))
                }
            };
            Ok(ArgsSequenceUpdate {
                arg_strings,
                transformation,
            })
        }
    }
}

impl TryFrom<KdlNode> for ArgsOperation {
    type Error = anyhow::Error;

    fn try_from(node: KdlNode) -> Result<Self, Self::Error> {
        let op = match node.name().value() {
            "prepend" => {
                let arg_seq_update = ArgsSequenceUpdate::try_from(node)?;
                ArgsOperation::Prepend(arg_seq_update)
            }
            "append" => {
                let arg_seq_update = ArgsSequenceUpdate::try_from(node)?;
                ArgsOperation::Append(arg_seq_update)
            }
            directive => {
                let type_name = std::any::type_name::<Self>();
                Err(anyhow!("args: Unhandled directive {directive:?} for {ty}", ty=type_name))?
            }
        };
        Ok(op)
    }
}

impl TryFrom<KdlNode> for EnvOperation {
    type Error = anyhow::Error;

    fn try_from(node: KdlNode) -> Result<Self, Self::Error> {
        let op = match node.name().value() {
            "clear" => todo!(),
            "set" => todo!(),
            "unset" => todo!(),
            "append" => todo!(),
            "prepend" => todo!(),
            directive => {
                let type_name = std::any::type_name::<Self>();
                Err(anyhow!("env: Unhandled directive {directive:?} for {ty}", ty=type_name))?
            }
        };
        Ok(op)
    }
}

impl TryFrom<KdlDocument> for BinWrapperConfig {
    type Error = anyhow::Error;

    fn try_from(mut doc: KdlDocument) -> Result<Self, Self::Error> {
        let mut command = None;
        let mut cmd_operations = Vec::new();
        let mut env_operations = Vec::new();
        let mut args_operations = Vec::new();

        for node in doc.nodes_mut() {
            let directive = node.name().value().to_string();
            let directive = directive.as_ref();
            match directive {
                "command" => {
                    command = Some(get_single_arg_string(node, directive)?);
                }
                "chdir" => {
                    check_no_children(&node, directive)?;
                    let dir = get_single_arg_string(&node, directive)?;
                    let op = CmdOperation::Chdir(dir.into());
                    cmd_operations.push(op);
                }
                "init-script" => {
                    check_no_args(&node, directive)?;
                    let inner_doc = get_inner_doc(node, directive)?;
                    let interp_node = get_node(&inner_doc, "interpreter", directive)?;
                    let content_node = get_node(&inner_doc, "content", directive)?;
                    let interpreter = get_single_arg_string(interp_node, "init-script > interpreter")?;
                    let content = get_single_arg_string(content_node, "init-script > content")?;
                    let op = CmdOperation::InitScript {
                        interpreter: interpreter.into(),
                        content: content.into(),
                    };
                    cmd_operations.push(op);
                }
                "args" => {
                    check_no_args(&node, "args")?;
                    let inner_doc = get_inner_doc(node, "args")?;
                    for child_node in inner_doc.nodes() {
                        let op = ArgsOperation::try_from(child_node.clone())?;
                        args_operations.push(op);
                    }
                }
                "env" => {
                    check_no_args(&node, "env")?;
                    let inner_doc = get_inner_doc(node, "env")?;
                    for child_node in inner_doc.nodes() {
                        let op = EnvOperation::try_from(child_node.clone())?;
                        env_operations.push(op);
                    }
                }
                directive => {
                    // TODO: test error cases!
                    Err(anyhow!("Unknown directive {directive:?} 👀"))?;
                }
            }
        }

        let Some(command) = command.take() else {
            return Err(anyhow!("Missing command!"));
        };

        Ok(BinWrapperConfig {
            command: command.into(),
            arg0_behavior: Arg0Behavior::Inherit,
            env_operations,
            cmd_operations,
            args_operations,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // Macro inspired from assertables::assert_ok, but with PRETTY print of the error
    macro_rules! assert_ok {
        ($x: expr) => {
            match ($x) {
                Ok(inner) => inner,
                Err(err) => {
                    panic!(
                        concat!(
                            "Assertion failed: `assert_ok!({})`\n",
                            "Got Err: {:#?}",
                        ),
                        stringify!($x),
                        err
                    )
                }
            }
        }
    }

    #[test]
    fn parse_cmd_op_chdir() {
        let input = r#"
            command "supercommand!"
            chdir "foobar"
        "#;
        let doc: KdlDocument = assert_ok!(input.parse());
        let wrapper_config = assert_ok!(BinWrapperConfig::try_from(doc));
        assert_eq!(wrapper_config.command, String::from("supercommand!"));
        assert_eq!(wrapper_config.cmd_operations, &[
            CmdOperation::Chdir("foobar".into()),
        ]);
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
        let wrapper_config = assert_ok!(BinWrapperConfig::try_from(doc));
        assert_eq!(wrapper_config.command, String::from("supercommand!"));
        assert_eq!(wrapper_config.cmd_operations, &[
            CmdOperation::InitScript {
                interpreter: "someshell".into(),
                content: "echo 'foo!'\necho 'and bar!'".into(),
            }
        ]);
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
        let wrapper_config = assert_ok!(BinWrapperConfig::try_from(doc));
        assert_eq!(wrapper_config.command, String::from("supercommand!"));
        assert_eq!(wrapper_config.args_operations, &[
            ArgsOperation::Prepend(
                ArgsSequenceUpdate {
                    arg_strings: vec![
                        "first".into(),
                        "second".into(),
                        "third".into(),
                    ],
                    transformation: None,
                }
            ),
            ArgsOperation::Append(
                ArgsSequenceUpdate {
                    arg_strings: vec![
                        "first second".into(),
                        "third fourth fifth".into(),
                    ],
                    transformation: Some(ArgsTransformation::WordSplit),
                }
            ),
        ]);
    }
}
