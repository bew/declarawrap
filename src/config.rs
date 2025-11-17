#[derive(Debug, PartialEq, Eq)]
pub struct BinWrapperConfig {
    env_operations: Vec<EnvOperation>,
    // wrapped: WrappedCmdConfig,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WrappedCmdConfig {
    command: String,
    cmd_operations: Vec<CmdOperation>,
    args_operations: Vec<ArgsSequenceUpdate>
}



#[derive(Debug, PartialEq, Eq)]
pub enum EnvOperation {
    ClearAll,
    Set {
        var_name: String,
        value: String,
        if_set: String, // MAYBE: use enum?
    },
    Unset { var_name: String },
    // note: for `append` / `prepend`
    SequenceUpdate {
        sep: String,
        value: String,
        if_present: String, // MAYBE: use enum?
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArgsSequenceUpdate {
    items: Vec<String>,
    transform: Option<String>, // MAYBE: use enum?
}

#[derive(Debug, PartialEq, Eq)]
struct CmdInitScript {
    #[knus(child, unwrap(argument))]
    interpreter: String,
    #[knus(child, unwrap(argument))]
    content: String,
}

#[derive(Debug, PartialEq, Eq)]
enum CmdOperation {
    Chdir(#[knus(argument)] String),
    // NOTE: `doesn't support inline struct enum variant, need separate struct.
    InitScript(CmdInitScript),
}
