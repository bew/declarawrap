#[derive(Debug, PartialEq, Eq)]
pub struct BinWrapperConfig {
    pub command: String,
    pub arg0_behavior: Arg0Behavior,
    pub env_operations: Vec<EnvOperation>,
    pub cmd_operations: Vec<CmdOperation>,
    pub args_operations: Vec<ArgsOperation>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnvIfSetBehavior {
    Replace,
    Error,
    Skip,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnvIfInSequenceBehavior {
    Keep,
    Remove,
    Error,
    Skip,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EnvSequenceUpdate {
    pub var_name: String,
    pub sep: String,
    pub value: String,
    pub if_present: EnvIfInSequenceBehavior,
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
    Append(EnvSequenceUpdate),
    Prepend(EnvSequenceUpdate),
    // remove from sequence (?)
    Remove {
        var_name: String,
        sep: String,
        value: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgsTransformation {
    WordSplit,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArgsSequenceUpdate {
    pub arg_strings: Vec<String>,
    pub transformation: Option<ArgsTransformation>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgsOperation {
    Append(ArgsSequenceUpdate),
    Prepend(ArgsSequenceUpdate),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Arg0Behavior {
    Inherit,
    Replace(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum CmdOperation {
    Chdir(String),
    InitScript {
        interpreter: String,
        content: String,
    },
}
