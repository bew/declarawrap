#[derive(Debug, PartialEq, Eq)]
pub struct BinWrapperConfig {
    pub command: String,
    pub env_operations: Vec<EnvOperation>,
    pub cmd_operations: Vec<CmdOperation>,
    pub args_operations: Vec<ArgsSequenceUpdate>
}

#[derive(Debug, PartialEq, Eq, Default)]
pub enum EnvIfSetBehavior {
    #[default]
    Replace,
    Error,
    Skip,
}

#[derive(Debug, PartialEq, Eq, Default)]
pub enum EnvIfPresentBehavior {
    #[default]
    Keep,
    Remove,
    Error,
    Skip,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnvOperation {
    ClearAll,
    Set {
        var_name: String,
        value: String,
        if_set: EnvIfSetBehavior,
    },
    Unset { var_name: String },
    // note: for `append` / `prepend`
    SequenceUpdate {
        sep: String,
        value: String,
        if_present: EnvIfPresentBehavior
    },
}

#[derive(Debug, PartialEq, Eq, Default)]
pub enum ArgsTransformation {
    #[default]
    WordSplit,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArgsSequenceUpdate {
    pub items: Vec<String>,
    pub transform: Option<ArgsTransformation>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CmdOperation {
    Chdir(String),
    InitScript {
        interpreter: String,
        content: String,
    },
}
