#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Debug)]
pub enum ControlAction {
    End,
    Break,
    Continue,
    LeaveSwitch,
    None,
    Return,
}
