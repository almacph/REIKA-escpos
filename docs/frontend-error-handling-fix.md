# Print API Error Handling Fix Required

## Problem Summary

The Svelte frontend does not properly handle HTTP error responses (4xx/5xx) from the print API. When a print request fails, the error message from the backend is not displayed to the user.

## Backend API Reference

The Rust backend runs at `http://localhost:55000` and returns JSON responses.

### Response Schema

All endpoints return the same response structure:

```typescript
interface PrinterStatus {
  is_connected: boolean;
  error?: string;  // Present when there's an error
}
```

### Endpoint Behaviors

| Endpoint | Method | Success | Error |
|----------|--------|---------|-------|
| `/print/test` | GET | 200 + `{ is_connected: true }` | 200 + `{ is_connected: false, error: "..." }` |
| `/print/test` | POST | 200 + `{ is_connected: true }` | 400/500 + `{ is_connected: false, error: "..." }` |
| `/print` | POST | 200 + `{ is_connected: true }` | 400/500 + `{ is_connected: false, error: "..." }` |

### Error Status Codes

- `400 Bad Request` - Invalid JSON payload or malformed command
- `500 Internal Server Error` - Printer communication failure, hardware error

## Frontend Issue

The frontend fetch calls do not read the response body when HTTP errors occur.

### Current Pattern (Broken)

```typescript
try {
    const response = await fetch(`${PRINTER_API_BASE}/print`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
    });

    if (response.ok) {
        // Only handles success case
        const status = await fetchPrinterStatus();
        if (status) {
            printerStatus = status;
        }
    }
    // BUG: When response.ok is false, the error response is never read
} catch (error) {
    // Only catches network errors (server unreachable), not HTTP errors
    printerStatus = {
        is_connected: false,
        error: error instanceof Error ? error.message : 'Print request failed'
    };
}
```

### What Goes Wrong

1. Backend returns HTTP 500 with `{ is_connected: false, error: "Printer not responding" }`
2. `response.ok` is `false` (status >= 400)
3. The `if (response.ok)` block is skipped
4. Response body containing the error message is never read
5. `printerStatus` is not updated
6. User sees no error feedback

## Required Fix

Always read and parse the response body. The schema is the same for success and error responses:

```typescript
try {
    const response = await fetch(`${PRINTER_API_BASE}/print`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
    });

    // Always read response body - contains status for both success and error
    const data = await response.json();
    printerStatus = printerStatusSchema.parse(data);
} catch (error) {
    // Network error only - server unreachable
    printerStatus = {
        is_connected: false,
        error: error instanceof Error ? error.message : 'Print request failed'
    };
}
```

## Functions Requiring This Fix

Update these functions in the printer status component:

1. `handleTestLine()` - POST to `/print/test`
2. `jsonCommandsTest()` - POST to `/print`
3. `printTestReceipt()` - POST to `/print`

## Zod Schema Reference

The frontend should already have a compatible schema:

```typescript
const printerStatusSchema = z.object({
    is_connected: z.boolean(),
    error: z.string().optional()
});
```

## Testing

1. Stop the print service or disconnect the printer
2. Attempt any print operation
3. Verify the UI displays the error message from the backend response
4. Verify `printerStatus.error` shows the actual backend error, not a generic message
