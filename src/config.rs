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
pub enum CmdOperation {
    Chdir(String),
    InitScript {
        interpreter: String,
        content: String,
    },
}
