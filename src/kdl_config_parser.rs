use anyhow::anyhow;
use kdl::{KdlDocument, KdlNode};

use crate::config::{BinWrapperConfig, CmdOperation};

fn check_no_args(node: &KdlNode, directive_name: &str) -> anyhow::Result<()> {
    match node.entries().len() {
        0 => Ok(()),
        _ => Err(anyhow!("{directive_name}: Node has unexpected arguments/properties")),
    }
}

fn check_no_children(node: &KdlNode, directive_name: &str) -> anyhow::Result<()> {
    match node.children() {
        Some(_) => Err(anyhow!("{directive_name}: Node has unexpected children")),
        None => Ok(()),
    }
}

fn get_children<'a>(node: &'a KdlNode, directive_name: &str) -> anyhow::Result<&'a KdlDocument> {
    node.children().ok_or(anyhow!("{directive_name}: Node has unexpected children"))
}

fn get_node<'a>(doc: &'a KdlDocument, node_name: &str, directive_name: &str) -> anyhow::Result<&'a KdlNode> {
    doc.get(node_name).ok_or(anyhow!("{directive_name}: Missing '{node_name}' node"))
}

fn get_single_arg_string<'a>(node: &'a KdlNode, directive_name: &str) -> anyhow::Result<&'a str> {
    let nb_args = node.entries().len();
    if nb_args != 1 {
        return Err(anyhow!("{directive_name}: Node expected 1 arg, got {nb_args}"))
    }
    let arg = node.entries().first().unwrap(); // SAFETY: we know there's one!
    arg.value().as_string().ok_or(anyhow!("{directive_name}: Node arg has unexpected type, expected String"))
}

impl TryFrom<KdlNode> for CmdOperation {
    type Error = anyhow::Error;

    fn try_from(node: KdlNode) -> Result<Self, Self::Error> {
        let op = match node.name().value() {
            directive @ "chdir" => {
                check_no_children(&node, directive)?;
                let dir = get_single_arg_string(&node, directive)?;
                CmdOperation::Chdir(dir.into())
            }
            directive @ "init-script" => {
                check_no_args(&node, directive)?;
                let children = get_children(&node, directive)?;
                let interp_node = get_node(children, "interpreter", directive)?;
                let content_node = get_node(children, "content", directive)?;
                let interpreter = get_single_arg_string(interp_node, "init-script > interpreter")?;
                let content = get_single_arg_string(content_node, "init-script > content")?;
                CmdOperation::InitScript {
                    interpreter: interpreter.into(),
                    content: content.into(),
                }
            }
            directive => {
                let type_name = std::any::type_name::<Self>();
                Err(anyhow!("Unhandled directive '{directive}' for {ty}", ty=type_name))?
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
            match node.name().value() {
                "command" => {
                    command = Some(get_single_arg_string(node, "root > command")?);
                },
                "chdir" | "init-script" => {
                    let op = CmdOperation::try_from(node.clone())?;
                    cmd_operations.push(op)
                },
                directive => {
                    // TODO: test error cases!
                    Err(anyhow!("Unknown directive '{directive}' 👀"))?;
                },
            }
        }

        let Some(command) = command.take() else {
            return Err(anyhow!("Missing command!"));
        };

        Ok(BinWrapperConfig {
            command: command.into(),
            env_operations,
            cmd_operations,
            args_operations,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;

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
                },
            }
        }
    }

    #[test]
    fn parse_cmd_op_chdir() {
        let input = r#"
            chdir "foobar"
        "#;
        let node: KdlNode = assert_ok!(input.parse());
        let op = assert_ok!(CmdOperation::try_from(node));
        assert_eq!(op, CmdOperation::Chdir("foobar".into()));
    }

    #[test]
    fn parse_cmd_op_init_script() {
        let input = r#"
            init-script {
                interpreter "my-super-shell"
                content """
                    super duper "content"
                    """
            }
        "#;
        let node: KdlNode = assert_ok!(input.parse());
        let op = assert_ok!(CmdOperation::try_from(node));
        assert_eq!(op, CmdOperation::InitScript {
            interpreter: "my-super-shell".into(),
            content: r#"super duper "content""#.into(),
        });
    }

    #[test]
    fn parse_cmd_wrapper_config() {
        let input = r###"
            command "supercommand!"

            chdir "foobar"

            init-script {
                interpreter "someshell"
                content """
                    echo 'foo!'
                    echo 'and bar!'
                    """
            }
        "###;
        let doc: KdlDocument = assert_ok!(input.parse());
        let wrapper_config = assert_ok!(BinWrapperConfig::try_from(doc));
        assert_eq!(wrapper_config.command, String::from("supercommand!"));
        assert_eq!(wrapper_config.cmd_operations, &[
            CmdOperation::Chdir("foobar".into()),
            CmdOperation::InitScript {
                interpreter: "someshell".into(),
                content: "echo 'foo!'\necho 'and bar!'".into(),
            }
        ]);
    }
}
