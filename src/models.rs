use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct PrinterTestSchema {
    test_page: bool,
    test_line: String,
}

// export const printerStatusSchema = z.object({
//     is_connected: z.boolean(),
//     error: z.string().optional()
// })