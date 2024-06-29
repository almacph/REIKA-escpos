use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrinterTestSchema {
    test_page: bool,
    test_line: String,
}

impl PrinterTestSchema {
    pub fn test_line(&self) -> &str {
        &self.test_line
    }
    pub fn test_page(&self) -> &bool {
        &self.test_page
    }
}
// export const printerStatusSchema = z.object({
//     is_connected: z.boolean(),
//     error: z.string().optional()
// })