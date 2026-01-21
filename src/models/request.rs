use serde::{Deserialize, Serialize};

use super::command::Command;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrinterTestSchema {
    test_page: bool,
    test_line: String,
}

impl PrinterTestSchema {
    pub fn test_line(&self) -> &str {
        &self.test_line
    }

    pub fn test_page(&self) -> bool {
        self.test_page
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Commands {
    pub commands: Vec<Command>,
}
