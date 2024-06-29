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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatusResponse {
    pub is_connected: bool,
    pub error: String,
}
// export const printerStatusSchema = z.object({
//     is_connected: z.boolean(),
//     error: z.string().optional()
// })