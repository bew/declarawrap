// Data model for a wrapper configuration.
//
// Pure data types, with no dependency on the KDL parsing layer: the parsing
// layer lives in `kdl_config_parser` and produces these values.

use std::marker::PhantomData;

// -------------------------------------------------------
// Directives

/// A parsed wrapper configuration.
#[derive(Debug, PartialEq, Eq)]
pub struct BinWrapperConfig {
    pub command: CommandDirective,
    pub arg0_behavior: Arg0Behavior,
    pub env_directives: Vec<EnvDirective>,
    pub cmd_directives: Vec<CmdDirective>,
    pub args_directives: Vec<ArgsDirective>,
}

/// The `command` directive: the executable to wrap.
#[derive(Debug, PartialEq, Eq)]
pub struct CommandDirective {
    pub program: String,
}

/// The `chdir` directive: working directory for the wrapped command.
#[derive(Debug, PartialEq, Eq)]
pub struct ChdirDirective {
    pub target_dir: String,
}

/// The `init-script` directive: an interpreter plus a script body.
#[derive(Debug, PartialEq, Eq)]
pub struct InitScriptDirective {
    pub interpreter: String,
    pub content: String,
}

/// The `env set` directive.
#[derive(Debug, PartialEq, Eq)]
pub struct EnvSetDirective {
    pub var_name: String,
    pub value: String,
    pub if_set: EnvIfSetBehavior,
}

/// The `env unset` directive.
#[derive(Debug, PartialEq, Eq)]
pub struct EnvUnsetDirective {
    pub var_name: String,
}

/// The `env clear` directive.
#[derive(Debug, PartialEq, Eq)]
pub struct EnvClearDirective;

/// An `env prepend`/`env append` directive.
///
/// `D` is a direction marker (`Prepend` or `Append`): the two directions carry
/// identical data, so the direction is encoded in the type rather than a field.
#[derive(Debug, PartialEq, Eq)]
pub struct EnvSequenceUpdate<D> {
    pub var_name: String,
    pub sep: String,
    pub value: String,
    pub if_present: EnvIfInSequenceBehavior,
    pub(crate) _direction: PhantomData<D>,
}

/// An `args prepend`/`args append` directive.
///
/// `D` is a direction marker (`Prepend` or `Append`).
#[derive(Debug, PartialEq, Eq)]
pub struct ArgsSequenceUpdate<D> {
    pub arg_strings: Vec<String>,
    pub transformation: Option<ArgsTransformation>,
    pub(crate) _direction: PhantomData<D>,
}

// -------------------------------------------------------
// Direction markers

/// Type-level marker for a sequence update that prepends.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepend;

/// Type-level marker for a sequence update that appends.
#[derive(Debug, PartialEq, Eq)]
pub struct Append;

// -------------------------------------------------------
// Per-category containers

/// Command-related directives, in textual order.
#[derive(Debug, PartialEq, Eq)]
pub enum CmdDirective {
    Chdir(ChdirDirective),
    InitScript(InitScriptDirective),
}

/// Environment-related directives, in textual order.
#[derive(Debug, PartialEq, Eq)]
pub enum EnvDirective {
    Clear(EnvClearDirective),
    Set(EnvSetDirective),
    Unset(EnvUnsetDirective),
    Prepend(EnvSequenceUpdate<Prepend>),
    Append(EnvSequenceUpdate<Append>),
}

/// Argument-related directives, in textual order.
#[derive(Debug, PartialEq, Eq)]
pub enum ArgsDirective {
    Prepend(ArgsSequenceUpdate<Prepend>),
    Append(ArgsSequenceUpdate<Append>),
}

// -------------------------------------------------------
// Behaviour knobs

/// How `env set` reacts when the variable is already set.
#[derive(Debug, PartialEq, Eq)]
pub enum EnvIfSetBehavior {
    Replace,
    Error,
    Skip,
}

/// How `env prepend`/`env append` reacts when the value is already present.
#[derive(Debug, PartialEq, Eq)]
pub enum EnvIfInSequenceBehavior {
    Keep,
    Remove,
    Error,
    Skip,
}

/// A transformation applied to argument strings.
#[derive(Debug, PartialEq, Eq)]
pub enum ArgsTransformation {
    WordSplit,
}

/// How the wrapped command's `argv[0]` is set.
#[derive(Debug, PartialEq, Eq)]
pub enum Arg0Behavior {
    Inherit,
    Replace(String),
}
