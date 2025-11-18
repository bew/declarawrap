use anyhow::anyhow;
use kdl::KdlNode;

use crate::config::CmdOperation;

impl TryFrom<KdlNode> for CmdOperation {
    type Error = anyhow::Error;

    fn try_from(node: KdlNode) -> Result<Self, Self::Error> {
        let op = match node.name().value() {
            "chdir" => {
                let None = node.children() else {
                    return Err(anyhow!("chdir: Node has unexpected children"))
                };
                let nb_args = node.entries().len();
                if nb_args != 1 {
                    return Err(anyhow!("chdir: Node expected 1 arg, got {nb_args}"))
                }
                let arg = node.entries().first().unwrap();
                let arg_str = arg.value().as_string().ok_or_else(|| {
                    anyhow!("chdir: Node arg has unexpected type, expected String")
                })?;
                CmdOperation::Chdir(arg_str.into())
            }
            "init-script" => {
                if node.entries().len() != 0 {
                    return Err(anyhow!("init-script: Node has unexpected arguments/properties"))
                }
                // TODO: check has (only!) expected childrens
                // TODO: extract checked values
                CmdOperation::InitScript { interpreter, content }
            }
            _ => todo!()
        };
        Ok(op)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // TODO: impl custom Error that impl miette::Result!
    #[test]
    fn parse_cmd_op_chdir() -> anyhow::Result<()> {
        let input = r#"
            chdir "foobar"
        "#;
        let node: KdlNode = input.parse()?;
        let res = CmdOperation::try_from(node);
        dbg!(&res);
        let op = res?;
        assert_eq!(op, CmdOperation::Chdir("foobar".into()));
        Ok(())
    }
}
