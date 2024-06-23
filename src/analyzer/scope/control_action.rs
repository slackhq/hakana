#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Debug)]
pub enum ControlAction {
    End,
    Break,
    BreakImmediateLoop,
    Continue,
    LeaveSwitch,
    None,
    Return,
}
