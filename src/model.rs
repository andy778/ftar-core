use std::collections::HashMap;

/// Event/gate kinds, using the one-letter codes of the OpenFTA file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Unknown,       // ?
    Basic,         // B
    And,           // A
    Or,            // O
    TransferIn,    // I
    TransferOut,   // T
    Intermediate,  // M
    Undeveloped,   // U
    Inhibit,       // H
    PriorityAnd,   // P
    Xor,           // X
    External,      // E
    ConditionNotAnalysed, // N
    ConditionAnalysed,    // C
    Dormant,       // D
}

impl EventType {
    pub fn from_code(c: char) -> Option<Self> {
        Some(match c {
            '?' => Self::Unknown,
            'B' => Self::Basic,
            'A' => Self::And,
            'O' => Self::Or,
            'I' => Self::TransferIn,
            'T' => Self::TransferOut,
            'M' => Self::Intermediate,
            'U' => Self::Undeveloped,
            'H' => Self::Inhibit,
            'P' => Self::PriorityAnd,
            'X' => Self::Xor,
            'E' => Self::External,
            'N' => Self::ConditionNotAnalysed,
            'C' => Self::ConditionAnalysed,
            'D' => Self::Dormant,
            _ => return None,
        })
    }

    pub fn code(self) -> char {
        match self {
            Self::Unknown => '?',
            Self::Basic => 'B',
            Self::And => 'A',
            Self::Or => 'O',
            Self::TransferIn => 'I',
            Self::TransferOut => 'T',
            Self::Intermediate => 'M',
            Self::Undeveloped => 'U',
            Self::Inhibit => 'H',
            Self::PriorityAnd => 'P',
            Self::Xor => 'X',
            Self::External => 'E',
            Self::ConditionNotAnalysed => 'N',
            Self::ConditionAnalysed => 'C',
            Self::Dormant => 'D',
        }
    }

    pub fn is_gate(self) -> bool {
        matches!(self, Self::And | Self::Or | Self::Xor | Self::PriorityAnd | Self::Inhibit)
    }

    /// Leaf events that carry a probability in the primary event database.
    pub fn is_primary(self) -> bool {
        matches!(self, Self::Basic | Self::Undeveloped | Self::External | Self::Dormant)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: EventType,
    /// Second type character in the file (dormant marker); `None` if absent.
    pub modifier: Option<char>,
    /// Event id; empty when the file says `NULL`.
    pub id: String,
    /// Description. Only stored in the tree file for intermediate events.
    pub text: String,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    /// Name of the associated `.ped` database, if any.
    pub db_name: Option<String>,
    /// Free-text note attached to the tree.
    pub note: String,
    pub root: Node,
}

impl Tree {
    /// Depth-first iterator over all nodes.
    pub fn nodes(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        let mut stack = vec![&self.root];
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(n.children.iter().rev());
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrimaryEvent {
    pub id: String,
    pub group: String,
    pub kind: EventType,
    pub modifier: Option<char>,
    pub description: String,
    pub probability: f64,
    /// Calculation-type marker (`lambda` field in the original source); `None` if absent.
    pub lambda: Option<char>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrimaryEventDb {
    pub events: HashMap<String, PrimaryEvent>,
}

impl PrimaryEventDb {
    pub fn probability(&self, id: &str) -> Option<f64> {
        self.events.get(id).map(|e| e.probability)
    }
}
